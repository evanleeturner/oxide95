# SPDX-License-Identifier: MIT
"""A second reader of the line grammar in `oxide-logline/src/lib.rs`'s head.

Purpose: judge a run's file by the grammar as written, sharing no code with the crate,
so a fault in the crate's own line cannot hide from its reader.

Call:
    parse_bytes(data) -> Parsed
    parse_file(path) -> Parsed
    python3 logline_grammar.py <file>: one JSON line of counts, for the page's driver

Parsed holds the header's values, the events and the violations. A Violation is a rule
and a line number and never the line, as a log can hold a token. Line 0 means the file
ended before the line the rule wanted.

Does not judge whether a value is the right one, or a clock the right time: the caller
holds those. An event id's characters and the two escapes are not in the head's words:
the workspace's id rule and `body`'s code supply them, each its own rule name.
"""

import json
import logging
import re
import sys
from collections import Counter
from dataclasses import dataclass
from dataclasses import field
from datetime import date
from pathlib import Path

logger = logging.getLogger(__name__)

HEADER_1 = re.compile(
    r"= (?P<program>[^ ]+) run (?P<seconds>[0-9a-f]+)-(?P<pid>[0-9a-f]+) fmt 1 "
    r"date (?P<date>[0-9]{4}-[0-9]{2}-[0-9]{2}) tz utc"
)
HEADER_2 = re.compile(r"= job (?P<job>.+) base (?P<base>.+) cwd (?P<cwd>.+)")
EVENT_LINE = re.compile(
    r"(?P<hour>[01][0-9]|2[0-3]):(?P<minute>[0-5][0-9]):(?P<second>[0-5][0-9])"
    r"\.(?P<milli>[0-9]{3}) (?P<level>[EWIDT]) (?P<rest>.*)",
    re.DOTALL,
)
EVENT_ID = re.compile(r"[a-z]+(?:\.[a-z]+)+")
KEY = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


@dataclass(frozen=True)
class Violation:
    rule: str
    line: int


@dataclass(frozen=True)
class Event:
    line: int
    clock_ms: int
    level: str
    name: str
    fields: tuple[tuple[str, str], ...]


@dataclass
class Parsed:
    program: str | None = None
    run_seconds: int | None = None
    run_pid: int | None = None
    date: str | None = None
    job: str | None = None
    base: str | None = None
    cwd: str | None = None
    events: list[Event] = field(default_factory=list)
    violations: list[Violation] = field(default_factory=list)

    def rule_counts(self) -> Counter[str]:
        """How many violations each rule holds; empty for a file the grammar takes."""
        return Counter(violation.rule for violation in self.violations)


def _read_quoted(text: str, start: int) -> tuple[str | None, int, str]:
    """Reads a quoted value from just past its opening quote.

    Returns the value, the index past the closing quote, and "" on success; on failure
    the value is None and the last item names the rule broken.
    """
    value: list[str] = []
    at = start
    while at < len(text):
        char = text[at]
        if char == '"':
            return "".join(value), at + 1, ""
        if char == "\\":
            if at + 1 >= len(text) or text[at + 1] not in '\\"':
                return None, at, "escape"
            value.append(text[at + 1])
            at += 2
            continue
        value.append(char)
        at += 1
    return None, at, "field"


def parse_fields(text: str) -> tuple[tuple[tuple[str, str], ...] | None, str]:
    """Reads ` key=value` fields to the end of `text`.

    Returns the pairs and "", or None and the rule broken. A bare value may not be empty
    or hold a space, a quote or `=`: the grammar quotes those.
    """
    fields: list[tuple[str, str]] = []
    at = 0
    while at < len(text):
        if text[at] != " ":
            return None, "field"
        key = KEY.match(text, at + 1)
        if key is None or key.end() >= len(text) or text[key.end()] != "=":
            return None, "field"
        at = key.end() + 1
        if at < len(text) and text[at] == '"':
            value, at, rule = _read_quoted(text, at + 1)
            if value is None:
                return None, rule
            if at < len(text) and text[at] != " ":
                return None, "field"
        else:
            end = text.find(" ", at)
            end = len(text) if end < 0 else end
            value = text[at:end]
            if not value or '"' in value or "=" in value:
                return None, "field"
            at = end
        fields.append((key.group(), value))
    return tuple(fields), ""


def _parse_event(text: str, number: int, parsed: Parsed) -> None:
    """Adds one event, or the one violation that stops its line, to `parsed`."""
    line = EVENT_LINE.fullmatch(text)
    if line is None:
        parsed.violations.append(Violation("event-line", number))
        return
    name, _, rest = line["rest"].partition(" ")
    if EVENT_ID.fullmatch(name) is None:
        parsed.violations.append(Violation("event-id", number))
        return
    fields, rule = parse_fields(f" {rest}" if rest else "")
    if fields is None:
        parsed.violations.append(Violation(rule, number))
        return
    clock_ms = (
        (int(line["hour"]) * 3600 + int(line["minute"]) * 60 + int(line["second"]))
        * 1000
    ) + int(line["milli"])
    parsed.events.append(Event(number, clock_ms, line["level"], name, fields))


def _parse_header_1(text: str, parsed: Parsed) -> None:
    header = HEADER_1.fullmatch(text)
    if header is None:
        parsed.violations.append(Violation("header-1", 1))
        return
    try:
        date.fromisoformat(header["date"])
    except ValueError:
        parsed.violations.append(Violation("header-1", 1))
        return
    parsed.program = header["program"]
    parsed.run_seconds = int(header["seconds"], 16)
    parsed.run_pid = int(header["pid"], 16)
    parsed.date = header["date"]


def _parse_header_2(text: str, parsed: Parsed) -> None:
    header = HEADER_2.fullmatch(text)
    if header is None or text.count(" base ") != 1 or text.count(" cwd ") != 1:
        parsed.violations.append(Violation("header-2", 2))
        return
    parsed.job, parsed.base, parsed.cwd = header["job"], header["base"], header["cwd"]


def parse_bytes(data: bytes) -> Parsed:
    """Judges a whole file's bytes. Never raises on what the file holds."""
    parsed = Parsed()
    pieces = data.split(b"\n")
    if pieces[-1] == b"":
        pieces.pop()
    elif data:
        parsed.violations.append(Violation("bytes", len(pieces)))
    if len(pieces) < 1:
        parsed.violations.append(Violation("header-1", 0))
    if len(pieces) < 2:
        parsed.violations.append(Violation("header-2", 0))
    for number, piece in enumerate(pieces, start=1):
        if b"\r" in piece:
            parsed.violations.append(Violation("bytes", number))
        try:
            text = piece.decode("utf-8")
        except UnicodeDecodeError:
            parsed.violations.append(Violation("bytes", number))
            continue
        if number == 1:
            _parse_header_1(text, parsed)
        elif number == 2:
            _parse_header_2(text, parsed)
        else:
            _parse_event(text, number, parsed)
    logger.debug(
        "parsed lines=%d events=%d rules=%s",
        len(pieces),
        len(parsed.events),
        dict(parsed.rule_counts()),
    )
    return parsed


def parse_file(path: Path) -> Parsed:
    """Reads `path` and judges it. Raises OSError when it cannot be read."""
    return parse_bytes(path.read_bytes())


def main() -> int:
    """Prints a file's counts, never a line of it; exits 0 whatever the file holds."""
    parsed = parse_file(Path(sys.argv[1]))
    counts = {"events": len(parsed.events), "rules": dict(parsed.rule_counts())}
    print(json.dumps(counts, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
