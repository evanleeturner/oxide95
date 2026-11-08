#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Scores what `page/clip-driver.mjs` wrote, and a process's refusals, by judges outside the crate.

ffmpeg's md5 per frame judges the frames and the canvas; a table written by hand before any run
judges the refusals; ffprobe's times judge the pace, against a stopwatch held in the driver. A
crate's own answer judges nothing here: the crate's lateness is printed and given no line. Every
row prints; one failed row makes the exit 1, and the run's answer is that exit.
"""
# `VS3`: a built clip's frames in the page's wasm, `seek` included, against ffmpeg's md5.
# `VS4`: hand-cut clips refused by name, on the page and under Wine, against the table.
# `VF2`: the page's pace: `smack_wait`'s due times against the stopwatch and ffprobe's times.
# `VF4`: a clip's frame placed by `Place` through the drawing page to the canvas, against ffmpeg.
import argparse
import json
import re
import subprocess
from dataclasses import asdict, dataclass
from pathlib import Path

PASS, FAIL, TOLD, READ, VOID = "pass", "FAIL", "told", "read", "void"
RUNS_WANTED = 3
STREAM_BINARIES = re.compile(r"chrome|firefox|wine|dump|node", re.IGNORECASE)
# The page must never open sound. The driver and the shim are read for this word.
NO_SOUND_WORD = "AudioContext"
# Rows of `f2-names.txt` a check names: crate, file, kind, name, line, held, checks, weight.
NAME_ROW_FIELDS = 8
NAMES_WANTED = {"VS3": 2, "VS4": 31, "VF4": 5, "VF2": 5}
# `Part`'s variants stand above this line of `refused.rs`, and `Refused`'s below it.
LAST_PART_LINE = 23


@dataclass
class Row:
    """One line of the score: what was read, where, and what it came to."""

    row: str
    where: str
    verdict: str
    value: str


def verdict(ok: bool) -> str:
    """A row's word for a line met or missed."""
    return PASS if ok else FAIL


def their_frames(ffmpeg: str, clip: Path) -> list[str]:
    """ffmpeg's md5 of each frame's rgb24, in order."""
    run = subprocess.run(
        [ffmpeg, "-v", "error", "-nostdin", "-i", str(clip), "-map", "0:v:0"]
        + ["-f", "framemd5", "-pix_fmt", "rgb24", "-"],
        capture_output=True,
        check=True,
    )
    lines = [line for line in run.stdout.decode().splitlines() if line and not line.startswith("#")]
    return [line.split(",")[-1].strip() for line in lines]


def their_times(ffprobe: str, clip: Path) -> list[int]:
    """ffprobe's time of each frame, in microseconds."""
    run = subprocess.run(
        [ffprobe, "-v", "error", "-select_streams", "v:0", "-show_entries", "frame=pts_time"]
        + ["-of", "csv=p=0", str(clip)],
        capture_output=True,
        check=True,
    )
    return [round(float(line.strip(" ,")) * 1_000_000) for line in run.stdout.decode().splitlines() if line]


def read_table(path: Path) -> dict[str, str]:
    """The hand-written table of clip and name."""
    pairs = (line.split() for line in path.read_text().splitlines() if line and not line.startswith("#"))
    return {clip: name for clip, name in pairs}


def alike(md5s: list[str]) -> list[list[int]]:
    """The frame numbers that share a hash, each group of 2 or more."""
    groups: dict[str, list[int]] = {}
    for frame, md5 in enumerate(md5s):
        groups.setdefault(md5, []).append(frame)
    return [group for group in groups.values() if len(group) > 1]


def score_preflight(rows: list[Row], theirs: dict[str, list[str]]) -> None:
    """Says which seek clips can tell every frame apart, by ffmpeg's hashes alone."""
    for clip, md5s in theirs.items():
        pairs = alike(md5s)
        rows.append(
            Row("vs3_preflight", clip, TOLD if pairs else PASS, f"{len(md5s)} frames, alike and so untold {pairs}")
        )
    if all(alike(md5s) for md5s in theirs.values()):
        rows.append(Row("vs3_preflight_any", "all", FAIL, "no clip tells all its frames apart"))


def score_seek(rows: list[Row], engine: str, seek: dict, theirs: dict[str, list[str]]) -> None:
    """Each frame read against ffmpeg's frame of that number, in each order of reading."""
    traps = 0
    for clip, answers in seek.items():
        their = theirs[clip]
        count = len(their)
        wanted = {
            "vs3_plain": ("plain", list(range(count))),
            "vs3_seek_forward": ("forward", list(range(count))),
            "vs3_seek_rewind": ("rewind", [*range(count - 1, 0, -1), 2]),
            "vs3_seek_own": ("own", list(range(count))),
        }
        for name, (kind, frames) in wanted.items():
            answer = answers[kind]
            traps += 1 if answer.get("trap") else 0
            reads = answer["reads"]
            off = [read["frame"] for read in reads if read["md5"] != their[read["frame"]]]
            ok = [read["frame"] for read in reads] == frames and not off and answer.get("frames") == count
            rows.append(Row(name, f"{engine} {clip}", verdict(ok), f"read {len(reads)} of {len(frames)}, off {off}"))
        past = answers["past"]
        traps += 1 if past.get("trap") else 0
        reads = past["reads"]
        ok = (
            past.get("answer") == "past-end"
            and past.get("before") == past.get("after")
            and len(reads) == 2
            and reads[1]["md5"] == their[past["before"]]
        )
        rows.append(
            Row(
                "vs3_seek_past",
                f"{engine} {clip}",
                verdict(ok),
                f"{past.get('answer')}, cursor {past.get('before')} to {past.get('after')}",
            )
        )
    rows.append(Row("vs3_traps", engine, verdict(traps == 0), str(traps)))


def score_refuse(rows: list[Row], engine: str, refuse: dict, table: dict[str, str]) -> None:
    """Each cut's name, its one line, its traps and its memory, against the table."""
    missing = sorted(set(table) ^ set(refuse))
    wrong = {clip: refuse[clip]["name"] for clip in table if clip in refuse and refuse[clip]["name"] != table[clip]}
    rows.append(Row("vs4_name", engine, verdict(not missing and not wrong), f"wrong {wrong}, unmatched {missing}"))
    lines_off = []
    for clip, row in refuse.items():
        said = [line for line in row["lines"].splitlines() if "smk.refused" in line]
        names = [match.group(1) for line in said if (match := re.search(r"\bw=(\S+)", line))]
        if names != [table.get(clip)]:
            lines_off.append(clip)
    rows.append(Row("vs4_line", engine, verdict(not lines_off), f"off {lines_off}"))
    traps = sorted(clip for clip, row in refuse.items() if row["trap"])
    rows.append(Row("vs4_traps", engine, verdict(not traps), f"{len(traps)} {traps}"))
    large = [clip for clip, name in table.items() if name == "too-large" and clip in refuse]
    grew = [clip for clip in large if refuse[clip]["before"] != refuse[clip]["after"]]
    rows.append(Row("vs4_memory", engine, verdict(bool(large) and not grew), f"{len(large)} clips, grew {grew}"))


def kebab(name: str) -> str:
    """A variant's name as `Refused::name` words it."""
    return re.sub(r"(?<=[a-z0-9])([A-Z])", r"-\1", name).lower()


def debug_name(error: str) -> str:
    """The name in a process's `Error: <Debug>` text."""
    match = re.match(r"Error: ([A-Za-z0-9]+)(?:\(([A-Za-z]+)\))?", error)
    if match is None:
        return "no-error-line"
    return f"{kebab(match.group(1))}-{match.group(2).lower()}" if match.group(2) else kebab(match.group(1))


def score_process(rows: list[Row], path: Path, table: dict[str, str]) -> set[str]:
    """Each cut's name from a process's error line; a panic's exit counts apart from a name."""
    answers: dict[str, str] = {}
    exits: dict[str, int] = {}
    for line in path.read_text().splitlines():
        clip, code, *error = line.split(" ", 2)
        answers[clip] = debug_name(error[0] if error else "")
        exits[clip] = int(code)
    missing = sorted(set(table) ^ set(answers))
    wrong = {clip: answers[clip] for clip in table if clip in answers and answers[clip] != table[clip]}
    rows.append(Row("vs4_wine_name", "wine", verdict(not missing and not wrong), f"wrong {wrong}, unmatched {missing}"))
    faults = {clip: code for clip, code in exits.items() if code != 1}
    rows.append(Row("vs4_wine_traps", "wine", verdict(not faults), f"{len(faults)} {faults}"))
    rows.append(Row("vs4_wine_past_end", "wine", TOLD, "`dump` never seeks, so this host does not reach it"))
    return set(answers.values())


def score_screen(rows: list[Row], engine: str, screen: dict, their: list[str]) -> None:
    """The canvas's rows against ffmpeg's rgb24; the layout must read every name of `Place`."""
    opened = screen["opened"]
    frames = screen.get("frames", [])
    laid = (
        opened["width"] % 4 != 0
        and opened["x"] != 0
        and opened["y"] != 0
        and opened["pitchMine"] != opened["clipWidth"]
        and opened["y"] + opened["clipHeight"] == opened["height"]
    )
    rows.append(Row("vf4_layout", engine, verdict(laid), str(opened)))
    rows.append(
        Row("screen_pitch", engine, verdict(opened["pitchMine"] == opened["pitchRead"]), str(opened["pitchRead"]))
    )
    unread = [at for at, frame in enumerate(frames) if not frame["read"]]
    rows.append(Row("vf4_read", engine, verdict(len(frames) == len(their) and not unread), f"false at {unread}"))
    for name, key in (("vf4_rgba", "rgba"), ("vf4_canvas", "canvas"), ("vf4_shot", "shot")):
        off = [at for at, frame in enumerate(frames) if frame[key].get("md5") != their[at]]
        rows.append(Row(name, engine, verdict(len(frames) == len(their) and not off), f"off {off}"))
    index_off = [frame["marginIndexOff"] for frame in frames]
    rows.append(Row("vf4_margin_index", engine, verdict(not any(index_off)), str(index_off)))
    colours = [frame["canvas"]["marginColours"] for frame in frames]
    rows.append(Row("vf4_margin_canvas", engine, verdict(all(count == 1 for count in colours)), str(colours)))
    alpha = [frame[key].get("alphaOff") for frame in frames for key in ("rgba", "canvas", "shot")]
    rows.append(Row("vf4_alpha", engine, verdict(not any(alpha)), f"{sum(value or 0 for value in alpha)} off"))


def pace_sums(run: dict, their: list[int]) -> dict[str, float]:
    """One run's rows from the stopwatch's stamps, each from `open`'s instant."""
    stamps = run["stamps"]
    opened = next(stamp for stamp in stamps if stamp[1] == "open")
    dues = [stamp for stamp in stamps if stamp[1] == "due"]
    lates = [due[0] - opened[0] for due in dues]
    beats = [stamp[0] for stamp in stamps if stamp[1] == "beat" and stamp[0] >= dues[0][0]]
    gaps = [later - earlier for earlier, later in zip(beats, beats[1:])]
    pairs = list(zip(lates, their))
    intervals = [abs((b - a) - (their_b - their_a)) for (a, their_a), (b, their_b) in zip(pairs, pairs[1:])]
    lags = [(stamp[0] - opened[0]) - (stamp[2] - opened[2]) * 1000 for stamp in stamps if stamp[0] >= opened[0]]
    return {
        "frames_theirs": len(dues) - len(their),
        "first_late_us": lates[0],
        "worst_late_us": max(abs(late - time) for late, time in pairs),
        "longest_gap_us": max(gaps, default=0),
        "frames_no_not_due": sum(1 for due in dues[1:] if due[4] == 0),
        "due_steps_off": sum(1 for due, time in zip(dues, their) if due[5] != time),
        "interval_worst_us": max(intervals, default=0),
        "lag_spread_us": max(lags) - min(lags),
        "crate_worst_late_us": max(due[6] for due in dues),
        "waits": run["waits"],
        "clock_reads": run["clockReads"],
        "reanchors": run["reanchors"],
    }


def score_pace(rows: list[Row], engine: str, pace: dict, times: dict[str, list[int]]) -> None:
    """Each run read, never an average; the tolerance's reference is one period, read off ffprobe."""
    rows.append(Row("vf2_clock_step_us", engine, READ, f"{pace['clockStepUs']:.1f}"))
    for clip, runs in pace["clips"].items():
        their = times[clip]
        period = their[1] - their[0]
        rows.append(Row("vf2_period_us", f"{engine} {clip}", READ, str(period)))
        lines = {
            "first_late_us": period / 2,
            "worst_late_us": period,
            "longest_gap_us": period / 2,
            "frames_no_not_due": 0,
            "due_steps_off": 0,
            "frames_theirs": 0,
        }
        clean = 0
        for number, run in enumerate(runs):
            where = f"{engine} {clip} run{number}"
            if "stamps" not in run or "reanchors" not in run:
                rows.append(Row("vf2_run", where, FAIL, f"refused {run.get('refused')}"))
                continue
            sums = pace_sums(run, their)
            is_clean = run["reanchors"] == 0
            clean += 1 if is_clean else 0
            for name, value in sums.items():
                if name in lines:
                    met = abs(value) <= lines[name]
                    word = verdict(met) if is_clean else VOID
                    rows.append(Row(f"vf2_{name}", where, word, f"{value:.0f} line {lines[name]:.0f}"))
                else:
                    rows.append(Row(f"vf2_{name}", where, READ if is_clean else VOID, f"{value:.0f}"))
        rows.append(Row("vf2_clean_runs", f"{engine} {clip}", verdict(clean >= RUNS_WANTED), f"{clean} of {len(runs)}"))


def score_streams(rows: list[Row], streams: Path, marks: Path) -> None:
    """The real server's streams, read from outside: any new one of a browser's or Wine's is a stop."""
    reads: list[float] = []
    seen: dict[str, tuple[float, str]] = {}
    failed = 0
    for line in streams.read_text().splitlines():
        kind, *rest = line.split()
        if kind == "read":
            reads.append(float(rest[0]))
        elif kind == "fail":
            failed += 1
        elif kind == "stream":
            seen.setdefault(rest[1], (float(rest[0]), rest[2] if len(rest) > 2 else "?"))
    rows.append(Row("streams_reads", "outside", verdict(bool(reads) and not failed), f"{len(reads)}, failed {failed}"))
    opened = {}
    for line in marks.read_text().splitlines():
        at, word, name, *_ = line.split()
        opened.setdefault(name, {})[word] = float(at)
    if reads:
        for name, span in sorted(opened.items()):
            before = sum(1 for at in reads if at < span["start"])
            during = sum(1 for at in reads if span["start"] <= at <= span.get("end", span["start"]))
            after = sum(1 for at in reads if at > span.get("end", span["start"]))
            ok = before > 0 and during > 0 and after > 0
            rows.append(Row("streams_around", f"outside {name}", verdict(ok), f"{before}, {during}, {after}"))
    first = reads[0] if reads else 0.0
    ours = {stream: binary for stream, (at, binary) in seen.items() if at > first and STREAM_BINARIES.search(binary)}
    standing = len(seen) - len(ours)
    rows.append(Row("streams_ours", "outside", verdict(not ours), f"new {ours}, others standing {standing}"))


def score_word(rows: list[Row], paths: list[Path]) -> None:
    """The page's own scripts never name the thing that opens sound."""
    count = sum(path.read_text().count(NO_SOUND_WORD) for path in paths)
    rows.append(Row("streams_word", "driver, shim", verdict(count == 0), str(count)))


def name_answer(kind: str, name: str, line: int) -> str | None:
    """The answer that holds a VS4 name, or None when any answer holds it."""
    if kind in ("enum", "fn") or name == "Truncated":
        return None
    if line <= LAST_PART_LINE:
        return f"truncated-{name.lower()}"
    return kebab(name)


def score_names(rows: list[Row], path: Path, answers: dict[str, set[str]], paced: bool) -> None:
    """The names each check holds, counted from the census's rows."""
    named: dict[str, list[list[str]]] = {check: [] for check in NAMES_WANTED}
    for line in path.read_text().splitlines():
        fields = line.split()
        if len(fields) != NAME_ROW_FIELDS:
            continue
        for check in fields[6].split(","):
            if check in named:
                named[check].append(fields)
    for check, wanted in NAMES_WANTED.items():
        rows.append(Row("names_counted", check, verdict(len(named[check]) == wanted), f"{len(named[check])} of {wanted}"))
    for host, said in answers.items():
        unheld = []
        for fields in named["VS4"]:
            answer = name_answer(fields[2], fields[3], int(fields[4]))
            if answer is not None and answer not in said:
                unheld.append(f"{fields[3]}:{fields[4]}")
        rows.append(Row("vs4_names_held", host, TOLD if unheld else PASS, f"unheld {unheld}"))
    rows.append(Row("vf2_names_held", "pages", verdict(paced), "by clock reads and waits above 0; no wait line in this arm"))
    rows.append(Row("vf4_names_held", "pages", TOLD, "by the plants moving, which the plants' own score holds"))


def main() -> int:
    """Scores the engines' files found in `--runs`; rows go to the output and `--json`."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", type=Path, required=True)
    parser.add_argument("--probes", type=Path, required=True)
    parser.add_argument("--paced", type=Path, required=True)
    parser.add_argument("--table", type=Path, required=True)
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--shim", type=Path, required=True)
    parser.add_argument("--names", type=Path)
    parser.add_argument("--wine", type=Path)
    parser.add_argument("--streams", type=Path)
    parser.add_argument("--json", type=Path)
    parser.add_argument("--ffmpeg", default="ffmpeg")
    parser.add_argument("--ffprobe", default="ffprobe")
    args = parser.parse_args()

    rows: list[Row] = []
    table = read_table(args.table)
    runs = {path.stem: json.loads(path.read_text()) for path in sorted(args.runs.glob("*.json"))}
    clips = {clip for run in runs.values() for clip in run.get("seek", {})}
    clips |= {"palette"} if any("screen" in run for run in runs.values()) else set()
    theirs = {clip: their_frames(args.ffmpeg, args.probes / f"{clip}.smk") for clip in sorted(clips)}
    paced = {clip for run in runs.values() for clip in run.get("pace", {}).get("clips", {})}
    times = {clip: their_times(args.ffprobe, args.paced / f"{clip}.smk") for clip in sorted(paced)}

    score_preflight(rows, theirs)
    answers: dict[str, set[str]] = {}
    for engine, run in runs.items():
        rows.append(Row("wasm", engine, READ, f"{run['version']} {run['wasmMd5']}"))
        said = set()
        if "seek" in run:
            score_seek(rows, engine, run["seek"], theirs)
            said |= {clip["past"].get("answer") for clip in run["seek"].values()}
        if "refuse" in run:
            score_refuse(rows, engine, run["refuse"], table)
            answers[engine] = said | {row["name"] for row in run["refuse"].values()}
        if "screen" in run:
            score_screen(rows, engine, run["screen"], theirs["palette"])
        if "pace" in run:
            score_pace(rows, engine, run["pace"], times)
    if args.wine:
        answers["wine"] = score_process(rows, args.wine, table)
    if args.streams:
        score_streams(rows, args.streams, args.runs / "marks.txt")
    score_word(rows, [args.driver, args.shim])
    if args.names:
        moved = all(
            run["waits"] > 0 and run["clockReads"] > 0
            for one in runs.values()
            for clip in one.get("pace", {}).get("clips", {}).values()
            for run in clip
        )
        score_names(rows, args.names, answers, moved and bool(paced))

    for row in rows:
        print(f"{row.row:22} {row.where:30} {row.verdict:5} {row.value}"[:240])
    failed = [row for row in rows if row.verdict == FAIL]
    print(f"rows {len(rows)}, failed {len(failed)}")
    if args.json:
        args.json.write_text(json.dumps([asdict(row) for row in rows], indent=0))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
