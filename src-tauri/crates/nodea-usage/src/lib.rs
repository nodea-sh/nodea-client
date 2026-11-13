//! How much of this device's GPU the Nodea process itself is using (#317).
//!
//! Other apps do not count: a video playing next to an idle node must not make
//! the node look busy. Each platform exposes per-process GPU figures in its own
//! way, so each gets a probe (strategy) behind [`UsageProbe`]:
//!
//! | Platform              | Source                                              |
//! |-----------------------|-----------------------------------------------------|
//! | macOS                 | IOKit GPU clients' `accumulatedGPUTime`, via ioreg  |
//! | Linux (AMD, Intel)    | DRM fdinfo `drm-engine-*` busy time                 |
//! | Linux (NVIDIA)        | NVML per-process SM utilization                     |
//! | Windows               | PDH `GPU Engine` counters (what Task Manager shows) |
//!
//! None needs elevated rights. A probe that cannot read anything reports
//! `None`, which the UI shows as "—" rather than a made-up figure.
//!
//! On a CPU-only build (no usable GPU) the share of the CPU is reported
//! instead, since that is what inference runs on there.
//!
//! The parsers are platform-independent and unit tested on captured output;
//! only the code that runs the OS query is gated on the target.

use serde::Serialize;
use std::time::Instant;

pub mod counters;
pub mod drm_fdinfo;
pub mod ioreg;
pub mod nvidia;
pub mod pdh;

/// One reading of a utilization source. `None`: no reading (yet).
pub trait UsageProbe: Send {
    fn sample(&mut self) -> Option<u8>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageSource {
    Gpu,
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NodeaUsage {
    pub source: UsageSource,
    /// 0-100, or `None` when this platform gave no reading.
    pub percent: Option<u8>,
}

pub(crate) fn clamp_pct(value: f64) -> u8 {
    if value.is_nan() {
        return 0;
    }
    value.round().clamp(0.0, 100.0) as u8
}

/// Reports the highest reading among several probes (e.g. a Linux machine
/// with an AMD iGPU and an NVIDIA card: whichever Nodea is using shows).
pub struct MaxOf(pub Vec<Box<dyn UsageProbe>>);

impl UsageProbe for MaxOf {
    fn sample(&mut self) -> Option<u8> {
        // Sample every probe, every time: counter-based probes need each tick.
        self.0.iter_mut().filter_map(|p| p.sample()).max()
    }
}

/// This process's share of the whole CPU (all cores), from sysinfo.
pub struct CpuProbe {
    system: sysinfo::System,
    pid: sysinfo::Pid,
    cores: f32,
}

impl CpuProbe {
    pub fn new(pid: u32) -> Self {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get() as f32)
            .unwrap_or(1.0);
        Self {
            system: sysinfo::System::new(),
            pid: sysinfo::Pid::from_u32(pid),
            cores,
        }
    }
}

impl UsageProbe for CpuProbe {
    fn sample(&mut self) -> Option<u8> {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu(),
        );
        // sysinfo reports 100% per core; normalise to the whole CPU.
        let per_core = self.system.process(self.pid)?.cpu_usage();
        Some(clamp_pct((per_core / self.cores) as f64))
    }
}

/// The GPU probe for this platform, watching `pid`.
pub fn gpu_probe(pid: u32) -> Box<dyn UsageProbe> {
    #[cfg(target_os = "macos")]
    {
        Box::new(ioreg::IoregProbe::new(pid))
    }
    #[cfg(target_os = "linux")]
    {
        let mut probes: Vec<Box<dyn UsageProbe>> =
            vec![Box::new(drm_fdinfo::FdinfoProbe::new(pid))];
        if let Some(nvml) = nvidia::NvmlProbe::new(pid) {
            probes.push(Box::new(nvml));
        }
        Box::new(MaxOf(probes))
    }
    #[cfg(windows)]
    {
        Box::new(pdh::PdhProbe::new(pid))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        struct Unsupported;
        impl UsageProbe for Unsupported {
            fn sample(&mut self) -> Option<u8> {
                None
            }
        }
        let _ = pid;
        Box::new(Unsupported)
    }
}

/// Samples Nodea's usage: the GPU when inference runs on one, else the CPU.
pub struct UsageSampler {
    source: UsageSource,
    probe: Box<dyn UsageProbe>,
}

impl UsageSampler {
    pub fn new(pid: u32, has_gpu: bool) -> Self {
        if has_gpu {
            Self::with_probe(UsageSource::Gpu, gpu_probe(pid))
        } else {
            Self::with_probe(UsageSource::Cpu, Box::new(CpuProbe::new(pid)))
        }
    }

    pub fn with_probe(source: UsageSource, probe: Box<dyn UsageProbe>) -> Self {
        Self { source, probe }
    }

    /// Blocking: may spawn ioreg or query the OS. Call off the async runtime.
    pub fn sample(&mut self) -> NodeaUsage {
        NodeaUsage {
            source: self.source,
            percent: self.probe.sample(),
        }
    }
}

/// Wall-clock nanoseconds since `since`; a helper for counter probes.
pub(crate) fn elapsed_ns(since: Instant) -> u64 {
    since.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
