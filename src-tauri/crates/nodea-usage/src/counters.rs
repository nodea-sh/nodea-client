//! Turning cumulative busy counters into a utilization percentage.
//!
//! macOS and Linux both report how long a process has kept each GPU engine
//! busy since it started (nanoseconds, or for Intel xe, GPU cycles against a
//! total-cycles counter). Utilization is the change between two samples
//! divided by the change in the reference: wall time, or total cycles.

use crate::clamp_pct;
use std::collections::BTreeMap;
use std::time::Instant;

/// Busy time per engine, plus, for cycle-based engines, the total it is
/// measured against. Without a total the reference is wall-clock nanoseconds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EngineCounters {
    pub busy: BTreeMap<String, u64>,
    pub total: BTreeMap<String, u64>,
}

/// Utilization of the busiest engine between two samples. Engines only in
/// `cur` (just created) count as idle this time rather than as a spike of
/// everything they accumulated before the first sample.
pub fn utilization(prev: &EngineCounters, cur: &EngineCounters, wall_ns: u64) -> u8 {
    cur.busy
        .iter()
        .filter_map(|(engine, &busy)| {
            let before = *prev.busy.get(engine)?;
            let delta = busy.saturating_sub(before) as f64;
            let reference = match (cur.total.get(engine), prev.total.get(engine)) {
                (Some(&t1), Some(&t0)) => t1.saturating_sub(t0),
                (Some(_), None) => return None,
                (None, _) => wall_ns,
            };
            if reference == 0 {
                return Some(0);
            }
            Some(clamp_pct(delta / reference as f64 * 100.0))
        })
        .max()
        .unwrap_or(0)
}

/// Remembers the previous sample so each call returns the utilization since.
#[derive(Default)]
pub struct CounterDelta {
    prev: Option<(Instant, EngineCounters)>,
}

impl CounterDelta {
    /// `None` on the first sample, or when `counters` is `None` (no reading).
    pub fn update(&mut self, counters: Option<EngineCounters>) -> Option<u8> {
        let now = Instant::now();
        let Some(cur) = counters else {
            self.prev = None;
            return None;
        };
        let result = self
            .prev
            .as_ref()
            .map(|(at, prev)| utilization(prev, &cur, crate::elapsed_ns(*at)));
        self.prev = Some((now, cur));
        result
    }
}
