//! Linux (NVIDIA): per-process SM utilization from NVML.
//!
//! NVIDIA's proprietary driver does not fill in DRM fdinfo, but its management
//! library (libnvidia-ml.so, installed with the driver) reports, per process,
//! the share of the GPU's streaming multiprocessors it used: the same figure
//! `nvidia-smi pmon` shows. The library is loaded at runtime, so machines
//! without it simply skip this probe.

/// The highest SM utilization among `samples` (pid, sm_util) for `pid`;
/// 0 when the process has none (it did not use the GPU in that window).
pub fn max_sm_util_for_pid(samples: impl IntoIterator<Item = (u32, u32)>, pid: u32) -> u32 {
    samples
        .into_iter()
        .filter(|&(p, _)| p == pid)
        .map(|(_, util)| util)
        .max()
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
pub use probe::NvmlProbe;

#[cfg(target_os = "linux")]
mod probe {
    use nvml_wrapper::error::NvmlError;
    use nvml_wrapper::Nvml;

    pub struct NvmlProbe {
        nvml: Nvml,
        pid: u32,
        /// NVML timestamp (µs) of the newest sample seen, per device index.
        last_seen: Vec<u64>,
    }

    impl NvmlProbe {
        /// `None` when NVML is not installed or sees no device.
        pub fn new(pid: u32) -> Option<Self> {
            let nvml = Nvml::init().ok()?;
            let count = nvml.device_count().ok()?;
            (count > 0).then(|| Self {
                nvml,
                pid,
                last_seen: vec![0; count as usize],
            })
        }
    }

    impl crate::UsageProbe for NvmlProbe {
        fn sample(&mut self) -> Option<u8> {
            let mut busiest: Option<u32> = None;
            for (index, last_seen) in self.last_seen.iter_mut().enumerate() {
                let Ok(device) = self.nvml.device_by_index(index as u32) else {
                    continue;
                };
                let samples = match device.process_utilization_stats(*last_seen) {
                    Ok(samples) => samples,
                    // No process used this GPU since the last sample.
                    Err(NvmlError::NotFound) => {
                        busiest = Some(busiest.unwrap_or(0));
                        continue;
                    }
                    Err(_) => continue,
                };
                if let Some(newest) = samples.iter().map(|s| s.timestamp).max() {
                    *last_seen = newest;
                }
                let ours = super::max_sm_util_for_pid(
                    samples.iter().map(|s| (s.pid, s.sm_util)),
                    self.pid,
                );
                busiest = Some(busiest.unwrap_or(0).max(ours));
            }
            busiest.map(|v| crate::clamp_pct(v as f64))
        }
    }
}
