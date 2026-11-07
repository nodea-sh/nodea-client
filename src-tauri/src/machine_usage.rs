// How busy this machine is right now, for the Node page's usage ring (#317).
//
// Inference runs on the GPU (Metal, Vulkan), so GPU utilization is the number
// that moves when the node is generating. It is read from whatever the OS
// exposes without elevated rights; where nothing does (Windows for now, or a
// Linux GPU with no busy counter), the ring falls back to CPU usage and says so.
//
// Both are whole-machine figures, not Nodea's share: another app using the GPU
// shows up here too. The frontend's tooltip states that.

use serde::Serialize;
use std::sync::Mutex;
use sysinfo::{CpuRefreshKind, RefreshKind, System};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageSource {
    Gpu,
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MachineUsage {
    pub source: UsageSource,
    /// 0-100.
    pub percent: u8,
}

fn clamp_pct(value: f64) -> u8 {
    value.round().clamp(0.0, 100.0) as u8
}

/// Highest "Device Utilization %" across the accelerators in
/// `ioreg -r -d 1 -w 0 -c IOAccelerator` output (an Intel Mac can have two).
pub fn parse_ioreg_utilization(output: &str) -> Option<u8> {
    const KEY: &str = "\"Device Utilization %\"=";
    output
        .match_indices(KEY)
        .filter_map(|(at, _)| {
            let digits: String = output[at + KEY.len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            digits.parse::<f64>().ok()
        })
        .map(clamp_pct)
        .max()
}

/// Highest value in `nvidia-smi --query-gpu=utilization.gpu
/// --format=csv,noheader,nounits` output (one line per GPU).
pub fn parse_nvidia_smi(output: &str) -> Option<u8> {
    output
        .lines()
        .filter_map(|line| line.trim().parse::<f64>().ok())
        .map(clamp_pct)
        .max()
}

/// The contents of an amdgpu `gpu_busy_percent` sysfs file.
pub fn parse_busy_percent(contents: &str) -> Option<u8> {
    contents.trim().parse::<f64>().ok().map(clamp_pct)
}

/// Prefer the GPU reading; fall back to CPU when the OS gave none.
pub fn choose(gpu: Option<u8>, cpu: f32) -> MachineUsage {
    match gpu {
        Some(percent) => MachineUsage { source: UsageSource::Gpu, percent },
        None => MachineUsage { source: UsageSource::Cpu, percent: clamp_pct(cpu as f64) },
    }
}

#[cfg(target_os = "macos")]
fn gpu_utilization() -> Option<u8> {
    let out = std::process::Command::new("ioreg")
        .args(["-r", "-d", "1", "-w", "0", "-c", "IOAccelerator"])
        .output()
        .ok()?;
    parse_ioreg_utilization(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(target_os = "linux")]
fn gpu_utilization() -> Option<u8> {
    let amd = std::fs::read_dir("/sys/class/drm")
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|card| std::fs::read_to_string(card.path().join("device/gpu_busy_percent")).ok())
        .filter_map(|contents| parse_busy_percent(&contents))
        .max();
    amd.or_else(|| {
        let out = std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=utilization.gpu", "--format=csv,noheader,nounits"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_nvidia_smi(&String::from_utf8_lossy(&out.stdout))
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn gpu_utilization() -> Option<u8> {
    None
}

/// Keeps one sysinfo `System` alive: CPU usage is the delta between two
/// refreshes, so a fresh one per call would always read 0.
pub struct UsageSampler {
    system: Mutex<System>,
}

impl Default for UsageSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageSampler {
    pub fn new() -> Self {
        let system =
            System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing().with_cpu_usage()));
        Self { system: Mutex::new(system) }
    }

    fn cpu_percent(&self) -> f32 {
        let mut system = self.system.lock().unwrap_or_else(|e| e.into_inner());
        system.refresh_cpu_usage();
        system.global_cpu_usage()
    }

    /// Blocking: may spawn `ioreg` or `nvidia-smi`. Call off the async runtime.
    pub fn sample(&self) -> MachineUsage {
        choose(gpu_utilization(), self.cpu_percent())
    }
}
