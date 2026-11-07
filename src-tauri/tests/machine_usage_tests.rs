use ai4all_tauri::machine_usage::{
    choose, parse_busy_percent, parse_ioreg_utilization, parse_nvidia_smi, MachineUsage, UsageSampler,
    UsageSource,
};

const APPLE_SILICON_IOREG: &str = r#"+-o AGXAcceleratorG13X  <class AGXAcceleratorG13X, id 0x1000005cd>
    {
      "PerformanceStatistics" = {"In use system memory (driver)"=0,"Alloc system memory"=1473904640,"Tiled Scene Bytes"=1310720,"Renderer Utilization %"=44,"Device Utilization %"=45,"recoveryCount"=0}
      "IOClass" = "AGXAcceleratorG13X"
    }
"#;

#[test]
fn ioreg_reads_device_utilization_not_renderer() {
    assert_eq!(parse_ioreg_utilization(APPLE_SILICON_IOREG), Some(45));
}

#[test]
fn ioreg_takes_the_busiest_of_several_gpus() {
    let two = r#""Device Utilization %"=3 ... "Device Utilization %"=87"#;
    assert_eq!(parse_ioreg_utilization(two), Some(87));
}

#[test]
fn ioreg_without_the_key_is_no_reading() {
    assert_eq!(parse_ioreg_utilization("+-o IOAccelerator {}"), None);
    assert_eq!(parse_ioreg_utilization(""), None);
}

#[test]
fn ioreg_idle_gpu_reads_zero() {
    assert_eq!(parse_ioreg_utilization(r#""Device Utilization %"=0,"#), Some(0));
}

#[test]
fn nvidia_smi_takes_the_busiest_gpu() {
    assert_eq!(parse_nvidia_smi("12\n 97 \n"), Some(97));
    assert_eq!(parse_nvidia_smi("[N/A]\n"), None);
    assert_eq!(parse_nvidia_smi(""), None);
}

#[test]
fn amd_busy_percent_is_parsed_and_clamped() {
    assert_eq!(parse_busy_percent("50\n"), Some(50));
    assert_eq!(parse_busy_percent("130"), Some(100));
    assert_eq!(parse_busy_percent("garbage"), None);
}

#[test]
fn a_gpu_reading_wins_over_cpu() {
    assert_eq!(choose(Some(0), 90.0), MachineUsage { source: UsageSource::Gpu, percent: 0 });
}

#[test]
fn without_a_gpu_reading_cpu_is_used_and_rounded() {
    assert_eq!(choose(None, 49.6), MachineUsage { source: UsageSource::Cpu, percent: 50 });
    assert_eq!(choose(None, f32::NAN), MachineUsage { source: UsageSource::Cpu, percent: 0 });
}

#[test]
fn serializes_the_shape_the_frontend_reads() {
    let json = serde_json::to_string(&MachineUsage { source: UsageSource::Gpu, percent: 71 }).unwrap();
    assert_eq!(json, r#"{"source":"gpu","percent":71}"#);
}

#[test]
fn sampling_the_real_machine_stays_in_range() {
    let sampler = UsageSampler::new();
    for _ in 0..2 {
        assert!(sampler.sample().percent <= 100);
    }
}
