//! Checks of the computer that run none of this workspace's other code.
//!
//! A fault on a player's screen has three possible causes: the computer, these
//! stand-ins, or the program using them. These checks clear the first, so a later
//! fault in the stand-ins' own checks can name its suspect. The browser's checks are
//! `page/`, plain modules with no build step. This library holds nothing.
#![no_std]
