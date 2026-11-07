# SPDX-License-Identifier: MIT
"""Reads each listed clip through this crate's `dump` and through ffmpeg's binary, and says where they part.

ffmpeg runs as a program on this computer whatever hosts `dump`, so a build for another host is
judged by the same answer. Each frame is compared by the md5 of its rgb24 bytes at its place, and
a clip's first track by its whole little-endian 16-bit samples. A difference names this crate.

    frames_check.py --dump <file> [--runner <program>] --list <file> --out <file> --scratch <dir>

`--list` holds one clip's path a line. Every path comes from the caller: neither this script nor
`dump` names a clip, and no clip, track or frame belongs in this tree. `--out` takes a line a clip,
by its file's name, so it stays outside the tree too. `--scratch` takes each clip's samples for as
long as that clip is read.

What this reads: `dump`'s frames and samples. What it does not read: the pace a clip plays at, a
front over this crate, a screen or a speaker. Agreeing with ffmpeg shows the two decoders agree on
these clips, and nothing about a clip not listed.

Exits 0 when every clip is clean, 1 when one is not.
"""

import argparse
import concurrent.futures
import hashlib
import struct
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

HEADER_BYTES = 104
SIZES_AT = 4
TRACK_WORD_AT = 72
HAS_TRACK = 1 << 30
RGB_BYTES = 3
KEPT_ERROR_BYTES = 200


@dataclass
class Row:
    """One clip's answers from the two decoders."""

    at: int
    name: str
    frames: int
    ours: int
    theirs: int
    wrong: int
    first: int
    spare: int
    code: int
    our_error: str
    their_error: str
    track: bool = False
    sound_ours: int = 0
    sound_theirs: int = 0
    sound_same: bool = False
    sound_first: int = -1
    sound_md5: str = ""

    def apart(self) -> int:
        """Frames apart from ffmpeg, a frame either side lacks included, and 1 for a track that differs."""
        return self.wrong + abs(self.ours - self.theirs) + int(self.track and not self.sound_same)

    def clean(self) -> bool:
        """Nothing apart, and both decoders read the header's count with bytes left over from neither."""
        whole = self.ours == self.frames == self.theirs
        return self.apart() == 0 and whole and self.code == 0 and self.spare == 0

    def line(self) -> str:
        """The row as one line of `--out`, its two free texts last."""
        return (
            f"at={self.at} clip={self.name} frames={self.frames} ours={self.ours} "
            f"theirs={self.theirs} wrong={self.wrong} first={self.first} spare={self.spare} "
            f"code={self.code} track={int(self.track)} sound_ours={self.sound_ours} "
            f"sound_theirs={self.sound_theirs} sound_same={int(self.sound_same)} "
            f"sound_first={self.sound_first} sound_md5={self.sound_md5} "
            f"err={self.our_error!r} their_err={self.their_error!r}"
        )


def kept_text(raw: bytes) -> str:
    """The head of a program's error output, as text."""
    return raw.decode(errors="replace").strip()[:KEPT_ERROR_BYTES]


def their_frames(ffmpeg: str, clip: Path) -> tuple[list[str], str]:
    """ffmpeg's md5 of each frame, in order, and what it said on its error output."""
    run = subprocess.run(
        [ffmpeg, "-v", "error", "-nostdin", "-i", str(clip), "-map", "0:v:0"]
        + ["-f", "framemd5", "-pix_fmt", "rgb24", "-"],
        capture_output=True,
        check=False,
    )
    lines = [line for line in run.stdout.decode().splitlines() if line and not line.startswith("#")]
    return [line.split(",")[-1].strip() for line in lines], kept_text(run.stderr)


def their_track(ffmpeg: str, clip: Path) -> tuple[bytes, str]:
    """ffmpeg's whole first track as little-endian 16-bit, and what it said on its error output."""
    run = subprocess.run(
        [ffmpeg, "-v", "error", "-nostdin", "-i", str(clip), "-map", "0:a:0"]
        + ["-f", "s16le", "-c:a", "pcm_s16le", "-"],
        capture_output=True,
        check=False,
    )
    return run.stdout, kept_text(run.stderr)


def our_frames(
    command: list[str], clip: Path, frame_bytes: int, folder: Path
) -> tuple[list[str], int, int, str, bytes]:
    """`dump`'s md5 of each frame, the bytes past its last whole frame, its code, its error output and samples.

    Its error output goes to a file, so a talkative host cannot fill a pipe nobody is reading.
    """
    samples = folder / "samples"
    errors = folder / "errors"
    sums = []
    spare = 0
    with errors.open("wb") as error_file:
        process = subprocess.Popen(
            [*command, "frames", str(clip), str(samples)],
            stdout=subprocess.PIPE,
            stderr=error_file,
        )
        while frame_bytes:
            frame = process.stdout.read(frame_bytes)
            if len(frame) < frame_bytes:
                spare = len(frame)
                break
            sums.append(hashlib.md5(frame).hexdigest())
        process.stdout.close()
        code = process.wait()
    sound = samples.read_bytes() if samples.exists() else b""
    return sums, spare, code, kept_text(errors.read_bytes()), sound


def first_apart(ours: list | bytes, theirs: list | bytes) -> int:
    """The first place the two differ, or -1 where neither does."""
    return next((at for at, (a, b) in enumerate(zip(ours, theirs)) if a != b), -1)


def read_clip(command: list[str], ffmpeg: str, scratch: Path, at: int, clip: Path) -> Row:
    """One clip through both decoders."""
    with clip.open("rb") as file:
        head = file.read(HEADER_BYTES)
    width, height, frames = struct.unpack_from("<III", head, SIZES_AT)
    has_track = bool(struct.unpack_from("<I", head, TRACK_WORD_AT)[0] & HAS_TRACK)
    theirs, their_error = their_frames(ffmpeg, clip)
    with tempfile.TemporaryDirectory(dir=scratch) as folder:
        ours, spare, code, our_error, sound = our_frames(command, clip, width * height * RGB_BYTES, Path(folder))
    row = Row(
        at=at,
        name=clip.name,
        frames=frames,
        ours=len(ours),
        theirs=len(theirs),
        wrong=sum(a != b for a, b in zip(ours, theirs)),
        first=first_apart(ours, theirs),
        spare=spare,
        code=code,
        our_error=our_error,
        their_error=their_error,
        track=has_track,
    )
    if has_track:
        track, track_error = their_track(ffmpeg, clip)
        row.their_error = row.their_error or track_error
        row.sound_ours = len(sound)
        row.sound_theirs = len(track)
        row.sound_same = sound == track
        row.sound_first = first_apart(sound, track)
        row.sound_md5 = hashlib.md5(sound).hexdigest()
    return row


def summary(rows: list[Row]) -> str:
    """The run's counts, as the last line of `--out`."""
    tracks = [row for row in rows if row.track]
    return (
        f"clips {len(rows)} frames {sum(row.frames for row in rows)} "
        f"differences {sum(row.apart() for row in rows)} "
        f"clips_clean {sum(row.clean() for row in rows)} tracks {len(tracks)} "
        f"tracks_same {sum(row.sound_same for row in tracks)} "
        f"refused {sum(row.code != 0 for row in rows)} "
        f"panics {sum('panicked' in row.our_error for row in rows)}"
    )


def main() -> int:
    """Read the list, write a line a clip and the counts, and answer whether every clip was clean."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dump", type=Path, required=True, help="the crate's `dump` example")
    parser.add_argument("--runner", help="a program that hosts `dump`, as `wine`")
    parser.add_argument("--list", type=Path, required=True, help="a file of one clip's path a line")
    parser.add_argument("--out", type=Path, required=True, help="a file, outside this tree")
    parser.add_argument("--scratch", type=Path, required=True, help="a folder for samples")
    parser.add_argument("--ffmpeg", default="ffmpeg", help="the judge, run on this computer")
    parser.add_argument("--jobs", type=int, default=4, help="clips read at once")
    args = parser.parse_args()

    command = [args.runner, str(args.dump)] if args.runner else [str(args.dump)]
    clips = [Path(line.strip()) for line in args.list.read_text().splitlines() if line.strip()]
    version = subprocess.run([args.ffmpeg, "-version"], capture_output=True, text=True, check=True).stdout.split()[2]
    digest = hashlib.sha256(args.dump.read_bytes()).hexdigest()
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        rows = list(
            pool.map(
                lambda place: read_clip(command, args.ffmpeg, args.scratch, *place),
                enumerate(clips),
            )
        )
    counts = summary(rows)
    with args.out.open("w") as out:
        out.write(f"# dump path={args.dump} sha256={digest} runner={args.runner or 'none'}\n")
        out.write(f"# ffmpeg path={args.ffmpeg} version={version}\n")
        for row in rows:
            out.write(row.line() + "\n")
        out.write(f"# {counts}\n")
    print(counts)
    if all(row.clean() for row in rows):
        return 0
    print("a frame or track differs from ffmpeg's: the suspect is video-smacker", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
