//! Windows: per-process GPU utilization from the `GPU Engine` counters.
//!
//! WDDM publishes one PDH counter instance per process and GPU engine, named
//! like `pid_1234_luid_0x0_0x1A2B_phys_0_eng_0_engtype_3D`, whose
//! `Utilization Percentage` is that process's share of the engine. Task
//! Manager's per-process "GPU" column is the busiest engine type, summing the
//! instances of that type. Vulkan and DirectX work both show up here.

use crate::clamp_pct;
use std::collections::BTreeMap;

/// Nodea's GPU %: of the `(instance name, utilization %)` items, those for
/// `pid`, summed per engine type, busiest type wins. 0 when there are none
/// (the process has not used the GPU).
pub fn process_engine_utilization<'a>(
    items: impl IntoIterator<Item = (&'a str, f64)>,
    pid: u32,
) -> u8 {
    let prefix = format!("pid_{pid}_");
    let mut per_type: BTreeMap<&str, f64> = BTreeMap::new();
    for (name, value) in items {
        if !name.starts_with(&prefix) || !value.is_finite() {
            continue;
        }
        let engine_type = name.rsplit_once("engtype_").map(|(_, t)| t).unwrap_or("");
        *per_type.entry(engine_type).or_default() += value;
    }
    clamp_pct(per_type.values().copied().fold(0.0_f64, f64::max))
}

#[cfg(windows)]
pub use probe::PdhProbe;

#[cfg(windows)]
mod probe {
    use windows::core::PCWSTR;
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhOpenQueryW, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE,
        PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
    };

    const ERROR_SUCCESS: u32 = 0;

    pub struct PdhProbe {
        pid: u32,
        query: PDH_HQUERY,
        counter: PDH_HCOUNTER,
        ok: bool,
        collected_once: bool,
    }

    // The handles are only used through &mut self, from one thread at a time.
    unsafe impl Send for PdhProbe {}

    impl PdhProbe {
        pub fn new(pid: u32) -> Self {
            let mut query = PDH_HQUERY(std::ptr::null_mut());
            let mut counter = PDH_HCOUNTER(std::ptr::null_mut());
            // English path so it works on every display language, as a
            // null-terminated UTF-16 buffer that outlives the call.
            let path: Vec<u16> = r"\GPU Engine(*)\Utilization Percentage"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let ok = unsafe {
                PdhOpenQueryW(PCWSTR::null(), 0, &mut query) == ERROR_SUCCESS
                    && PdhAddEnglishCounterW(query, PCWSTR(path.as_ptr()), 0, &mut counter)
                        == ERROR_SUCCESS
            };
            Self {
                pid,
                query,
                counter,
                ok,
                collected_once: false,
            }
        }

        fn read(&mut self) -> Option<u8> {
            if !self.ok {
                return None;
            }
            if unsafe { PdhCollectQueryData(self.query) } != ERROR_SUCCESS {
                return None;
            }
            // A rate counter needs two collections before it has a value.
            if !self.collected_once {
                self.collected_once = true;
                return None;
            }
            let mut size = 0u32;
            let mut count = 0u32;
            let status = unsafe {
                PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &mut size,
                    &mut count,
                    None,
                )
            };
            if status != PDH_MORE_DATA {
                // No instances at all: nothing is using any GPU engine.
                return (status == ERROR_SUCCESS).then_some(0);
            }
            // The buffer holds the items followed by their name strings.
            let words =
                (size as usize).div_ceil(std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>()) + 1;
            let mut buffer = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); words];
            let status = unsafe {
                PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &mut size,
                    &mut count,
                    Some(buffer.as_mut_ptr()),
                )
            };
            if status != ERROR_SUCCESS {
                return None;
            }
            let items: Vec<(String, f64)> = buffer[..count as usize]
                .iter()
                .filter(|item| item.FmtValue.CStatus == PDH_CSTATUS_VALID_DATA)
                .filter_map(|item| {
                    let name = unsafe { item.szName.to_string() }.ok()?;
                    let value = unsafe { item.FmtValue.Anonymous.doubleValue };
                    Some((name, value))
                })
                .collect();
            Some(super::process_engine_utilization(
                items.iter().map(|(n, v)| (n.as_str(), *v)),
                self.pid,
            ))
        }
    }

    impl crate::UsageProbe for PdhProbe {
        fn sample(&mut self) -> Option<u8> {
            self.read()
        }
    }

    impl Drop for PdhProbe {
        fn drop(&mut self) {
            if self.ok {
                unsafe {
                    PdhCloseQuery(self.query);
                }
            }
        }
    }
}
