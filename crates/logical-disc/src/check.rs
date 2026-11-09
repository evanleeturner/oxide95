// SPDX-License-Identifier: MIT
//! The check of a picked disc: the files a game expects, by name, against
//! the files the player picked, by path and size. Each wrong file is one
//! [`Finding`], and each finding is one sentence naming the file, so a page
//! says what is wrong instead of failing blank. The expected names are a
//! game's fact and live in its own crate; this module names no game.
//!
//! Paths are relative to the disc's root, `/` between their parts, as a
//! picked folder lists them once the page drops the folder's own name.
//! Names compare exactly; letter case differs only as a finding, ASCII
//! alone, since a disc's names are ASCII. A picked file the list does not
//! name is no finding: the list says what must be there, not all that may.
use alloc::vec::Vec;
use core::fmt;

/// One file of the player's pick: its path from the disc's root, and its
/// size as read now, `None` when it can no longer be read, as a file moved
/// or removed after the pick answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picked<'a> {
    pub path: &'a str,
    pub size: Option<u64>,
}

/// One expected file the pick does not hold right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finding<'a> {
    /// No picked path matches the name, letter case aside.
    Missing { expected: &'a str },
    /// A picked path matches the name only with its letter case changed.
    CaseChanged { expected: &'a str, found: &'a str },
    /// The picked file holds 0 bytes.
    Empty { path: &'a str },
    /// The picked file can no longer be read: moved or removed after the pick.
    Unreadable { path: &'a str },
}

impl Finding<'_> {
    /// The finding's kind, as its `disc.check` DEBUG line writes it.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Finding::Missing { .. } => "missing",
            Finding::CaseChanged { .. } => "case",
            Finding::Empty { .. } => "empty",
            Finding::Unreadable { .. } => "unreadable",
        }
    }

    /// The expected name the finding is about.
    #[must_use]
    pub fn expected(&self) -> &str {
        match self {
            Finding::Missing { expected } | Finding::CaseChanged { expected, .. } => expected,
            Finding::Empty { path } | Finding::Unreadable { path } => path,
        }
    }
}

impl fmt::Display for Finding<'_> {
    /// One sentence naming the file, for the page to show as it stands.
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Finding::Missing { expected } => write!(
                out,
                "The file {expected} is not in the picked disc: pick the disc's whole folder again."
            ),
            Finding::CaseChanged { expected, found } => write!(
                out,
                "The file {expected} was picked as {found}: the disc's names keep their letter case, so this is not the disc's own copy."
            ),
            Finding::Empty { path } => write!(
                out,
                "The file {path} is empty in the picked disc, but the game reads data from it."
            ),
            Finding::Unreadable { path } => write!(
                out,
                "The file {path} can no longer be read: it was moved or removed after the pick, so pick the disc again."
            ),
        }
    }
}

/// Checks the pick against the expected names, and returns the findings in
/// the expected list's order, at most one an expected name: an empty list
/// means the pick holds every expected file, each readable and non-empty.
///
/// An exact path wins over a case-changed one. Writes one `disc.check`
/// DEBUG line for the whole check and one `disc.finding` line a finding.
#[must_use]
pub fn check<'a>(expected: &[&'a str], picked: &[Picked<'a>]) -> Vec<Finding<'a>> {
    let mut findings = Vec::new();
    for &name in expected {
        let finding = match picked.iter().find(|file| file.path == name) {
            Some(Picked { size: None, .. }) => Some(Finding::Unreadable { path: name }),
            Some(Picked { size: Some(0), .. }) => Some(Finding::Empty { path: name }),
            Some(_) => None,
            None => Some(
                match picked
                    .iter()
                    .find(|file| file.path.eq_ignore_ascii_case(name))
                {
                    Some(file) => Finding::CaseChanged {
                        expected: name,
                        found: file.path,
                    },
                    None => Finding::Missing { expected: name },
                },
            ),
        };
        if let Some(finding) = finding {
            log::debug!(k = finding.kind(), p = finding.expected(); "disc.finding");
            findings.push(finding);
        }
    }
    log::debug!(n = expected.len(), m = picked.len(), f = findings.len(); "disc.check");
    findings
}
