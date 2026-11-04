// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Qualified T8132/25G83 selection of the original ADT's sixteen performance states.
//!
//! Each firmware row is one core voltage: the source states that share it, at most two in rising
//! clock order, become one row whose main clock is the higher and auxiliary clock the lower. J713
//! has four single states followed by six equal-voltage pairs (ten rows); J773g has five, then
//! five pairs and a single top state (eleven rows). The values are this machine's original ADT,
//! not frequencies or voltages copied from a reference machine.

pub(crate) const SOURCE_STATES: usize = 16;
/// The qualified firmware row counts: J713 and J773g.
pub(crate) const ROWS: [usize; 2] = [10, 11];

use super::super::fw::initdata::{Performance, PERF_STATES};

/// Allocation-free selection, also exercised directly by the host-side source-data tests.
pub(crate) fn select(
    source: &super::super::adt::perf::Source,
) -> core::result::Result<Performance, super::super::adt::perf::Error> {
    use super::super::adt::perf::Error;
    if source.count != SOURCE_STATES {
        return Err(Error::Shape);
    }
    let mut performance = Performance {
        count: 0,
        frequency: [0; PERF_STATES],
        auxiliary: [0; PERF_STATES],
        voltage: [0; PERF_STATES],
        sram: [0; PERF_STATES],
    };
    let mut first = 0;
    while first < SOURCE_STATES {
        let mut last = first;
        while last + 1 < SOURCE_STATES && source.core_mv[last + 1] == source.core_mv[first] {
            last += 1;
        }
        // Rows rise in voltage. A group is a single state or a pair whose clock rises.
        if performance.count == PERF_STATES
            || last - first > 1
            || (first != 0 && source.core_mv[first] <= source.core_mv[first - 1])
            || (last != first && source.frequency_mhz[last] <= source.frequency_mhz[first])
        {
            return Err(Error::Shape);
        }
        for state in first..=last {
            if source.frequency_mhz[state] != source.sram_frequency_mhz[state]
                || source.sram_mv[state] != source.sram_mv[first]
            {
                return Err(Error::Shape);
            }
        }
        let row = performance.count;
        performance.frequency[row] = source.frequency_mhz[last];
        performance.auxiliary[row] = source.frequency_mhz[first];
        performance.voltage[row] = source.core_mv[first];
        performance.sram[row] = source.sram_mv[first];
        performance.count += 1;
        first = last + 1;
    }
    if !ROWS.contains(&performance.count) {
        return Err(Error::Shape);
    }
    if !performance.valid() {
        return Err(Error::Range);
    }
    Ok(performance)
}
