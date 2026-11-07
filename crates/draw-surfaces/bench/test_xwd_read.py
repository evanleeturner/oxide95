#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""G6: the reader cannot lie. Dumps made by hand with known pixels, and ones it must refuse.

Run: python3 -m unittest test_xwd_read, from this folder.
"""

from __future__ import annotations

import struct
import unittest

import xwd_read

MASKS_565 = (0xF800, 0x07E0, 0x001F)
MASKS_888 = (0xFF0000, 0x00FF00, 0x0000FF)
HEADER_BYTES = 100
Z_PIXMAP = 2
TRUE_COLOUR = 4
PSEUDO_COLOUR = 3
DIRECT_COLOUR = 5
TABLE_ENTRIES = 256


def make_dump(
    rows: list[bytes],
    width: int,
    bits: int,
    depth: int,
    masks: tuple[int, int, int],
    *,
    big_endian: bool = False,
    line: int | None = None,
    form: int = Z_PIXMAP,
    visual: int = TRUE_COLOUR,
    colours: int = 0,
    table: bytes | None = None,
) -> bytes:
    """A dump by the header's words alone, as `xwd` writes them, big-endian."""
    line = line if line is not None else width * bits // 8
    words = [0] * 25
    words[0] = HEADER_BYTES
    words[2], words[3], words[4], words[5] = form, depth, width, len(rows)
    words[7] = int(big_endian)
    words[11], words[12], words[13] = bits, line, visual
    words[14], words[15], words[16] = masks
    words[19] = colours
    body = b"".join(row.ljust(line, b"\xee") for row in rows)
    table = table if table is not None else b"\xcc" * (colours * 12)
    return struct.pack(">25I", *words) + table + body


def plain_table(entries: int = TABLE_ENTRIES) -> bytes:
    """A DirectColor table that maps each value to itself, as all 3 measured dumps hold."""
    return b"".join(
        struct.pack(">IHHHBB", i * 0x010101, i * 257, i * 257, i * 257, 0, 0)
        for i in range(entries)
    )


class KnownPixels(unittest.TestCase):
    """Each expected colour below is worked by hand from the bytes written."""

    def test_sixteen_bits_by_masks(self):
        row = struct.pack("<4H", 0xF800, 0x07E0, 0x001F, 0x9A6B)
        dump = xwd_read.read_dump(make_dump([row], 4, 16, 16, MASKS_565))
        self.assertEqual(dump.pixel(0, 0), (0xF8, 0, 0))
        self.assertEqual(dump.pixel(1, 0), (0, 0xFC, 0))
        self.assertEqual(dump.pixel(2, 0), (0, 0, 0xF8))
        # 0x9a6b is 10011 010011 01011, so 0x13, 0x13 and 0x0b, each shifted up.
        self.assertEqual(dump.pixel(3, 0), (0x98, 0x4C, 0x58))

    def test_twenty_four_bits_in_three_bytes(self):
        row = bytes([0x5B, 0x37, 0x13, 0xC1, 0x2E, 0x8D])
        dump = xwd_read.read_dump(make_dump([row], 2, 24, 24, MASKS_888))
        self.assertEqual(dump.pixel(0, 0), (0x13, 0x37, 0x5B))
        self.assertEqual(dump.pixel(1, 0), (0x8D, 0x2E, 0xC1))

    def test_thirty_two_bits_ignore_the_spare_byte(self):
        row = bytes([0x5B, 0x37, 0x13, 0xFF])
        dump = xwd_read.read_dump(make_dump([row], 1, 32, 24, MASKS_888))
        self.assertEqual(dump.pixel(0, 0), (0x13, 0x37, 0x5B))

    def test_big_endian_pixels(self):
        row = bytes([0x00, 0x13, 0x37, 0x5B])
        dump = xwd_read.read_dump(make_dump([row], 1, 32, 24, MASKS_888, big_endian=True))
        self.assertEqual(dump.pixel(0, 0), (0x13, 0x37, 0x5B))

    def test_masks_are_read_never_assumed(self):
        row = bytes([0x13, 0x37, 0x5B, 0x00])
        swapped = (0x0000FF, 0x00FF00, 0xFF0000)
        dump = xwd_read.read_dump(make_dump([row], 1, 32, 24, swapped))
        self.assertEqual(dump.pixel(0, 0), (0x13, 0x37, 0x5B))

    def test_rows_are_stepped_by_the_header_line(self):
        rows = [bytes([1, 2, 3]), bytes([4, 5, 6])]
        dump = xwd_read.read_dump(make_dump(rows, 1, 24, 24, MASKS_888, line=4096))
        self.assertEqual(dump.pixel(0, 1), (6, 5, 4))

    def test_the_colour_table_is_stepped_over(self):
        row = bytes([0x5B, 0x37, 0x13])
        dump = xwd_read.read_dump(make_dump([row], 1, 24, 24, MASKS_888, colours=3))
        self.assertEqual(dump.pixel(0, 0), (0x13, 0x37, 0x5B))


class FalseHeaders(unittest.TestCase):
    """A dump shaped as the measured 16-bit Wine dump: 24-bit cells, 8-bit masks, a 5-6-5 word."""

    @staticmethod
    def shaped(row: bytes) -> bytes:
        return make_dump(
            [row],
            len(row) // 3,
            24,
            24,
            MASKS_888,
            visual=DIRECT_COLOUR,
            colours=TABLE_ENTRIES,
            table=plain_table(),
        )

    def test_the_word_reads_by_the_screen(self):
        # 0x11ab is 00010 001101 01011, so 2, 13 and 11, each shifted up.
        dump = xwd_read.read_for_screen(self.shaped(bytes([0xAB, 0x11, 0x00])), 16)
        self.assertEqual(dump.pixel(0, 0), (0x10, 0x34, 0x58))

    def test_the_header_alone_reads_the_false_colour(self):
        dump = xwd_read.read_dump(self.shaped(bytes([0xAB, 0x11, 0x00])))
        self.assertEqual(dump.pixel(0, 0), (0x00, 0x11, 0xAB))

    def test_a_word_past_16_bits_is_refused(self):
        with self.assertRaises(xwd_read.RefusedError):
            xwd_read.read_for_screen(self.shaped(bytes([0xAB, 0x11, 0x01])), 16)

    def test_any_other_mismatch_is_refused(self):
        with self.assertRaises(xwd_read.RefusedError):
            xwd_read.read_for_screen(self.shaped(bytes([0xAB, 0x11, 0x00])), 8)

    def test_a_deep_screen_reads_by_its_header(self):
        dump = xwd_read.read_for_screen(self.shaped(bytes([0x5B, 0x37, 0x13])), 24)
        self.assertEqual(dump.pixel(0, 0), (0x13, 0x37, 0x5B))

    def test_a_true_shallow_dump_reads_by_its_header(self):
        row = struct.pack("<H", 0x11AB)
        whole = make_dump([row], 1, 16, 16, MASKS_565)
        self.assertEqual(xwd_read.read_for_screen(whole, 16).pixel(0, 0), (0x10, 0x34, 0x58))


class DirectColourTables(unittest.TestCase):
    """DirectColor keeps its colours in a table, so masks hold only under a plain one."""

    ROW = bytes([0x5B, 0x37, 0x13])

    def refused(self, *, colours: int, table: bytes) -> None:
        whole = make_dump(
            [self.ROW], 1, 24, 24, MASKS_888, visual=DIRECT_COLOUR, colours=colours, table=table
        )
        with self.assertRaises(xwd_read.RefusedError):
            xwd_read.read_dump(whole)

    def test_a_table_that_turns_a_colour(self):
        turned = bytearray(plain_table())
        turned[12 * 16 + 4] ^= 0xFF
        self.refused(colours=TABLE_ENTRIES, table=bytes(turned))

    def test_no_table(self):
        self.refused(colours=0, table=b"")

    def test_a_short_table(self):
        self.refused(colours=8, table=plain_table(8))


class Refusals(unittest.TestCase):
    """A dump whose masks or layout cannot be read says so."""

    ROW = bytes(4)

    def refused(self, **terms):
        settled = {"bits": 32, "depth": 24, "masks": MASKS_888} | terms
        with self.assertRaises(xwd_read.RefusedError):
            xwd_read.read_dump(make_dump([self.ROW], 1, **settled))

    def test_a_colour_with_no_mask(self):
        self.refused(masks=(0xFF0000, 0x00FF00, 0))

    def test_masks_that_overlap(self):
        self.refused(masks=(0xFF0000, 0xFF0000, 0x0000FF))

    def test_a_mask_with_a_gap(self):
        self.refused(masks=(0xF0F000, 0x000F00, 0x0000FF))

    def test_a_mask_wider_than_a_channel(self):
        self.refused(masks=(0xFFF000, 0x000F00, 0x0000FF))

    def test_masks_past_the_pixel(self):
        self.refused(bits=16, depth=16, masks=(0xF80000, 0x07E0, 0x001F))

    def test_shallow_masks_on_deep_pixels(self):
        self.refused(bits=24, depth=24, masks=MASKS_565)

    def test_a_table_of_colours(self):
        self.refused(visual=PSEUDO_COLOUR)

    def test_a_form_not_read_here(self):
        self.refused(form=1)

    def test_a_short_file(self):
        whole = make_dump([self.ROW], 1, 32, 24, MASKS_888)
        with self.assertRaises(xwd_read.RefusedError):
            xwd_read.read_dump(whole[:-1])

    def test_no_header(self):
        with self.assertRaises(xwd_read.RefusedError):
            xwd_read.read_dump(b"short")


if __name__ == "__main__":
    unittest.main()
