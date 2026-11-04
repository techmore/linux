// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! T8132 board admission for the qualified M4 firmware graph.
//!
//! M4 uses ten (J713) or eleven (J773g) firmware rows selected from sixteen source states,
//! including off, and sixteen voltage lanes. The source values belong to this machine's
//! preserved original ADT.

pub(crate) mod data;

use super::adt::GpuAdt;
use super::fw::initdata::Performance;
use kernel::{device, prelude::*};

/// Admit the qualified table layout while preserving every machine-specific source value.
/// Differently shaped tables, inconsistent voltage pairs and a row count other than the one
/// qualified for this board fail before publication.
pub(crate) fn qualify(dev: &device::Device, adt: &GpuAdt) -> Result<Performance> {
    let performance = data::select(&adt.performance).map_err(|error| {
        dev_err!(
            dev,
            "M4 original ADT performance layout rejected: {:?}\n",
            error
        );
        EINVAL
    })?;
    let rows = super::boot::glue::perf_rows();
    if performance.count != rows {
        dev_err!(
            dev,
            "M4 original ADT gives {} firmware rows, this board is qualified for {}\n",
            performance.count,
            rows
        );
        return Err(EINVAL);
    }
    dev_info!(
        dev,
        "M4 ADT DVFS: {} source states -> {} firmware rows, top {} MHz\n",
        adt.performance.count,
        performance.count,
        performance.frequency[performance.count - 1]
    );
    for i in 0..performance.count {
        dev_info!(
            dev,
            "M4 ADT DVFS S{}: main={} MHz auxiliary={} MHz core={} mV SRAM={} mV\n",
            i,
            performance.frequency[i],
            performance.auxiliary[i],
            performance.voltage[i],
            performance.sram[i]
        );
    }
    Ok(performance)
}
