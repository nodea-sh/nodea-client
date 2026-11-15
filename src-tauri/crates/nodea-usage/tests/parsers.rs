use nodea_usage::counters::{utilization, CounterDelta, EngineCounters};
use nodea_usage::{drm_fdinfo, ioreg, nvidia, pdh};
use nodea_usage::{NodeaUsage, UsageProbe, UsageSampler, UsageSource};

fn engines(busy: &[(&str, u64)], total: &[(&str, u64)]) -> EngineCounters {
    EngineCounters {
        busy: busy.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        total: total.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
    }
}

// ---- counter math -------------------------------------------------------

#[test]
fn busy_time_over_wall_time_is_the_utilization() {
    let prev = engines(&[("gpu", 1_000)], &[]);
    let cur = engines(&[("gpu", 501_000)], &[]);
    assert_eq!(utilization(&prev, &cur, 1_000_000), 50);
}

#[test]
fn the_busiest_engine_wins() {
    let prev = engines(&[("gfx", 0), ("compute", 0)], &[]);
    let cur = engines(&[("gfx", 100), ("compute", 900)], &[]);
    assert_eq!(utilization(&prev, &cur, 1_000), 90);
}

#[test]
fn cycle_engines_are_measured_against_total_cycles() {
    let prev = engines(&[("rcs", 10)], &[("rcs", 1_000)]);
    let cur = engines(&[("rcs", 260)], &[("rcs", 2_000)]);
    // wall time is ignored for cycle-based engines
    assert_eq!(utilization(&prev, &cur, 1), 25);
}

#[test]
fn an_engine_that_just_appeared_is_not_a_spike() {
    let prev = engines(&[], &[]);
    let cur = engines(&[("gpu", 9_999_999)], &[]);
    assert_eq!(utilization(&prev, &cur, 1_000), 0);
}

#[test]
fn a_counter_that_went_backwards_reads_zero_and_overlap_is_capped() {
    let prev = engines(&[("gpu", 500)], &[]);
    assert_eq!(utilization(&prev, &engines(&[("gpu", 100)], &[]), 1_000), 0);
    // Two command queues busy at once can add up to more than wall time.
    assert_eq!(
        utilization(&prev, &engines(&[("gpu", 5_000)], &[]), 1_000),
        100
    );
}

#[test]
fn delta_needs_two_samples_and_resets_on_a_failed_read() {
    let mut delta = CounterDelta::default();
    assert_eq!(delta.update(Some(engines(&[("gpu", 0)], &[]))), None);
    assert!(delta.update(Some(engines(&[("gpu", 0)], &[]))).is_some());
    assert_eq!(delta.update(None), None);
    assert_eq!(delta.update(Some(engines(&[("gpu", 0)], &[]))), None);
}

// ---- macOS ioreg --------------------------------------------------------

// Trimmed from `ioreg -l -w 0 -r -c IOAccelerator` on an M1 Pro.
const IOREG: &str = r#"+-o AGXAcceleratorG13X  <class AGXAcceleratorG13X, id 0x1000009a2, registered, matched, active, busy 0 (17996 ms), retain 86>
  | {
  |   "PerformanceStatistics" = {"Renderer Utilization %"=44,"Device Utilization %"=45}
  | }
  |
  +-o AGXDeviceUserClient  <class AGXDeviceUserClient, id 0x1006f8cf1, !registered, !matched, active, busy 0, retain 5>
  |   {
  |     "AppUsage" = ({"API"="Metal","lastSubmittedTime"=2368881194677166,"accumulatedGPUTime"=175791},{"API"="Metal","lastSubmittedTime"=0,"accumulatedGPUTime"=0})
  |     "IOUserClientCreator" = "pid 40473, ai4all-tauri"
  |   }
  |
  +-o AGXDeviceUserClient  <class AGXDeviceUserClient, id 0x1006f8cf2, !registered, !matched, active, busy 0, retain 5>
  |   {
  |     "AppUsage" = ({"API"="Metal","lastSubmittedTime"=2368881194677166,"accumulatedGPUTime"=1000})
  |     "IOUserClientCreator" = "pid 40473, ai4all-tauri"
  |   }
  |
  +-o AGXDeviceUserClient  <class AGXDeviceUserClient, id 0x1006f8cf7, !registered, !matched, active, busy 0, retain 5>
  |   {
  |     "AppUsage" = ({"API"="Metal","lastSubmittedTime"=2369796774834833,"accumulatedGPUTime"=4052583})
  |     "IOUserClientCreator" = "pid 404730, com.apple.WebKit"
  |   }
"#;

#[test]
fn ioreg_sums_every_gpu_client_of_the_process_only() {
    let counters = ioreg::parse_process_gpu_time(IOREG, 40473).unwrap();
    // pid 404730 shares a prefix with 40473 and must not be counted.
    assert_eq!(counters.busy.get("gpu"), Some(&176_791));
}

#[test]
fn ioreg_process_without_a_gpu_client_reads_zero() {
    let counters = ioreg::parse_process_gpu_time(IOREG, 1).unwrap();
    assert_eq!(counters.busy.get("gpu"), Some(&0));
}

#[test]
fn ioreg_with_no_clients_at_all_is_no_reading() {
    assert_eq!(
        ioreg::parse_process_gpu_time("+-o AGXAcceleratorG13X  <class>\n", 1),
        None
    );
    assert_eq!(ioreg::parse_process_gpu_time("", 1), None);
}

// ---- Linux DRM fdinfo ---------------------------------------------------

const AMDGPU: &str = "pos:\t0\nflags:\t02100002\nmnt_id:\t26\ndrm-driver:\tamdgpu\ndrm-pdev:\t0000:03:00.0\ndrm-client-id:\t12\ndrm-memory-vram:\t1024 KiB\ndrm-engine-gfx:\t1000 ns\ndrm-engine-compute:\t500000 ns\ndrm-engine-capacity-gfx:\t2\n";
const I915: &str = "drm-driver:\ti915\ndrm-client-id:\t7\ndrm-engine-render:\t25542 ns\ndrm-engine-copy:\t0 ns\ndrm-engine-capacity-video:\t2\n";
const XE: &str = "drm-driver:\txe\ndrm-client-id:\t3\ndrm-cycles-rcs:\t28257900\ndrm-total-cycles-rcs:\t7655183225\ndrm-cycles-ccs:\t0\ndrm-total-cycles-ccs:\t7655183225\n";

#[test]
fn fdinfo_reads_amdgpu_engines_and_skips_capacity() {
    let client = drm_fdinfo::parse_fdinfo(AMDGPU).unwrap();
    assert_eq!(client.client_id, "12");
    assert_eq!(client.counters.busy.get("gfx"), Some(&1000));
    assert_eq!(client.counters.busy.get("compute"), Some(&500_000));
    assert!(!client.counters.busy.contains_key("capacity-gfx"));
}

#[test]
fn fdinfo_reads_i915_and_xe_cycles() {
    assert_eq!(
        drm_fdinfo::parse_fdinfo(I915)
            .unwrap()
            .counters
            .busy
            .get("render"),
        Some(&25542)
    );
    let xe = drm_fdinfo::parse_fdinfo(XE).unwrap();
    assert_eq!(xe.counters.busy.get("rcs"), Some(&28_257_900));
    assert_eq!(xe.counters.total.get("rcs"), Some(&7_655_183_225));
}

#[test]
fn fdinfo_of_a_non_drm_file_is_ignored() {
    assert_eq!(
        drm_fdinfo::parse_fdinfo("pos:\t0\nflags:\t02\nmnt_id:\t15\n"),
        None
    );
}

#[test]
fn fdinfo_clients_shared_by_several_fds_count_once() {
    let a = drm_fdinfo::parse_fdinfo(AMDGPU).unwrap();
    let dup = drm_fdinfo::parse_fdinfo(AMDGPU).unwrap();
    let other =
        drm_fdinfo::parse_fdinfo(&AMDGPU.replace("client-id:\t12", "client-id:\t13")).unwrap();
    let combined = drm_fdinfo::combine([a, dup, other]).unwrap();
    assert_eq!(combined.busy.get("compute"), Some(&1_000_000));
    assert_eq!(drm_fdinfo::combine([]), None);
}

// ---- NVIDIA NVML --------------------------------------------------------

#[test]
fn nvml_takes_this_process_busiest_sample_only() {
    let samples = [(100, 80), (42, 30), (42, 55), (7, 99)];
    assert_eq!(nvidia::max_sm_util_for_pid(samples, 42), 55);
    assert_eq!(nvidia::max_sm_util_for_pid(samples, 5), 0);
}

// ---- Windows PDH --------------------------------------------------------

#[test]
fn pdh_sums_engines_of_a_type_for_the_process_and_takes_the_busiest_type() {
    let items = [
        (
            "pid_1234_luid_0x00000000_0x0000C4F2_phys_0_eng_0_engtype_3D",
            20.0,
        ),
        (
            "pid_1234_luid_0x00000000_0x0000C4F2_phys_0_eng_1_engtype_3D",
            15.0,
        ),
        (
            "pid_1234_luid_0x00000000_0x0000C4F2_phys_0_eng_3_engtype_Compute",
            60.0,
        ),
        (
            "pid_12345_luid_0x00000000_0x0000C4F2_phys_0_eng_3_engtype_Compute",
            99.0,
        ),
        (
            "pid_88_luid_0x00000000_0x0000C4F2_phys_0_eng_0_engtype_3D",
            70.0,
        ),
    ];
    assert_eq!(pdh::process_engine_utilization(items, 1234), 60);
}

#[test]
fn pdh_process_without_engines_reads_zero_and_is_capped() {
    assert_eq!(
        pdh::process_engine_utilization([("pid_9_x_engtype_3D", 50.0)], 1),
        0
    );
    let busy = [("pid_1_a_engtype_3D", 80.0), ("pid_1_b_engtype_3D", 70.0)];
    assert_eq!(pdh::process_engine_utilization(busy, 1), 100);
}

// ---- sampler ------------------------------------------------------------

struct Fixed(Option<u8>);
impl UsageProbe for Fixed {
    fn sample(&mut self) -> Option<u8> {
        self.0
    }
}

#[test]
fn sampler_reports_its_source_and_the_probe_reading() {
    let mut gpu = UsageSampler::with_probe(UsageSource::Gpu, Box::new(Fixed(Some(37))));
    assert_eq!(
        gpu.sample(),
        NodeaUsage {
            source: UsageSource::Gpu,
            percent: Some(37)
        }
    );
    let mut none = UsageSampler::with_probe(UsageSource::Gpu, Box::new(Fixed(None)));
    assert_eq!(none.sample().percent, None);
}

#[test]
fn max_of_reports_the_busiest_probe() {
    let mut probe = nodea_usage::MaxOf(vec![
        Box::new(Fixed(None)),
        Box::new(Fixed(Some(12))),
        Box::new(Fixed(Some(3))),
    ]);
    assert_eq!(probe.sample(), Some(12));
    assert_eq!(
        nodea_usage::MaxOf(vec![Box::new(Fixed(None))]).sample(),
        None
    );
}

#[test]
fn serializes_the_shape_the_frontend_reads() {
    let json = |u| serde_json::to_string(&u).unwrap();
    assert_eq!(
        json(NodeaUsage {
            source: UsageSource::Gpu,
            percent: Some(71)
        }),
        r#"{"source":"gpu","percent":71}"#
    );
    assert_eq!(
        json(NodeaUsage {
            source: UsageSource::Cpu,
            percent: None
        }),
        r#"{"source":"cpu","percent":null}"#
    );
}

// ---- the real platform probe, on whatever machine runs the tests --------

#[test]
fn the_platform_probes_run_and_stay_in_range() {
    let pid = std::process::id();
    let mut gpu = nodea_usage::gpu_probe(pid);
    let mut cpu = nodea_usage::CpuProbe::new(pid);
    for _ in 0..3 {
        if let Some(v) = gpu.sample() {
            assert!(v <= 100);
        }
        if let Some(v) = cpu.sample() {
            assert!(v <= 100);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
