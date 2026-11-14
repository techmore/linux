// SPDX-License-Identifier: GPL-2.0-only OR MIT
//! M3 queue counters and modulo-32-bit firmware completion stamps.
//!
//! Firmware stamps advance by 0x100 and barriers compare their signed modular
//! difference, as in event::EventValue. Only a bounded batch is outstanding;
//! its distance is well below the half-range. Keep the host command ordinal
//! separately in u64 so stamp wrap never re-registers a queue or emits InitBM.
//!
//! The stamp sequence must not reach its wrap: there it skips zero
//! (0xffffff00 -> 0x100), and the firmware stops on a pass whose own-stamp
//! barrier does not advance by exactly 0x100 ("DAG: Non Sequential Stamp
//! Updates ... stampValue 0x100"), as G15 did at its TA stamp wrap. The
//! render job renumbers its stamps from the seeds, while idle, inside
//! [`RENUMBER_MARGIN`] before the first wrap.

/// Ordinals after which [`stamp`] repeats: the nonzero 24-bit stamp units.
pub(crate) const PERIOD: u64 = 0x00ff_ffff;

/// Nonzero wire stamp after `ordinal` commands, from the retired seed.
/// As on the M4 runtime, do not publish zero as a completion stamp. The
/// 24-bit stamp sequence skips zero; command ordinals/event counts do not.
pub(crate) const fn stamp(seed: u32, ordinal: u64) -> u32 {
    let unit = (((seed >> 8) as u64 - 1 + ordinal % PERIOD) % PERIOD) + 1;
    (unit as u32) << 8
}

/// First ordinal whose [`stamp`] from `seed` has wrapped (0xffffff00 -> 0x100):
/// 0x860000 for the TA seed and 0xc30000 for the fragment seed.
pub(crate) const fn wrap_ordinal(seed: u32) -> u64 {
    PERIOD - ((seed >> 8) as u64 - 1)
}

/// Ordinals before a wrap inside which the stamps are renumbered from their
/// seeds, at the first publication with nothing in flight. The G15 margin
/// (hw::tuning::STAMP_RESEED_MARGIN).
pub(crate) const RENUMBER_MARGIN: u64 = 100_000;

/// Whether `ordinal` of the current numbering is inside the margin before `wrap`.
pub(crate) const fn renumber_due(ordinal: u64, wrap: u64) -> bool {
    ordinal.saturating_add(RENUMBER_MARGIN) >= wrap
}

/// Ordinal of host command `count` in the numbering restarted after command
/// `base` (0 before the first renumbering). The base command itself is
/// ordinal 0: the seeds, which the stamps hold after it.
pub(crate) const fn local(count: u64, base: u64) -> u64 {
    count.saturating_sub(base)
}

/// Firmware command count, before the command with this host ordinal.
pub(crate) const fn previous(ordinal: u64) -> u32 {
    (ordinal as u32).wrapping_sub(1)
}

/// Notifier's modulo-32-bit event threshold (two stages for render).
pub(crate) const fn events(ordinal: u64, stages: u32) -> u32 {
    (ordinal as u32).wrapping_mul(stages)
}

/// Signed modular ordering, valid for the bounded set of outstanding draws.
pub(crate) const fn reached(observed: u32, expected: u32) -> bool {
    observed.wrapping_sub(expected) as i32 >= 0
}
