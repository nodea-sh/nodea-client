//! Linux (AMD, Intel and other DRM drivers): per-process GPU busy time.
//!
//! Since kernel 5.19 (amdgpu since 5.14), every open DRM file in
//! `/proc/<pid>/fdinfo/<fd>` carries the client's engine usage:
//!
//! ```text
//! drm-driver:        amdgpu
//! drm-client-id:     12
//! drm-engine-gfx:    1234567 ns
//! drm-engine-compute: 890 ns
//! ```
//!
//! Intel's xe driver reports cycles instead: `drm-cycles-rcs` against
//! `drm-total-cycles-rcs`. Several fds can share one client, so clients are
//! de-duplicated by `drm-client-id` before summing per engine.
//!
//! NVIDIA's proprietary driver does not report this; see `nvml.rs`.

use crate::counters::EngineCounters;
use std::collections::BTreeSet;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DrmClient {
    pub client_id: String,
    pub counters: EngineCounters,
}

/// One fdinfo file. `None` if it is not a DRM client with engine figures.
pub fn parse_fdinfo(contents: &str) -> Option<DrmClient> {
    let mut client = DrmClient::default();
    for line in contents.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if key == "drm-client-id" {
            client.client_id = value.to_string();
        } else if let Some(engine) = key.strip_prefix("drm-engine-") {
            // "drm-engine-capacity-<e>" is an engine count, not busy time.
            if engine.starts_with("capacity-") {
                continue;
            }
            if let Some(ns) = value.strip_suffix("ns").and_then(|v| v.trim().parse().ok()) {
                client.counters.busy.insert(engine.to_string(), ns);
            }
        } else if let Some(engine) = key.strip_prefix("drm-total-cycles-") {
            if let Ok(v) = value.parse() {
                client.counters.total.insert(engine.to_string(), v);
            }
        } else if let Some(engine) = key.strip_prefix("drm-cycles-") {
            if let Ok(v) = value.parse() {
                client.counters.busy.insert(engine.to_string(), v);
            }
        }
    }
    (!client.client_id.is_empty() && !client.counters.busy.is_empty()).then_some(client)
}

/// Sum the engines of distinct clients. `None` when there are no clients.
pub fn combine(clients: impl IntoIterator<Item = DrmClient>) -> Option<EngineCounters> {
    let mut seen = BTreeSet::new();
    let mut combined: Option<EngineCounters> = None;
    for client in clients {
        if !seen.insert(client.client_id.clone()) {
            continue;
        }
        let acc = combined.get_or_insert_with(EngineCounters::default);
        for (engine, v) in client.counters.busy {
            *acc.busy.entry(engine).or_default() += v;
        }
        for (engine, v) in client.counters.total {
            // Total cycles are the GPU's clock, the same for every client.
            acc.total.insert(engine, v);
        }
    }
    combined
}

#[cfg(target_os = "linux")]
pub struct FdinfoProbe {
    pid: u32,
    delta: crate::counters::CounterDelta,
}

#[cfg(target_os = "linux")]
impl FdinfoProbe {
    pub fn new(pid: u32) -> Self {
        Self {
            pid,
            delta: Default::default(),
        }
    }

    fn read(&self) -> Option<EngineCounters> {
        let dir = std::fs::read_dir(format!("/proc/{}/fdinfo", self.pid)).ok()?;
        let clients = dir
            .flatten()
            .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
            .filter_map(|contents| parse_fdinfo(&contents));
        combine(clients)
    }
}

#[cfg(target_os = "linux")]
impl crate::UsageProbe for FdinfoProbe {
    fn sample(&mut self) -> Option<u8> {
        let counters = self.read();
        self.delta.update(counters)
    }
}
