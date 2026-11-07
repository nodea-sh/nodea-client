use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error {
        message: String,
    },
    DeviceLimitReached {
        message: String,
    },
    /// The server refused this account: it was deleted, or an admin
    /// deactivated it. Terminal, like DeviceLimitReached - the same token
    /// would be refused again - and distinct from Error, which the UI reads
    /// as "can't reach the coordinator". `message` is the server's own
    /// reason ("Account deleted", "Account is inactive").
    Refused {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ModelStatus {
    NoModel,
    Downloading {
        model_name: String,
        progress: u8,
        /// 0 for both until the response headers give a size.
        #[serde(default)]
        downloaded_bytes: u64,
        #[serde(default)]
        total_bytes: u64,
        /// Over a rolling window (download_rate.rs); 0 until it has one.
        #[serde(default)]
        bytes_per_sec: u64,
    },
    Loading {
        model_name: String,
        /// Unix ms, so a UI that mounts mid-load still counts from the start.
        #[serde(default)]
        started_at_ms: u64,
    },
    Ready {
        model_name: String,
        /// How long the load took; None when nothing was loaded (already
        /// resident) or the load was not timed.
        #[serde(default)]
        load_ms: Option<u32>,
    },
    Error {
        message: String,
    },
}

/// Now, as the Unix ms `Loading::started_at_ms` carries.
pub fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// What llama.cpp runs on, for the UI. Every layer is offloaded
/// (`n_gpu_layers: u32::MAX`), so with a usable device the model runs on its
/// GPU backend; without one - or in a build with neither backend - on the CPU.
pub fn inference_backend(has_accelerator: bool) -> &'static str {
    if !has_accelerator {
        "CPU"
    } else if cfg!(target_os = "macos") {
        "Metal"
    } else if cfg!(feature = "vulkan") {
        "Vulkan"
    } else {
        "CPU"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub connection: ConnectionStatus,
    pub model: ModelStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_status_reaches_the_ui_with_its_type_and_reason() {
        // useNodePhase.ts switches on `type` and shows `message`.
        let json = serde_json::to_value(ConnectionStatus::Refused {
            message: "Account is inactive".to_string(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "Refused", "message": "Account is inactive"})
        );
    }

    #[test]
    fn download_progress_reaches_the_ui_in_bytes() {
        // useNodePhase.ts reads these names.
        let json = serde_json::to_value(ModelStatus::Downloading {
            model_name: "m.gguf".to_string(),
            progress: 25,
            downloaded_bytes: 1_000,
            total_bytes: 4_000,
            bytes_per_sec: 500,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "Downloading", "model_name": "m.gguf", "progress": 25,
                "downloaded_bytes": 1000, "total_bytes": 4000, "bytes_per_sec": 500
            })
        );
    }

    #[test]
    fn load_timing_reaches_the_ui() {
        let loading = serde_json::to_value(ModelStatus::Loading {
            model_name: "m.gguf".to_string(),
            started_at_ms: 42,
        })
        .unwrap();
        assert_eq!(
            loading,
            serde_json::json!({"type": "Loading", "model_name": "m.gguf", "started_at_ms": 42})
        );

        let ready = serde_json::to_value(ModelStatus::Ready {
            model_name: "m.gguf".to_string(),
            load_ms: Some(12_400),
        })
        .unwrap();
        assert_eq!(
            ready,
            serde_json::json!({"type": "Ready", "model_name": "m.gguf", "load_ms": 12400})
        );
    }

    #[test]
    fn no_accelerator_means_cpu() {
        assert_eq!(inference_backend(false), "CPU");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn an_accelerator_on_macos_is_metal() {
        assert_eq!(inference_backend(true), "Metal");
    }
}
