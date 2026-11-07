#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Reads two logs of one call script, one per route, and says where the answers part.

`routes_agree.exe` makes the same calls through the route that asks the real USER32 and through
the route that answers over `Desk`. A difference between the two logs is a finding unless a list
below names it, so those lists are the whole of what this forgives. `LISTED` was named before the
first log was read; `HELD` names, exactly, what stands after that log's causes were read. A listed
difference never seen goes `stale`, and is a red: a stale list clears nothing.

What this reads: each call's `c`, `a`, `s`, `o` and `v`, step by step, in order, and whether all
32 call names were made on each route.

What this does not read:

- A clock. A message's time and `GetMessageTime`'s answer are dropped on both sides, so a `Desk`
  whose time stayed 0 passes here. Whether its clock runs is another check's to say.
- The timer message's `w` and `l` on the `Desk` side: the script posts them itself, with the values
  the real route's log showed, so their agreeing reads neither route.
- A key's messages in `keys.held` and `keys.tap`: both routes drain them unread, so `keys.queue`
  alone compares a key's messages.
- `from`, `by` and `with`: the first and last are one route's alone.
- `Native` under `Desk`, and a browser's key: both routes here run in one program under Wine,
  `Desk` over `Page`.

Each line it prints is `same`, `form`, `clock`, `listed <item>`, `held <item>` or `past <call>
<field> <one answer> <the other>`, then any `stale <item>` and `uncalled <name> <route>`. A `held`
line is a difference standing: the routes still part there, and an exit of 0 does not say they
agree. Exits 0 only on no `past`, no `stale`, no `uncalled` and both logs run to their end.
"""
from __future__ import annotations

import argparse
import difflib
import re
import sys
from dataclasses import dataclass
from pathlib import Path

FIELDS = ("a", "s", "o", "v")
DONE = "routes_agree done"
CLOCK = "clock"
HANDLES = "handles"
THIN = "thin"
DESK = "desk"
CALL_LINE = re.compile(
    r"^imp\.call c=USER32!(?P<name>\S*) a=(?P<a>.*?) s=(?P<s>.*?) with=.*? "
    r"o=(?P<o>.*?) v=(?P<v>.*?) from=.*? by=.*?$"
)
NAMES = frozenset(
    {
        "RegisterClassA", "CreateWindowExA", "DestroyWindow", "FindWindowA", "LoadIconA",
        "ShowWindow", "SetWindowPos", "AdjustWindowRect", "GetWindowRect", "GetWindowLongA",
        "SetWindowLongA", "ClientToScreen", "ScreenToClient", "SetFocus", "GetSystemMetrics",
        "DefWindowProcA", "PeekMessageA", "WaitMessage", "TranslateMessage", "DispatchMessageA",
        "SendMessageA", "PostMessageA", "PostQuitMessage", "GetMessageTime", "GetCursorPos",
        "SetCursorPos", "ShowCursor", "LoadCursorA", "ClipCursor", "GetAsyncKeyState",
        "GetKeyState", "MapVirtualKeyA",
    }
)
# Where a table names a handle. Each route numbers its own by order of first sight, so a wrong
# handle still shows: the 2 routes' orders then disagree.
HANDLE_ANSWERS = frozenset(
    {"CreateWindowExA", "FindWindowA", "LoadIconA", "LoadCursorA", "SetFocus"}
)
WINDOW_FIRST = frozenset(
    {
        "DestroyWindow", "ShowWindow", "SetWindowPos", "GetWindowRect", "GetWindowLongA",
        "SetWindowLongA", "ClientToScreen", "ScreenToClient", "SetFocus", "DefWindowProcA",
        "SendMessageA", "PostMessageA", "PeekMessageA", "TranslateMessage", "DispatchMessageA",
    }
)


@dataclass(frozen=True)
class Listed:
    """One field's difference, by step, call and both exact answers."""

    item: str
    step: str
    call: str
    field: str
    thin: str
    other: str

    def takes(self, step: str, call: str, field: str, thin: str, other: str) -> bool:
        """Whether this is that difference, exactly."""
        return (self.step, self.call, self.field, self.thin, self.other) == (
            step,
            call,
            field,
            thin,
            other,
        )


# The known differences between the routes. With these off (`--list-off`), each goes `past` by
# name, as must the handles, so none of these can be empty.
LISTED = (
    Listed("the tap", "keys.tap", "GetAsyncKeyState", "v", "1", "0"),
    Listed("8001", "keys.held", "GetAsyncKeyState", "v", "8001", "8000"),
    Listed("the timer's window", "timer", "reached", "v", "1", "0"),
)
# Differences standing, each with its cause read and its judge named. Each has its own item, as
# `stale` reads by item. With the list off the fields compare raw, so an entry holding a handle's
# number goes `past` and `stale` there.
HELD: tuple[Listed, ...] = (
    # `RegisterClassA` answers another atom. Both legal: the page gives a class's atom a range and
    # no number (`dataxchg_about-atom-tables.md:84`).
    Listed("the class's atom", "classes", "RegisterClassA", "v", "c02b", "c000"),
    # Translating a `WM_CHAR` answers 1 on the real route, 0 on `Desk`. Not both legal: the page
    # backs `Desk` (`nf-winuser-translatemessage.md:83`), and Wine 9.0 differs from the page; the
    # real route only passes the call on.
    Listed("translating a WM_CHAR", "keys.queue", "TranslateMessage", "v", "1", "0"),
    # A message's `x`, `y` before the program has placed the pointer: each route's start, the real
    # route's the middle of its private screen, `Desk`'s the origin. No page names a start, so both
    # are legal, and these 2 stand.
    Listed(
        "x, y of message 12",
        "messages",
        "PeekMessageA",
        "o",
        "0,12,9,0,clock,200,180",
        "0,12,9,0,clock,0,0",
    ),
    Listed(
        "x, y of message 401",
        "messages",
        "PeekMessageA",
        "o",
        "H3,401,8,9,clock,200,180",
        "H3,401,8,9,clock,0,0",
    ),
)


@dataclass(frozen=True)
class Call:
    """One line of a log, in the form both routes share."""

    step: str
    name: str
    raw: dict[str, str]
    numbered: dict[str, str]
    formed: bool
    clocked: bool

    def key(self) -> tuple[str, str]:
        """What lines two logs up: the name and the arguments, each route's handles numbered."""
        return (self.name, self.numbered["a"])


@dataclass
class Log:
    """A log's calls by step, in order, and whether its program reached its last line."""

    steps: dict[str, list[Call]]
    complete: bool


def to_one_form(route: str, name: str, fields: dict[str, str]) -> bool:
    """Brings a call's fields to the form both routes share. These map values and waive nothing,
    but for `PostQuitMessage`'s `v`, which the thin route's line lacks. Says whether it changed
    anything."""
    numbers = fields["a"].split(",")
    if route == THIN and name == "RegisterClassA":
        fields["a"] = ",".join(numbers[index] for index in (0, 1, 3, 5))
    elif route == THIN and name == "CreateWindowExA":
        fields["a"] = ",".join(numbers[:8] + numbers[9:])
    elif route == DESK and name == "LoadIconA":
        fields["a"] = numbers[0]
        fields["s"] = f"#{int(numbers[1], 16):x}"
    elif name == "PostQuitMessage":
        fields["v"] = ""
    else:
        return False
    return True


def drop_clock(name: str, fields: dict[str, str]) -> bool:
    """Blanks a message's time and `GetMessageTime`'s answer: each route reads its own clock."""
    if name == "GetMessageTime":
        fields["v"] = CLOCK
        return True
    field = {"PeekMessageA": "o", "TranslateMessage": "a", "DispatchMessageA": "a"}.get(name)
    if field is None:
        return False
    numbers = fields[field].split(",")
    if len(numbers) != 7:
        return False
    numbers[4] = CLOCK
    fields[field] = ",".join(numbers)
    return True


def number_handles(name: str, fields: dict[str, str], handles: dict[str, str]) -> dict[str, str]:
    """A copy of the fields with each handle as its route's first sight of it: H1, H2. 0 stays."""

    def numbered(raw: str) -> str:
        return handles.setdefault(raw, f"H{len(handles)}")

    copy = dict(fields)
    if name in WINDOW_FIRST and fields["a"]:
        first, _, rest = fields["a"].partition(",")
        copy["a"] = ",".join(filter(None, (numbered(first), rest)))
    if name == "PeekMessageA" and fields["o"]:
        first, _, rest = fields["o"].partition(",")
        copy["o"] = ",".join(filter(None, (numbered(first), rest)))
    if name in HANDLE_ANSWERS and fields["v"]:
        copy["v"] = numbered(fields["v"])
    return copy


def read_log(path: Path, route: str) -> Log:
    """A log's call lines and `reached` lines, each under its step."""
    steps: dict[str, list[Call]] = {}
    handles = {"0": "0"}
    step = ""
    complete = False
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.rstrip("\r")
        if line.startswith("step "):
            step = line.removeprefix("step ")
            steps.setdefault(step, [])
        elif line.startswith("reached "):
            fields = {"a": "", "s": "", "o": "", "v": line.removeprefix("reached ")}
            steps[step].append(Call(step, "reached", fields, fields, False, False))
        elif line == DONE:
            complete = True
        elif matched := CALL_LINE.match(line):
            name = matched["name"]
            fields = {field: matched[field] for field in FIELDS}
            formed = to_one_form(route, name, fields)
            clocked = drop_clock(name, fields)
            numbered = number_handles(name, fields, handles)
            steps[step].append(Call(step, name, fields, numbered, formed, clocked))
    return Log(steps, complete)


class Score:
    """The running verdict, printed as it goes."""

    def __init__(self, other: str, list_on: bool) -> None:
        self.other = other
        self.list_on = list_on
        self.seen: set[str] = set()
        self.counts = {"same": 0, "form": 0, "clock": 0, "listed": 0, "held": 0, "past": 0}

    def _say(self, kind: str, words: str) -> None:
        self.counts[kind] += 1
        print(f"{kind} {words}".rstrip())

    def difference(self, step: str, call: str, field: str, thin: str, other: str) -> None:
        """One field's two answers, named by a list or past them."""
        for table, kind in ((LISTED if self.list_on else (), "listed"), (HELD, "held")):
            for entry in table:
                if entry.takes(step, call, field, thin, other):
                    self.seen.add(entry.item)
                    self._say(kind, f"{entry.item} {call} {field} {thin} {other} step={step}")
                    return
        self._say("past", f"{call} {field} {thin} {other} step={step}")

    def missing(self, call: Call, side: str) -> None:
        """A line one route wrote and the other did not."""
        self._say(
            "past",
            f"{call.name} line missing-on-{side} a={call.numbered['a']} "
            f"v={call.numbered['v']} step={call.step}",
        )

    def pair(self, thin: Call, other: Call) -> None:
        """A line from each route, field by field."""
        apart = False
        for field in FIELDS:
            if self.list_on:
                answers = (thin.numbered[field], other.numbered[field])
                if answers[0] == answers[1] and thin.raw[field] != other.raw[field]:
                    self.seen.add(HANDLES)
                    self._say("listed", f"{HANDLES} {thin.name} {field} step={thin.step}")
                    apart = True
            else:
                answers = (thin.raw[field], other.raw[field])
            if answers[0] != answers[1]:
                apart = True
                self.difference(thin.step, thin.name, field, *answers)
        if apart:
            return
        if thin.clocked or other.clocked:
            self._say(CLOCK, f"{thin.name} step={thin.step}")
        elif thin.formed or other.formed:
            self._say("form", f"{thin.name} step={thin.step}")
        else:
            self._say("same", f"{thin.name} step={thin.step}")

    def stale(self) -> int:
        """Every listed difference this pair of logs never showed."""
        items = [entry.item for entry in HELD]
        if self.list_on:
            items += [entry.item for entry in LISTED] + [HANDLES]
        unseen = [item for item in items if item not in self.seen]
        for item in unseen:
            print(f"stale {item}")
        return len(unseen)


def read_block(score: Score, thin: list[Call], other: list[Call]) -> None:
    """A run of lines whose arguments disagree: the same names in order are paired, the rest are
    missing lines."""
    names = difflib.SequenceMatcher(
        None, [call.name for call in thin], [call.name for call in other], autojunk=False
    )
    for tag, low, high, other_low, other_high in names.get_opcodes():
        if tag == "equal":
            for one, two in zip(thin[low:high], other[other_low:other_high]):
                score.pair(one, two)
            continue
        for call in thin[low:high]:
            score.missing(call, score.other)
        for call in other[other_low:other_high]:
            score.missing(call, THIN)


def read_step(score: Score, thin: list[Call], other: list[Call]) -> None:
    """One step's lines, lined up by name and arguments, so one missing line moves no other."""
    lines = difflib.SequenceMatcher(
        None, [call.key() for call in thin], [call.key() for call in other], autojunk=False
    )
    for tag, low, high, other_low, other_high in lines.get_opcodes():
        if tag == "equal":
            for one, two in zip(thin[low:high], other[other_low:other_high]):
                score.pair(one, two)
        else:
            read_block(score, thin[low:high], other[other_low:other_high])


def uncalled(log: Log, route: str) -> int:
    """Says each name a route never called, as a call not made answers nothing."""
    called = {call.name for calls in log.steps.values() for call in calls}
    absent = sorted(NAMES - called)
    for name in absent:
        print(f"uncalled {name} {route}")
    return len(absent)


def main() -> int:
    """Scores the two logs named; the exit is the verdict."""
    parser = argparse.ArgumentParser(description=__doc__.partition("\n")[0])
    parser.add_argument("thin", type=Path, help="the log of the route that asks USER32")
    parser.add_argument("other", type=Path, help="the log to read it against")
    parser.add_argument("--other-route", choices=(DESK, THIN), default=DESK)
    parser.add_argument("--list-off", action="store_true", help="the 4 known differences off")
    arguments = parser.parse_args()

    thin = read_log(arguments.thin, THIN)
    other = read_log(arguments.other, arguments.other_route)
    score = Score(arguments.other_route, not arguments.list_off)
    for step in thin.steps.keys() - other.steps.keys():
        print(f"past step {step} missing-on-{arguments.other_route}")
        score.counts["past"] += 1
    for step in other.steps.keys() - thin.steps.keys():
        print(f"past step {step} missing-on-{THIN}")
        score.counts["past"] += 1
    for step, calls in thin.steps.items():
        if step in other.steps:
            read_step(score, calls, other.steps[step])

    stale = score.stale()
    absent = uncalled(thin, THIN) + uncalled(other, arguments.other_route)
    complete = thin.complete and other.complete
    counts = " ".join(f"{kind}={count}" for kind, count in score.counts.items())
    print(f"score {counts} stale={stale} uncalled={absent} complete={complete}")
    return 0 if score.counts["past"] == 0 and stale == 0 and absent == 0 and complete else 1


if __name__ == "__main__":
    sys.exit(main())
