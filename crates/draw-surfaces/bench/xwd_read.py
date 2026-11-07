#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Read an `xwd` dump's pixels by its own header, so a picture is judged by a script.

A dump says how wide a pixel is and where each colour sits in it. Each colour comes back
as 8 bits a channel, its low bits zero where the screen keeps fewer, so a picture that lost
bits reads different from the one asked for. A header this reader cannot take is refused,
never guessed at: a green read of a false reader is the risk `xwd_row.py` carries.

A header can still be false, and a wrong count from one looks like a real red. Measured on a
private 16-bit screen under Wine: `xwd` wrote 24-bit pixels under 8-bit masks, each holding the
screen's own 5-6-5 word in its low 16 bits. So `read_for_screen` takes the depth from whoever
started the screen, which no header can overrule.

Usage: xwd_read.py <file>; prints the header's sides and the commonest colours.
"""

from __future__ import annotations

import collections
import dataclasses
import pathlib
import struct
import sys

HEADER_WORDS = 25
HEADER_BYTES = HEADER_WORDS * 4
COLOUR_ENTRY_BYTES = 12
Z_PIXMAP = 2
TRUE_COLOUR = 4
DIRECT_COLOUR = 5
PIXEL_BITS = (16, 24, 32)
CHANNEL_BITS = 8
COLOUR_BITS = 24
COLOUR_ENTRY_FORM = ">IHHHBB"
TABLE_ENTRIES = 256
TABLE_PIXEL_STEP = 0x010101
TABLE_COLOUR_STEP = 257
SHALLOW_DEPTH = 16
SHALLOW_MASKS = (0xF800, 0x07E0, 0x001F)
SHALLOW_WORD_MAX = 0xFFFF


class RefusedError(ValueError):
    """The dump's header cannot be read without guessing."""


@dataclasses.dataclass(frozen=True)
class Dump:
    """One dump's pixels and the header words that say how to read them."""

    width: int
    height: int
    depth: int
    bits: int
    line: int
    big_endian: bool
    masks: tuple[int, int, int]
    data: bytes
    start: int

    def word(self, x: int, y: int) -> int:
        """The whole stored value of the pixel at a place."""
        step = self.bits // 8
        at = self.start + y * self.line + x * step
        order = "big" if self.big_endian else "little"
        return int.from_bytes(self.data[at : at + step], order)

    def pixel(self, x: int, y: int) -> tuple[int, int, int]:
        """Red, green and blue at a place, each as 8 bits."""
        value = self.word(x, y)
        red, green, blue = (_channel(value, mask) for mask in self.masks)
        return red, green, blue


def _channel(value: int, mask: int) -> int:
    shift = (mask & -mask).bit_length() - 1
    return ((value & mask) >> shift) << (CHANNEL_BITS - mask.bit_count())


def _check_masks(masks: tuple[int, int, int], depth: int, bits: int) -> None:
    """Refuse masks that do not describe every colour bit of a pixel this wide."""
    joined = 0
    for mask in masks:
        if mask == 0:
            raise RefusedError(f"a colour with no mask: {masks}")
        shifted = mask >> ((mask & -mask).bit_length() - 1)
        if shifted & (shifted + 1):
            raise RefusedError(f"a mask with a gap: {mask:#x}")
        if mask.bit_count() > CHANNEL_BITS:
            raise RefusedError(f"a mask wider than 8 bits: {mask:#x}")
        if joined & mask:
            raise RefusedError(f"masks that overlap: {masks}")
        joined |= mask
    if joined >> bits:
        raise RefusedError(f"masks past a {bits}-bit pixel: {masks}")
    if joined.bit_count() != min(depth, COLOUR_BITS):
        raise RefusedError(f"masks of {joined.bit_count()} bits at depth {depth}: {masks}")


def _check_direct_table(data: bytes, start: int, colours: int) -> None:
    """Refuse a DirectColor dump unless its table maps each value to itself, as measured.

    DirectColor keeps its colours in a table, so masks alone are true only under this one.
    """
    if colours != TABLE_ENTRIES:
        raise RefusedError(f"a DirectColor table of {colours} entries")
    for index in range(colours):
        at = start + index * COLOUR_ENTRY_BYTES
        if at + COLOUR_ENTRY_BYTES > len(data):
            raise RefusedError(f"{len(data)} bytes hold no table of {colours}")
        pixel, red, green, blue, _, _ = struct.unpack_from(COLOUR_ENTRY_FORM, data, at)
        colour = index * TABLE_COLOUR_STEP
        if pixel != index * TABLE_PIXEL_STEP or not red == green == blue == colour:
            raise RefusedError(f"a DirectColor table that turns colours, at entry {index}")


def read_dump(data: bytes) -> Dump:
    """Parse a dump; raises `RefusedError` for any header it cannot take."""
    if len(data) < HEADER_BYTES:
        raise RefusedError(f"{len(data)} bytes hold no header")
    head = struct.unpack(f">{HEADER_WORDS}I", data[:HEADER_BYTES])
    size, _, form, depth, width, height, _, order = head[:8]
    bits, line, visual = head[11:14]
    masks = (head[14], head[15], head[16])
    colours = head[19]
    if form != Z_PIXMAP:
        raise RefusedError(f"pixmap format {form}, not ZPixmap")
    if visual not in (TRUE_COLOUR, DIRECT_COLOUR):
        raise RefusedError(f"visual class {visual}: its colours are in a table, not in masks")
    if bits not in PIXEL_BITS:
        raise RefusedError(f"{bits} bits a pixel")
    _check_masks(masks, depth, bits)
    if visual == DIRECT_COLOUR:
        _check_direct_table(data, size, colours)
    start = size + colours * COLOUR_ENTRY_BYTES
    if line < width * bits // 8:
        raise RefusedError(f"rows of {line} bytes hold no {width} pixels of {bits} bits")
    if len(data) < start + line * height:
        raise RefusedError(f"{len(data)} bytes hold no {height} rows of {line}")
    return Dump(width, height, depth, bits, line, order != 0, masks, data, start)


def read_for_screen(data: bytes, screen_depth: int) -> Dump:
    """Parse a dump from a screen whose depth the caller knows, whatever the header says.

    A dump at the screen's depth reads by its header. A 16-bit screen's dump holds the screen's
    own 5-6-5 word in each pixel, so it reads by 5-6-5 once every word is seen to fit. Any other
    mismatch is refused.
    """
    dump = read_dump(data)
    if dump.depth == screen_depth:
        return dump
    if screen_depth != SHALLOW_DEPTH:
        raise RefusedError(f"a dump of depth {dump.depth} from a screen of depth {screen_depth}")
    for y in range(dump.height):
        for x in range(dump.width):
            word = dump.word(x, y)
            if word > SHALLOW_WORD_MAX:
                raise RefusedError(f"{word:#x} at {x},{y} does not fit a 16-bit screen")
    return dataclasses.replace(dump, depth=SHALLOW_DEPTH, masks=SHALLOW_MASKS)


def main() -> None:
    """Print one dump's header and commonest colours."""
    dump = read_dump(pathlib.Path(sys.argv[1]).read_bytes())
    masks = "/".join(f"{mask:x}" for mask in dump.masks)
    print(f"sides {dump.width}x{dump.height} depth {dump.depth} bits {dump.bits} masks {masks}")
    counts = collections.Counter(
        dump.pixel(x, y) for y in range(dump.height) for x in range(dump.width)
    )
    print(" ".join(f"{r:02x}{g:02x}{b:02x}x{n}" for (r, g, b), n in counts.most_common(8)))


if __name__ == "__main__":
    main()
