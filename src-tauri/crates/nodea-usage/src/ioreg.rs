//! macOS: per-process GPU time from IOKit.
//!
//! Every process that opens the GPU gets a user client under the accelerator
//! in the IORegistry. Its `IOUserClientCreator` names the pid, and `AppUsage`
//! lists, per command queue, `accumulatedGPUTime`: nanoseconds of GPU time the
//! process has used. This is the figure behind Activity Monitor's "% GPU"
//! column, and reading it needs no special rights.

use crate::counters::EngineCounters;

/// Total `accumulatedGPUTime` of `pid`'s GPU clients in
/// `ioreg -l -w 0 -r -c IOAccelerator` output, as one "gpu" engine.
/// `None` when the output lists no GPU clients at all (nothing to read), and
/// zero when it does but none belongs to `pid` (Nodea has not used the GPU).
pub fn parse_process_gpu_time(output: &str, pid: u32) -> Option<EngineCounters> {
    let creator = format!("\"IOUserClientCreator\" = \"pid {pid},");
    let mut saw_any_client = false;
    let mut total: u64 = 0;

    // Each registry object starts with "+-o"; its properties follow until
    // the next one.
    for object in output.split("+-o ") {
        if !object.contains("\"IOUserClientCreator\"") {
            continue;
        }
        saw_any_client = true;
        if !object.contains(&creator) {
            continue;
        }
        total = total.saturating_add(sum_accumulated_gpu_time(object));
    }

    saw_any_client.then(|| {
        let mut counters = EngineCounters::default();
        counters.busy.insert("gpu".to_string(), total);
        counters
    })
}

fn sum_accumulated_gpu_time(object: &str) -> u64 {
    const KEY: &str = "\"accumulatedGPUTime\"=";
    object
        .match_indices(KEY)
        .filter_map(|(at, _)| {
            object[at + KEY.len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .ok()
        })
        .fold(0u64, |acc, v| acc.saturating_add(v))
}

#[cfg(target_os = "macos")]
pub struct IoregProbe {
    pid: u32,
    delta: crate::counters::CounterDelta,
}

#[cfg(target_os = "macos")]
impl IoregProbe {
    pub fn new(pid: u32) -> Self {
        Self {
            pid,
            delta: Default::default(),
        }
    }

    fn read(&self) -> Option<EngineCounters> {
        let out = std::process::Command::new("ioreg")
            .args(["-l", "-w", "0", "-r", "-c", "IOAccelerator"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_process_gpu_time(&String::from_utf8_lossy(&out.stdout), self.pid)
    }
}

#[cfg(target_os = "macos")]
impl crate::UsageProbe for IoregProbe {
    fn sample(&mut self) -> Option<u8> {
        let counters = self.read();
        self.delta.update(counters)
    }
}
