// SPDX-License-Identifier: MIT
//! What each check wants, read by the checks and by the script that scores
//! a run, so the two judge from one table.

pub(super) const LOCK_WANT: &[(&str, &str)] = &[
    ("id.nonzero", "true"),
    ("id.stable", "true"),
    ("free.other_in", "in"),
    ("twice.returned", "true"),
    ("id.two_differ", "true"),
    ("held.second.inside", "0"),
    ("one_left.second.inside", "0"),
    ("both_left.second.inside", "1"),
    ("handed.third.inside", "0"),
    ("handed.third.in_on_release", "1"),
    ("left.other_in", "in"),
    ("reborn.other_in", "in"),
    ("rounds.2.count", "exact"),
    ("rounds.2.overlap", "0"),
    ("rounds.4.count", "exact"),
    ("rounds.4.overlap", "0"),
];

pub(super) const SLOTS_WANT: &[(&str, &str)] = &[
    ("slot.taken", "true"),
    ("first.unset", "0 error 0"),
    ("first.set", "1 error 0"),
    ("first.get", "7 error 0"),
    ("value.full_width", "4294967295 error 0"),
    ("value.zero", "0 error 0"),
    ("keeps.get", "0 error 0"),
    ("keeps.set", "1 error 4660"),
    ("cross.b.unset", "0 error 0"),
    ("cross.b.set", "1 error 0"),
    ("cross.b.get", "9 error 0"),
    ("cross.a.get", "7 error 0"),
    ("error.second", "0 error 87"),
    ("error.first", "4660"),
    ("keeps.free", "1 error 4660"),
    ("again.index", "same"),
    ("again.a.unset", "0 error 0"),
    ("again.b.unset", "0 error 0"),
    ("bad.get", "0 error 87"),
    ("bad.set", "0 error 87"),
    ("bad.free", "0 error 87"),
];

/// The judge's answers. The crate refuses the read and the write, so its
/// test is ignored with that reason; what Windows 95 answers is not read here.
pub(super) const FREED_WANT: &[(&str, &str)] = &[
    ("freed.get", "0 error 0"),
    ("freed.set", "1 error 0"),
    ("freed.free", "0 error 87"),
];

pub(super) const OUT_WANT: &[(&str, &str)] = &[
    ("out.count", "64"),
    ("out.answer", "4294967295"),
    ("out.one_freed", "comes back"),
];

pub(super) const COUNTER_WANT: &[(&str, &str)] = &[
    ("inc.5", "6 held 6"),
    ("dec.6", "5 held 5"),
    ("inc.max", "-2147483648 held -2147483648"),
    ("dec.min", "2147483647 held 2147483647"),
    ("inc.m1", "0 held 0"),
    ("dec.1", "0 held 0"),
    ("dec.0", "-1 held -1"),
    ("rounds.answers", "each once"),
    ("rounds.sum", "exact"),
];

/// The lines' own answers, harvested from the first clean run: they say the
/// lines are unchanged, and judge nothing beyond that.
pub(super) const LINES_WANT: &[(&str, &str)] = &[
    (
        "line.id",
        "imp.call c=id a= v=<id> from=c2 by=kernel-threads",
    ),
    (
        "line.initialize",
        "imp.call c=initialize a=10 v=0 from=c2 by=kernel-threads",
    ),
    (
        "line.enter",
        "imp.call c=enter a=10 v=0 from=c2 by=kernel-threads",
    ),
    (
        "line.leave",
        "imp.call c=leave a=10 v=0 from=c2 by=kernel-threads",
    ),
    (
        "line.delete",
        "imp.call c=delete a=10 v=0 from=c2 by=kernel-threads",
    ),
    (
        "line.slot_alloc",
        "imp.call c=slot_alloc a= v=0 from=c2 by=kernel-threads",
    ),
    (
        "line.slot_set",
        "imp.call c=slot_set a=0,7 v=1 from=c2 by=kernel-threads",
    ),
    (
        "line.slot_get",
        "imp.call c=slot_get a=0 v=7 from=c2 by=kernel-threads",
    ),
    (
        "line.slot_free",
        "imp.call c=slot_free a=0 v=1 from=c2 by=kernel-threads",
    ),
    (
        "line.slot_free.refused",
        "imp.call c=slot_free a=0 v=0 from=c2 by=kernel-threads",
    ),
    (
        "line.increment",
        "imp.call c=increment a= v=6 from=c2 by=kernel-threads",
    ),
    (
        "line.decrement",
        "imp.call c=decrement a= v=5 from=c2 by=kernel-threads",
    ),
    (
        "line.increment.unimported",
        "imp.call c=unimported a= v=6 from=c2 by=kernel-threads",
    ),
    (
        "line.decrement.unimported",
        "imp.call c=unimported a= v=5 from=c2 by=kernel-threads",
    ),
];
