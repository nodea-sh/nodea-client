//! Prints a process's GPU (or CPU) usage once a second, as the Node page
//! would show it, so each platform probe can be checked against the OS's own
//! tool (Activity Monitor, nvtop/nvidia-smi pmon, Task Manager).
//!
//!   cargo run -p nodea-usage --example usage_probe -- --pid <PID> [--cpu]
//!
//! Without --pid it watches itself (expect 0%).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pid = args
        .iter()
        .position(|a| a == "--pid")
        .and_then(|i| args.get(i + 1))
        .map(|p| p.parse::<u32>().expect("--pid takes a number"))
        .unwrap_or_else(std::process::id);
    let has_gpu = !args.iter().any(|a| a == "--cpu");

    let mut sampler = nodea_usage::UsageSampler::new(pid, has_gpu);
    println!(
        "watching pid {pid} ({}), Ctrl-C to stop",
        if has_gpu { "GPU" } else { "CPU" }
    );
    loop {
        let usage = sampler.sample();
        match usage.percent {
            Some(p) => println!("{:?} {p:>3}%", usage.source),
            None => println!("{:?}   —  (no reading)", usage.source),
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
