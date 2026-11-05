//! What this node reports about its memory in the DeviceIdentity handshake.
//!
//! The backend derives the budget a model must fit in from these three figures
//! (`backend/memory_budget.py`); this module only measures honestly.
//!
//! - `total_memory_mb` / `available_memory_mb` come from `sysinfo` on every
//!   platform. On Windows `sysinfo` reads them with `GlobalMemoryStatusEx`
//!   (and reports free memory as "available" there); on Linux from
//!   `/proc/meminfo` (`MemTotal` / `MemAvailable`); on macOS from the kernel.
//! - `accelerator_memory_mb` is the inference backend's own ceiling, the only
//!   figure that needs a per-platform probe:
//!   - macOS: Metal `recommendedMaxWorkingSetSize` - the GPU's share of
//!     unified memory on Apple Silicon, or the dGPU's VRAM on Intel + AMD.
//!   - Linux and Windows with the `cuda` feature (release builds): total VRAM
//!     of CUDA device 0, the device mistralrs's `best_device()` loads onto.
//!   - Linux and Windows without `cuda` (dev and CI builds): `None`. mistralrs
//!     runs on the CPU there and system RAM binds; reporting VRAM would size
//!     the node for memory it cannot use.
//!
//! CI (`test-rust`) runs on Linux without CUDA, so the macOS and CUDA
//! branches are not exercised there and need a real check on each platform.

/// Memory figures in megabytes, as sent in `DeviceIdentity` fields 8-10.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryReport {
    pub total_memory_mb: u32,
    pub available_memory_mb: u32,
    pub accelerator_memory_mb: Option<u32>,
}

pub fn probe() -> MemoryReport {
    let mut system = sysinfo::System::new();
    system.refresh_memory();

    MemoryReport {
        total_memory_mb: bytes_to_mb(system.total_memory()),
        available_memory_mb: bytes_to_mb(system.available_memory()),
        accelerator_memory_mb: accelerator_memory_bytes().map(bytes_to_mb),
    }
}

/// Whole megabytes, saturating rather than wrapping past `u32::MAX` MB.
pub fn bytes_to_mb(bytes: u64) -> u32 {
    u32::try_from(bytes / 1024 / 1024).unwrap_or(u32::MAX)
}

#[cfg(target_os = "macos")]
fn accelerator_memory_bytes() -> Option<u64> {
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice};

    // None when no Metal device can be created (e.g. a non-interactive
    // process); the backend then falls back to total RAM.
    MTLCreateSystemDefaultDevice()
        .map(|device| device.recommendedMaxWorkingSetSize())
        .filter(|bytes| *bytes > 0)
}

#[cfg(all(not(target_os = "macos"), feature = "cuda"))]
fn accelerator_memory_bytes() -> Option<u64> {
    use cudarc::driver::result;

    // None when the driver has no usable device (e.g. no NVIDIA GPU, or the
    // driver is too old for this build); the backend then falls back to
    // total RAM, the same as a CPU-only node.
    result::init().ok()?;
    let device = result::device::get(0).ok()?;
    // SAFETY: `device` is a valid handle from cuDeviceGet after cuInit.
    let bytes = unsafe { result::device::total_mem(device) }.ok()?;
    u64::try_from(bytes).ok().filter(|bytes| *bytes > 0)
}

#[cfg(all(not(target_os = "macos"), not(feature = "cuda")))]
fn accelerator_memory_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1024 * 1024;

    #[test]
    fn bytes_to_mb_truncates_to_whole_megabytes() {
        assert_eq!(bytes_to_mb(16 * 1024 * MB), 16384);
        assert_eq!(bytes_to_mb(MB - 1), 0);
        assert_eq!(bytes_to_mb(MB + MB / 2), 1);
    }

    #[test]
    fn bytes_to_mb_saturates_instead_of_wrapping() {
        assert_eq!(bytes_to_mb(u64::MAX), u32::MAX);
    }

    #[test]
    fn probe_reports_real_system_memory() {
        let report = probe();

        assert!(report.total_memory_mb > 0, "{report:?}");
        // Not `> 0`: on macOS sysinfo's figure is free + inactive + purgeable
        // minus compressed pages, which saturates to 0 under memory pressure.
        assert!(
            report.available_memory_mb <= report.total_memory_mb,
            "{report:?}"
        );
    }

    #[cfg(target_os = "linux")]
    fn meminfo_mb(key: &str) -> u32 {
        let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap();
        let kb: u64 = meminfo
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}:")))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| panic!("{key} missing from /proc/meminfo"));
        bytes_to_mb(kb * 1024)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_total_memory_matches_proc_meminfo() {
        assert_eq!(probe().total_memory_mb, meminfo_mb("MemTotal"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_available_memory_is_mem_available_not_a_constant() {
        let report = probe();
        let mem_available = meminfo_mb("MemAvailable");

        // MemAvailable moves between the two reads; allow a little drift.
        assert!(
            report.available_memory_mb.abs_diff(mem_available) < 256,
            "sysinfo {} MB vs /proc/meminfo {} MB",
            report.available_memory_mb,
            mem_available
        );
    }

    #[cfg(all(target_os = "linux", not(feature = "cuda")))]
    #[test]
    fn linux_cpu_build_reports_no_accelerator() {
        assert_eq!(probe().accelerator_memory_mb, None);
    }

    // Needs an NVIDIA machine: without a device the probe is None and this
    // passes vacuously.
    #[cfg(all(not(target_os = "macos"), feature = "cuda"))]
    #[test]
    fn cuda_build_reports_nonzero_vram_when_a_device_exists() {
        if let Some(accelerator) = probe().accelerator_memory_mb {
            assert!(accelerator > 0);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_working_set_never_exceeds_total_memory() {
        let report = probe();
        if let Some(accelerator) = report.accelerator_memory_mb {
            assert!(accelerator > 0, "{report:?}");
            assert!(accelerator <= report.total_memory_mb, "{report:?}");
        }
    }
}
