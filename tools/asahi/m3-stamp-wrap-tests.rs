// SPDX-License-Identifier: GPL-2.0-only OR MIT
//! Host tests of the M3 render stamp numbering across the stamp wrap: the barrier pairs
//! (wait, value) of each draw's own-stamp dependency, as the firmware receives them.
//! Run: rustc --edition=2021 --test tools/asahi/m3-stamp-wrap-tests.rs -o /tmp/m3-stamp-wrap-tests
#![allow(dead_code)]
#[path = "../../drivers/gpu/drm/asahi/m3/m3_timeline.rs"]
mod timeline;
use std::collections::VecDeque;
use std::sync::OnceLock;

const SHARED: &str = include_str!("../../drivers/gpu/drm/asahi/m3/m3_shared_layout.rs");
/// m3_pass_layout::SLOTS: passes in flight at most.
const SLOTS: usize = 16;

fn seed(name: &str) -> u32 {
    let key = format!("pub(crate) const {}:u32=0x", name);
    let at = SHARED.find(&key).expect(name) + key.len();
    let end = SHARED[at..].find(';').unwrap() + at;
    u32::from_str_radix(&SHARED[at..end].replace('_', ""), 16).unwrap()
}
/// The seeds m3_render passes to timeline::stamp (STAMP_* - 256), read from the driver.
fn seeds() -> (u32, u32) {
    static SEEDS: OnceLock<(u32, u32)> = OnceLock::new();
    *SEEDS.get_or_init(|| (seed("STAMP_TA") - 256, seed("STAMP_FRAGMENT") - 256))
}
fn ta_seed() -> u32 { seeds().0 }
fn fs_seed() -> u32 { seeds().1 }
/// m3_render::RENDER_WRAP.
fn render_wrap() -> u64 { timeline::wrap_ordinal(ta_seed()).min(timeline::wrap_ordinal(fs_seed())) }

#[derive(Clone, Copy, PartialEq, Debug)]
enum Fix {
    /// Before the renumbering: one numbering from boot.
    None,
    /// Renumbering at idle publications only, without the drain (negative control).
    IdleOnly,
    /// Renumbering, and nothing published behind a batch inside the margin.
    Drain,
}

#[derive(Debug, PartialEq)]
struct Pair { draw: u64, wait: u32, value: u32, behind: bool }

/// The render job's numbering and the runtime's publication rules, per draw.
struct Model {
    fix: Fix, draw: u64, base: u64, ta: u32, fs: u32,
    flight: VecDeque<(u64, u32, u32)>,
    fs_bad: Vec<Pair>, ta_bad: Vec<Pair>, renumbered: Vec<u64>, max_ordinal: u64,
}

impl Model {
    /// Idle after `draw` draws, numbered from draw `base`.
    fn at(fix: Fix, draw: u64, base: u64) -> Self {
        let o = timeline::local(draw, base);
        Model { fix, draw, base, ta: timeline::stamp(ta_seed(), o), fs: timeline::stamp(fs_seed(), o),
            flight: VecDeque::new(), fs_bad: vec![], ta_bad: vec![], renumbered: vec![], max_ordinal: o }
    }
    fn ordinal(&self, d: u64) -> u64 {
        if self.fix == Fix::None { d } else { timeline::local(d, self.base) }
    }
    /// Render::renumber_due.
    fn due(&self) -> bool {
        self.fix != Fix::None && timeline::renumber_due(self.ordinal(self.draw), render_wrap())
    }
    /// Retire the oldest draw: its stamps land.
    fn retire(&mut self) -> bool {
        match self.flight.pop_front() {
            Some((_, ta, fs)) => { self.ta = ta; self.fs = fs; true }
            None => false,
        }
    }
    /// Runtime::pipeline_slot + Render::begin_batch + append. False: wait for a retirement.
    fn publish(&mut self, n: usize) -> bool {
        let overlap = !self.flight.is_empty();
        if overlap && (self.flight.len() + n > SLOTS || (self.fix == Fix::Drain && self.due())) {
            return false;
        }
        if !overlap && self.due() {
            // Render::renumber_stamps: the seeds, both copies, while idle.
            self.ta = timeline::stamp(ta_seed(), 0);
            self.fs = timeline::stamp(fs_seed(), 0);
            self.base = self.draw;
            self.renumbered.push(self.draw);
        }
        for _ in 0..n {
            self.draw += 1;
            let d = self.draw;
            let (o, p) = (self.ordinal(d), self.ordinal(d - 1));
            self.max_ordinal = self.max_ordinal.max(o);
            let behind = !self.flight.is_empty();
            let (ta_wait, ta_value) = (timeline::stamp(ta_seed(), p), timeline::stamp(ta_seed(), o));
            let (fs_wait, fs_value) = (timeline::stamp(fs_seed(), p), timeline::stamp(fs_seed(), o));
            // The wait is what the stamp holds once draw d-1 retires, or the GPU would stall
            // (or run early) here.
            let (last_ta, last_fs) = self.flight.back().map_or((self.ta, self.fs), |&(_, t, f)| (t, f));
            assert_eq!((ta_wait, fs_wait), (last_ta, last_fs), "draw {d} waits on values its predecessor never writes");
            // The firmware's own-stamp check: the value advances by exactly 0x100.
            if fs_value != fs_wait.wrapping_add(0x100) {
                self.fs_bad.push(Pair { draw: d, wait: fs_wait, value: fs_value, behind });
            }
            if ta_value != ta_wait.wrapping_add(0x100) {
                self.ta_bad.push(Pair { draw: d, wait: ta_wait, value: ta_value, behind });
            }
            self.flight.push_back((d, ta_value, fs_value));
        }
        true
    }
}

/// Deterministic xorshift for client patterns.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
    fn below(&mut self, n: u64) -> u64 { self.next() % n }
}

#[derive(Clone, Copy, Debug)]
enum Client {
    /// Vsync-paced: each frame retires before the next is submitted.
    Paced,
    /// Never idle: publishes whenever allowed, retires only when it has to.
    Busy,
    /// Random batches of 1..=16 draws and random retirement.
    Mixed(u64),
}

/// Drive `draws` draws from `m` with `client`.
fn run(m: &mut Model, client: Client, draws: u64) {
    let end = m.draw + draws;
    let mut rng = Rng(match client { Client::Mixed(s) => s | 1, _ => 0x9e3779b97f4a7c15 });
    while m.draw < end {
        match client {
            Client::Paced => { assert!(m.publish(1)); m.retire(); }
            Client::Busy => { while !m.publish(1) { assert!(m.retire()); } }
            Client::Mixed(_) => {
                let n = 1 + rng.below(SLOTS as u64) as usize;
                while !m.publish(n) { assert!(m.retire()); }
                for _ in 0..rng.below(3) { m.retire(); }
            }
        }
    }
}

#[test]
fn seeds_and_wrap_points() {
    assert_eq!((ta_seed(), fs_seed()), (0x7a00_0000, 0x3d00_0000));
    assert_eq!(timeline::wrap_ordinal(ta_seed()), 0x86_0000);
    assert_eq!(timeline::wrap_ordinal(fs_seed()), 0xc3_0000);
    assert_eq!(render_wrap(), 0x86_0000);
    for seed in [ta_seed(), fs_seed()] {
        let w = timeline::wrap_ordinal(seed);
        assert_eq!(timeline::stamp(seed, 0), seed);
        assert_eq!(timeline::stamp(seed, w - 1), 0xffff_ff00);
        assert_eq!(timeline::stamp(seed, w), 0x100, "the sequence skips zero at its wrap");
        assert_eq!(timeline::stamp(seed, w + timeline::PERIOD), 0x100);
    }
    assert!(!timeline::renumber_due(render_wrap() - timeline::RENUMBER_MARGIN - 1, render_wrap()));
    assert!(timeline::renumber_due(render_wrap() - timeline::RENUMBER_MARGIN, render_wrap()));
    assert_eq!(timeline::local(5, 9), 0);
    assert_eq!(timeline::local(9, 5), 4);
}

/// The two crash logs: `draw=` is the last published draw; TA completed through draw-1 and
/// the fragment stamp through draw-2.
#[test]
fn reported_draws_are_the_first_and_second_fragment_wraps() {
    for (k, draw) in [(0u64, 12_779_520u64), (1, 29_556_735)] {
        assert_eq!(draw, timeline::wrap_ordinal(fs_seed()) + k * timeline::PERIOD);
        assert_eq!(timeline::stamp(fs_seed(), draw), 0x100, "firmware: stampValue 0x100");
        assert_eq!(timeline::stamp(fs_seed(), draw - 1), 0xffff_ff00);
        assert_eq!(timeline::stamp(fs_seed(), draw - 2), 0xffff_fe00, "log: fragment stamps=0xfffffe00");
        assert_eq!(timeline::stamp(ta_seed(), draw - 1), 0x3d00_0000, "log: TA stamps=0x3d000000");
        // The TA stamp is not at a wrap there; it wrapped at 0x860000 (+ k periods) unharmed.
        assert_ne!(timeline::stamp(ta_seed(), draw), 0x100);
    }
}

#[test]
fn one_numbering_skips_zero_at_every_wrap() {
    let fs = timeline::wrap_ordinal(fs_seed());
    let ta = timeline::wrap_ordinal(ta_seed());
    for (start, fs_expect, ta_expect) in [
        (fs - 50, vec![fs], vec![]),
        (fs + timeline::PERIOD - 50, vec![fs + timeline::PERIOD], vec![]),
        (ta - 50, vec![], vec![ta]),
    ] {
        let mut m = Model::at(Fix::None, start, 0);
        run(&mut m, Client::Paced, 100);
        assert_eq!(m.fs_bad.iter().map(|p| p.draw).collect::<Vec<_>>(), fs_expect);
        assert_eq!(m.ta_bad.iter().map(|p| p.draw).collect::<Vec<_>>(), ta_expect);
        for p in m.fs_bad.iter().chain(&m.ta_bad) {
            assert_eq!((p.wait, p.value), (0xffff_ff00, 0x100));
        }
    }
}

/// Starting points just outside and inside the margin, for every client pattern: the pairs
/// stay sequential, the waits match, and the absolute draw count runs past both reported
/// crash draws with the stamps renumbered once.
#[test]
fn renumbering_keeps_every_pair_sequential() {
    let margin = timeline::RENUMBER_MARGIN;
    let wrap = render_wrap();
    let clients = [Client::Paced, Client::Busy, Client::Mixed(1), Client::Mixed(0xdead_beef), Client::Mixed(42)];
    for crash in [12_779_520u64, 29_556_735] {
        for before in [margin + 20_000, margin + 1, margin, margin - 1, margin - 15] {
            for client in clients {
                // Local ordinal wrap - before, absolute draw ~20000 before the crash draw.
                let draw = crash - 20_000;
                let mut m = Model::at(Fix::Drain, draw, draw - (wrap - before));
                run(&mut m, client, 140_000);
                assert!(m.fs_bad.is_empty() && m.ta_bad.is_empty(), "{client:?} {before}: {:?} {:?}",
                    m.fs_bad.first(), m.ta_bad.first());
                assert_eq!(m.renumbered.len(), 1, "{client:?} {before}");
                assert!(m.max_ordinal < wrap, "{client:?} {before}: ordinal {:#x}", m.max_ordinal);
                assert!(m.draw > crash + 100_000);
            }
        }
    }
}

/// Over one full numbering from boot, renumbered every ~8.68M draws: no pair breaks. Jumps
/// over the quiet stretch between margins (the state there is the idle state).
#[test]
fn renumbering_repeats_from_boot() {
    let wrap = render_wrap();
    let mut m = Model::at(Fix::Drain, 0, 0);
    run(&mut m, Client::Mixed(7), 1_000);
    for cycle in 1..=3 {
        assert!(m.fs_bad.is_empty() && m.ta_bad.is_empty());
        while m.retire() {}
        // The idle state anywhere between margins is Model::at(draw, base).
        let skip = m.base + wrap - timeline::RENUMBER_MARGIN - 5_000;
        assert!(skip > m.draw);
        m = Model::at(Fix::Drain, skip, m.base);
        run(&mut m, Client::Mixed(11 + cycle), 10_000);
        assert_eq!(m.renumbered.len(), 1, "cycle {cycle}");
        assert!(m.fs_bad.is_empty() && m.ta_bad.is_empty() && m.max_ordinal < wrap);
    }
    // Three numberings: past 2 * 0x860000 draws, beyond the first fragment wrap.
    assert!(m.draw > 12_779_520 + 2 * 1_000_000);
}

/// Without the drain, a client that never lets the GPU idle carries the stamps through the
/// wrap (as G15 found: 433c2871ce31); with it, it cannot.
#[test]
fn busy_client_needs_the_drain() {
    let wrap = render_wrap();
    let draw = 20_000_000;
    for fix in [Fix::IdleOnly, Fix::Drain] {
        let mut m = Model::at(fix, draw, draw - (wrap - timeline::RENUMBER_MARGIN - 1_000));
        // Fill the pipeline before the margin, then stay busy.
        while m.publish(1) {}
        run(&mut m, Client::Busy, 120_000);
        match fix {
            Fix::IdleOnly => {
                assert!(m.renumbered.is_empty());
                assert_eq!(m.ta_bad.len(), 1, "TA wrap reached");
                assert!(m.ta_bad[0].behind);
            }
            _ => {
                assert_eq!(m.renumbered.len(), 1);
                assert!(m.fs_bad.is_empty() && m.ta_bad.is_empty());
            }
        }
    }
}

/// Incident 1 got past the first fragment wrap (draw 12779520) and stopped at the second.
/// Both logs show the stopping draw published behind its predecessor (fragment done through
/// draw-2: "2 commands in flight"). This only shows that the same numbering yields the same
/// (wait, value) pair either way, so a firmware that stops on some wraps and not others must
/// depend on something else, such as whether the previous update is still pending. That is a
/// hypothesis about firmware, not tested here.
#[test]
fn the_wrap_pair_is_the_same_paced_or_pipelined() {
    let fs = timeline::wrap_ordinal(fs_seed());
    let mut paced = Model::at(Fix::None, fs - 10, 0);
    run(&mut paced, Client::Paced, 20);
    let mut busy = Model::at(Fix::None, fs - 10, 0);
    run(&mut busy, Client::Busy, 20);
    assert_eq!(paced.fs_bad, vec![Pair { draw: fs, wait: 0xffff_ff00, value: 0x100, behind: false }]);
    assert_eq!(busy.fs_bad, vec![Pair { draw: fs, wait: 0xffff_ff00, value: 0x100, behind: true }]);
}
