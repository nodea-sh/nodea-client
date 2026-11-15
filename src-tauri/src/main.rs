// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_updater;
mod auth;
mod grpc_client;

// The node's core lives in the library (see lib.rs) so integration tests can
// reach it; these re-exports keep `crate::…` paths working in the binary.
pub use ai4all_tauri::{devlog, model_manager, openai_request, proto, protocol, reconnect, status};

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::{Emitter, Manager, WindowEvent, Wry};
use tokio::sync::Mutex;

const TRAY_STATUS_ID: &str = "status";
const TRAY_TOGGLE_SHARING_ID: &str = "toggle_sharing";
const TRAY_UPDATE_ID: &str = "update";

/// The tray menu items `refresh_tray` and the updater rewrite. v2 has no
/// lookup by id, so the handles are kept in managed state.
struct TrayItems {
    status: MenuItem<Wry>,
    toggle_sharing: CheckMenuItem<Wry>,
    update: MenuItem<Wry>,
}

/// Builds the menu-bar tray's menu: a disabled status line, a serving/pause
/// toggle mirroring the app's own Sharing switch, and Open/Quit. The icon
/// itself is declared in tauri.conf.json (`app.trayIcon`, id "main").
/// Closing the main window keeps the app running behind this tray icon - see
/// the `on_window_event` hookup in `main()`'s `.setup()`.
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, TRAY_STATUS_ID, "Status: Offline", false, None::<&str>)?;
    let toggle_sharing = CheckMenuItem::with_id(
        app,
        TRAY_TOGGLE_SHARING_ID,
        "Sharing",
        true,
        false,
        None::<&str>,
    )?;
    let open = MenuItem::with_id(app, "open", "Open Nodea", true, None::<&str>)?;
    let (update_label, update_enabled) = ai4all_tauri::updater::UpdateState::Idle.tray_label();
    let update = MenuItem::with_id(
        app,
        TRAY_UPDATE_ID,
        update_label,
        update_enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &toggle_sharing,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &update,
            &quit,
        ],
    )?;

    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))?;
    }
    app.manage(TrayItems {
        status,
        toggle_sharing,
        update,
    });
    Ok(())
}

/// One of the four states the tray's status line shows - a coarser view
/// than the app's own six-phase `useNodePhase` (paused and never-connected
/// both read as "Offline" here; the tray doesn't need that distinction).
fn tray_status_label(
    connection: &status::ConnectionStatus,
    model: &status::ModelStatus,
) -> &'static str {
    match model {
        status::ModelStatus::Downloading { .. } => "Downloading",
        status::ModelStatus::Loading { .. } => "Loading",
        status::ModelStatus::Ready { .. }
            if matches!(connection, status::ConnectionStatus::Connected) =>
        {
            "Serving"
        }
        _ => "Offline",
    }
}

/// Refreshes both dynamic tray menu items from current state. Called
/// whenever connection/model status changes (see `set_connection_status`/
/// `set_model_status`) so the tray never disagrees with the app's own
/// status widget - the same single-source-of-truth principle as
/// `useNodePhase` on the frontend.
async fn refresh_tray(state: &AppState, app_handle: &tauri::AppHandle) {
    let connection = state.connection_status.lock().await.clone();
    let model = state.model_status.lock().await.clone();

    let label = tray_status_label(&connection, &model);
    let Some(items) = app_handle.try_state::<TrayItems>() else {
        return;
    };
    let _ = items.status.set_text(format!("Status: {}", label));

    // A checkbox-style item (native checkmark), not a text swap - "Sharing"
    // stays put, checked means currently serving.
    let _ = items.toggle_sharing.set_checked(label == "Serving");
}

fn handle_menu_event(app_handle: &tauri::AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "open" => {
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        TRAY_TOGGLE_SHARING_ID => {
            let app_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let currently_serving = {
                    let state = app_handle.state::<AppState>();
                    let connection = state.connection_status.lock().await.clone();
                    let model = state.model_status.lock().await.clone();
                    tray_status_label(&connection, &model) == "Serving"
                };

                if currently_serving {
                    let _ =
                        disconnect_grpc(app_handle.state::<AppState>(), app_handle.clone()).await;
                } else {
                    let _ = connect_grpc(app_handle.state::<AppState>(), app_handle.clone()).await;
                }

                // The frontend's own Sharing toggle (src/hooks/useNodePhase.ts)
                // only changes when the user clicks the widget - it doesn't
                // otherwise watch connection state, so without this the app UI
                // would show stale "Sharing" after a pause triggered from the tray.
                let _ = app_handle.emit("sharing-changed", !currently_serving);
            });
        }
        TRAY_UPDATE_ID => {
            tauri::async_runtime::spawn(app_updater::tray_clicked(app_handle.clone()));
        }
        "quit" => {
            // A ready update installs on the way out (RunEvent::Exit in main()).
            app_handle.exit(0);
        }
        _ => {}
    }
}

#[derive(Clone, serde::Serialize)]
struct MessageLogEntry {
    timestamp: String,
    log_type: String,     // "rest" or "grpc"
    message_type: String, // For gRPC: "Connection", "ChatCompletionRequest", etc.
    // For REST: HTTP method "GET", "POST", etc.
    content: String, // For gRPC: description
    // For REST: URL path

    // REST-specific optional fields
    #[serde(skip_serializing_if = "Option::is_none")]
    status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
}

// Derive Clone to make AppState compatible with Tauri's state management
#[derive(Clone)]
struct AppState {
    access_token: Arc<Mutex<Option<String>>>,
    refresh_token: Arc<Mutex<Option<String>>>,
    device_id: Arc<String>, // Wrap in Arc for Clone
    message_log: Arc<Mutex<Vec<MessageLogEntry>>>,
    model_manager: model_manager::SharedModelManager,
    connection_status: Arc<Mutex<status::ConnectionStatus>>,
    model_status: Arc<Mutex<status::ModelStatus>>,
    // Cancels the currently-running connection's heartbeat + receive loops
    // (see grpc_client.rs) - disconnect_grpc uses this to actually tear the
    // connection down instead of just changing the status label, which used
    // to leave the old stream (and its heartbeat task) running forever.
    active_connection_cancel: Arc<Mutex<Option<tokio_util::sync::CancellationToken>>>,
    // In-flight generations: per-request cancellation, and the gate a model
    // switch closes to drain them (see generations.rs).
    generations: Arc<ai4all_tauri::generations::Generations>,
    // Also inside model_manager, but a download has to reach it without
    // taking that lock (see reload.rs).
    model_cache_dir: Arc<PathBuf>,
    // Held for a whole model switch. A reconnect re-sends LoadModel, and with
    // the download outside the model_manager lock nothing else would stop two
    // switches downloading the same file at once.
    model_switch: Arc<Mutex<()>>,
    // Created on first use: picking GPU or CPU probes the accelerator.
    usage_sampler: Arc<std::sync::Mutex<Option<nodea_usage::UsageSampler>>>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredTokens {
    access_token: String,
    refresh_token: String,
}

/// Login with email and password
#[tauri::command]
async fn login_email_password(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    email: String,
    password: String,
) -> Result<String, String> {
    let auth_result = auth::authenticate_email_password(email, password)
        .await
        .map_err(|e| e.to_string())?;

    // Update in-memory state
    let mut token = state.access_token.lock().await;
    *token = Some(auth_result.access_token.clone());
    drop(token);

    let mut refresh = state.refresh_token.lock().await;
    *refresh = Some(auth_result.refresh_token.clone());
    drop(refresh);

    // Save to persistent store
    if let Err(e) = save_tokens_to_store(
        &app_handle,
        &auth_result.access_token,
        &auth_result.refresh_token,
    )
    .await
    {
        eprintln!("Failed to save tokens to store: {}", e);
    }

    Ok(auth_result.access_token)
}

/// Login with OAuth provider (google, github, etc.)
#[tauri::command]
async fn login_oauth(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    provider: String,
) -> Result<String, String> {
    let auth_result = auth::authenticate_oauth(&provider)
        .await
        .map_err(|e| e.to_string())?;

    // Update in-memory state
    let mut token = state.access_token.lock().await;
    *token = Some(auth_result.access_token.clone());
    drop(token);

    let mut refresh = state.refresh_token.lock().await;
    *refresh = Some(auth_result.refresh_token.clone());
    drop(refresh);

    // Save to persistent store
    if let Err(e) = save_tokens_to_store(
        &app_handle,
        &auth_result.access_token,
        &auth_result.refresh_token,
    )
    .await
    {
        eprintln!("Failed to save tokens to store: {}", e);
    }

    Ok(auth_result.access_token)
}

/// Sign up a new user with email and password.
///
/// No session is created or stored here - the backend creates the account
/// and emails our own verification link. The caller logs in next with the
/// same credentials; it does not have to wait for the link.
#[tauri::command]
async fn sign_up(email: String, password: String) -> Result<String, String> {
    auth::sign_up_email_password(email, password)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn logout(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    // Clear in-memory state
    let mut token = state.access_token.lock().await;
    *token = None;
    drop(token);

    let mut refresh = state.refresh_token.lock().await;
    *refresh = None;
    drop(refresh);

    let mut log = state.message_log.lock().await;
    log.clear();
    drop(log);

    // Clear persistent store
    if let Err(e) = clear_tokens_from_store(&app_handle).await {
        eprintln!("Failed to clear tokens from store: {}", e);
    }

    Ok(())
}

#[tauri::command]
async fn is_authenticated(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let token = state.access_token.lock().await;
    Ok(token.is_some())
}

/// This machine's persistent device id, so the frontend can highlight "this
/// Mac" in the org's device list without guessing by name.
#[tauri::command]
fn get_device_id(state: tauri::State<'_, AppState>) -> String {
    state.device_id.as_ref().clone()
}

/// How much of this device's GPU (CPU on a CPU-only build) Nodea itself is
/// using, for the Node page's usage ring. Other apps do not count.
#[tauri::command]
async fn nodea_usage(state: tauri::State<'_, AppState>) -> Result<nodea_usage::NodeaUsage, String> {
    let sampler = state.usage_sampler.clone();
    tokio::task::spawn_blocking(move || {
        let mut sampler = sampler.lock().unwrap_or_else(|e| e.into_inner());
        sampler
            .get_or_insert_with(|| {
                let has_gpu = ai4all_tauri::memory::probe()
                    .accelerator_memory_mb
                    .is_some();
                nodea_usage::UsageSampler::new(std::process::id(), has_gpu)
            })
            .sample()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Helper function to update connection status and emit event
pub(crate) async fn set_connection_status(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    new_status: status::ConnectionStatus,
) {
    let mut status = state.connection_status.lock().await;
    *status = new_status.clone();
    drop(status);

    if let Err(e) = app_handle.emit("connection-status-changed", &new_status) {
        eprintln!("Failed to emit connection status event: {}", e);
    }

    refresh_tray(state, app_handle).await;
}

/// Helper function to update model status and emit event
pub(crate) async fn set_model_status(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    new_status: status::ModelStatus,
) {
    let mut status = state.model_status.lock().await;
    *status = new_status.clone();
    drop(status);

    if let Err(e) = app_handle.emit("model-status-changed", &new_status) {
        eprintln!("Failed to emit model status event: {}", e);
    }

    refresh_tray(state, app_handle).await;
}

/// Get current status snapshot
#[tauri::command]
async fn get_all_status(
    state: tauri::State<'_, AppState>,
) -> Result<status::StatusSnapshot, String> {
    let connection = state.connection_status.lock().await.clone();
    let model = state.model_status.lock().await.clone();

    Ok(status::StatusSnapshot { connection, model })
}

#[tauri::command]
async fn connect_grpc(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    // Only a presence check: connect_with_retry reads the token itself, per
    // attempt, so a refresh mid-retry actually reaches the next attempt.
    if state.access_token.lock().await.is_none() {
        return Err("Not authenticated".to_string());
    }

    let device_id = state.device_id.as_ref().clone();
    let state_clone = state.inner().clone();

    // Cancel any still-running previous connection before starting a fresh
    // one (e.g. a quick pause/resume) - see CancellationToken usage in
    // grpc_client.rs. Without this, resuming sharing could open a second
    // live stream for the same device alongside one disconnect_grpc never
    // actually tore down, and the engine would try to load a second copy of
    // the model into memory (see handle_load_model's is_model_loaded_and_matches
    // guard for the other half of that fix).
    let cancel_token = tokio_util::sync::CancellationToken::new();
    {
        let mut slot = state.active_connection_cancel.lock().await;
        if let Some(previous) = slot.take() {
            previous.cancel();
        }
        *slot = Some(cancel_token.clone());
    }

    tokio::spawn(async move {
        println!("Starting gRPC connection in background task...");

        let error_msg = {
            let result = grpc_client::connect_and_stream(
                device_id.clone(),
                state_clone.clone(),
                app_handle.clone(),
                cancel_token,
            )
            .await;

            if let Err(e) = result {
                let msg = format!("gRPC connection error for device {}: {}", device_id, e);
                eprintln!("❌ {}", msg);
                Some(msg)
            } else {
                println!(
                    "gRPC connection task completed successfully for device {}",
                    device_id
                );
                None
            }
        };

        if let Some(error_message) = error_msg {
            // Only a terminal failure gets here now that retries are unbounded,
            // and the connection loop has already set the status that explains
            // it. Overwriting a DeviceLimitReached with a generic Error would
            // replace the one message that tells the user what to do about it.
            let already_explained = matches!(
                *state_clone.connection_status.lock().await,
                status::ConnectionStatus::DeviceLimitReached { .. }
                    | status::ConnectionStatus::Refused { .. }
            );

            if !already_explained {
                set_connection_status(
                    &state_clone,
                    &app_handle,
                    status::ConnectionStatus::Error {
                        message: error_message,
                    },
                )
                .await;
            }
        }
    });

    Ok(())
}

#[tauri::command]
async fn disconnect_grpc(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    println!("Disconnecting gRPC connection...");

    // Actually tear down the running connection - the heartbeat task and the
    // message receive loop in grpc_client.rs both watch this token. Without
    // this, "disconnect" only ever changed the status label: the old stream
    // and its heartbeat kept running, and re-enabling sharing opened a
    // second stream for the same device_id alongside the still-live first
    // one, which the server then rejected (surfacing as the "stalled" phase
    // that Retry couldn't fix, since retrying just opened a third stream).
    if let Some(cancel_token) = state.active_connection_cancel.lock().await.take() {
        cancel_token.cancel();
    }

    // Set status to Disconnected
    // This will also signal any retry loop to stop
    set_connection_status(&state, &app_handle, status::ConnectionStatus::Disconnected).await;

    println!("gRPC connection set to Disconnected");
    Ok(())
}

#[tauri::command]
async fn get_message_log(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<MessageLogEntry>, String> {
    let log = state.message_log.lock().await;
    Ok(log.clone())
}

// A Tauri command's parameters are the frontend's invoke() payload.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn log_rest_api_call(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    method: String,
    url: String,
    status_code: u16,
    request_body: Option<String>,
    response_body: Option<String>,
    duration_ms: u64,
) -> Result<(), String> {
    // Bodies can carry passwords and newly created API keys.
    let request_body = request_body.as_deref().map(devlog::redact_body);
    let response_body = response_body.as_deref().map(devlog::redact_body);

    if status_code == 0 || status_code >= 400 {
        eprintln!(
            "[rest] {} {} -> {} ({}ms): {}",
            method,
            url,
            status_code,
            duration_ms,
            response_body.as_deref().unwrap_or("<no response body>")
        );
    } else {
        println!(
            "[rest] {} {} -> {} ({}ms)",
            method, url, status_code, duration_ms
        );
    }

    let entry = MessageLogEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        log_type: "rest".to_string(),
        message_type: method.clone(),
        content: url.clone(),
        status_code: Some(status_code),
        request_body,
        response_body,
        duration_ms: Some(duration_ms),
    };

    let mut log = state.message_log.lock().await;
    devlog::push_capped(&mut log, entry.clone());

    // Emit event to frontend
    if let Err(e) = app_handle.emit("new-message", &entry) {
        eprintln!("Failed to emit log message event: {}", e);
    }

    Ok(())
}

/// Generic bridge for frontend-side events that never reach a REST call or
/// gRPC message (e.g. UI guard clauses that silently no-op) - printed to
/// stdout/stderr so they show up in the same logs as the rest of the app,
/// since the webview's own console isn't visible outside devtools.
#[tauri::command]
async fn log_frontend_event(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    level: String,
    message: String,
) -> Result<(), String> {
    if level == "error" || level == "warn" {
        eprintln!("[frontend:{}] {}", level, message);
    } else {
        println!("[frontend:{}] {}", level, message);
    }

    let entry = MessageLogEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        log_type: "frontend".to_string(),
        message_type: level,
        content: message,
        status_code: None,
        request_body: None,
        response_body: None,
        duration_ms: None,
    };

    let mut log = state.message_log.lock().await;
    log.push(entry.clone());

    if let Err(e) = app_handle.emit("new-message", &entry) {
        eprintln!("Failed to emit log message event: {}", e);
    }

    Ok(())
}

/// Load a model from a local path
#[tauri::command]
async fn load_model(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    model_path: String,
) -> Result<String, String> {
    println!("Load model command called with path: {}", model_path);

    // Extract model name from path
    let model_name = std::path::Path::new(&model_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Set loading status
    set_model_status(
        &state,
        &app_handle,
        status::ModelStatus::Loading {
            model_name: model_name.clone(),
            started_at_ms: status::unix_ms_now(),
        },
    )
    .await;

    let mut model_mgr = state.model_manager.lock().await;
    let started = std::time::Instant::now();
    let result = model_mgr.load_model(model_path).await;
    let load_ms = Some(started.elapsed().as_millis() as u32);

    match result {
        Ok(_) => {
            set_model_status(
                &state,
                &app_handle,
                status::ModelStatus::Ready {
                    model_name,
                    load_ms,
                },
            )
            .await;
            Ok("Model loaded successfully".to_string())
        }
        Err(e) => {
            set_model_status(
                &state,
                &app_handle,
                status::ModelStatus::Error { message: e.clone() },
            )
            .await;
            Err(e)
        }
    }
}

/// Generate text from a prompt
#[tauri::command]
async fn generate_text(
    state: tauri::State<'_, AppState>,
    prompt: String,
    max_tokens: Option<usize>,
) -> Result<String, String> {
    println!(
        "generate_text command: {}",
        model_manager::describe_prompt(&prompt, max_tokens.unwrap_or(100))
    );

    let model_mgr = state.model_manager.lock().await;

    if !model_mgr.is_loaded() {
        return Err("Model not loaded. Please load a model first.".to_string());
    }

    let tokens = max_tokens.unwrap_or(100);
    let response = model_mgr.generate(&prompt, tokens).await?;

    Ok(response)
}

/// What llama.cpp runs on here ("Metal", "Vulkan" or "CPU"), for the UI.
#[tauri::command]
fn get_inference_backend() -> &'static str {
    status::inference_backend(
        ai4all_tauri::memory::probe()
            .accelerator_memory_mb
            .is_some(),
    )
}

/// Check if a model is currently loaded
#[tauri::command]
async fn is_model_loaded(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let model_mgr = state.model_manager.lock().await;
    Ok(model_mgr.is_loaded())
}

#[derive(serde::Serialize)]
struct ModelCacheInfo {
    path: String,
    size_bytes: u64,
}

fn dir_size_bytes(dir: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total += dir_size_bytes(&path);
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

/// Where models are cached on disk, and how much space they currently use -
/// backs the Settings page's "Model cache" row (path + Reveal button).
#[tauri::command]
async fn get_model_cache_info(state: tauri::State<'_, AppState>) -> Result<ModelCacheInfo, String> {
    let model_mgr = state.model_manager.lock().await;
    let dir = model_mgr.cache_dir();
    Ok(ModelCacheInfo {
        path: dir.to_string_lossy().to_string(),
        size_bytes: dir_size_bytes(dir),
    })
}

#[derive(serde::Serialize)]
struct SystemMemory {
    total_memory_mb: u32,
    available_memory_mb: u32,
    accelerator_memory_mb: Option<u32>,
}

/// The same memory figures this node reports in its DeviceIdentity - the app
/// uses them only to suggest models that fit when an admin adds one.
#[tauri::command]
fn get_system_memory() -> SystemMemory {
    let m = ai4all_tauri::memory::probe();
    SystemMemory {
        total_memory_mb: m.total_memory_mb,
        available_memory_mb: m.available_memory_mb,
        accelerator_memory_mb: m.accelerator_memory_mb,
    }
}

/// Open the model cache folder in the system file manager. The path comes
/// from the app's own state, never from the webview, so shell.open's scope can
/// stay limited to web URLs.
#[tauri::command]
async fn reveal_model_cache(state: tauri::State<'_, AppState>) -> Result<(), String> {
    open::that(state.model_cache_dir.as_path())
        .map_err(|e| format!("Failed to open the model cache folder: {}", e))
}

/// Load model with auto-download if not cached
#[tauri::command]
async fn load_model_auto(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    model_url: String,
    model_name: String,
) -> Result<String, String> {
    println!("Load model auto command called");
    println!("  URL: {}", model_url);
    println!("  Name: {}", model_name);
    ai4all_tauri::catalogue::validate_model_file_name(&model_name)?;

    let mut model_mgr = state.model_manager.lock().await;

    // Check if model is cached
    let model_path = if model_mgr.is_model_cached(&model_name) {
        println!("Model found in cache: {}", model_name);
        // Model is cached, skip download and go straight to loading
        set_model_status(
            &state,
            &app_handle,
            status::ModelStatus::Loading {
                model_name: model_name.clone(),
                started_at_ms: status::unix_ms_now(),
            },
        )
        .await;
        model_mgr.get_cached_model_path(&model_name)
    } else {
        // Need to download first
        println!("Model not in cache, downloading...");

        // Set initial downloading status
        set_model_status(
            &state,
            &app_handle,
            status::ModelStatus::Downloading {
                model_name: model_name.clone(),
                progress: 0,
                downloaded_bytes: 0,
                total_bytes: 0,
                bytes_per_sec: 0,
            },
        )
        .await;

        // Create progress callback that emits both events
        let app_handle_clone = app_handle.clone();
        let model_name_clone = model_name.clone();
        let state_clone = state.inner().clone();
        let rate = std::sync::Mutex::new(ai4all_tauri::download_rate::RateMeter::default());

        let progress_callback = move |progress: model_manager::DownloadProgress| {
            println!(
                "Download progress: {:.1}% - {}",
                progress.percentage, progress.status
            );

            // Emit legacy event for compatibility
            app_handle_clone
                .emit("model-download-progress", progress.clone())
                .ok();

            // Only update status to "Downloading" if not complete (< 100%)
            // At 100%, we'll explicitly set "Loading" status after download_model returns
            if progress.percentage < 100.0 {
                let bytes_per_sec = rate
                    .lock()
                    .map(|mut meter| {
                        meter.record(std::time::Instant::now(), progress.downloaded_bytes)
                    })
                    .unwrap_or(0);
                let app_handle_clone2 = app_handle_clone.clone();
                let model_name_clone2 = model_name_clone.clone();
                let state_clone2 = state_clone.clone();

                tokio::spawn(async move {
                    set_model_status(
                        &state_clone2,
                        &app_handle_clone2,
                        status::ModelStatus::Downloading {
                            model_name: model_name_clone2,
                            progress: progress.percentage as u8,
                            downloaded_bytes: progress.downloaded_bytes,
                            total_bytes: progress.total_bytes,
                            bytes_per_sec,
                        },
                    )
                    .await;
                });
            }
        };

        // Download the model
        let download_result = model_mgr
            .download_model(&model_url, &model_name, progress_callback)
            .await;

        match download_result {
            Ok(path) => {
                // Download complete, now show loading status
                set_model_status(
                    &state,
                    &app_handle,
                    status::ModelStatus::Loading {
                        model_name: model_name.clone(),
                        started_at_ms: status::unix_ms_now(),
                    },
                )
                .await;
                path
            }
            Err(e) => {
                set_model_status(
                    &state,
                    &app_handle,
                    status::ModelStatus::Error { message: e.clone() },
                )
                .await;
                return Err(e);
            }
        }
    };

    // Load the model
    let started = std::time::Instant::now();
    let result = model_mgr.load_model(model_path).await;
    let load_ms = Some(started.elapsed().as_millis() as u32);

    match result {
        Ok(_) => {
            set_model_status(
                &state,
                &app_handle,
                status::ModelStatus::Ready {
                    model_name,
                    load_ms,
                },
            )
            .await;
            Ok("Model loaded successfully".to_string())
        }
        Err(e) => {
            set_model_status(
                &state,
                &app_handle,
                status::ModelStatus::Error { message: e.clone() },
            )
            .await;
            Err(e)
        }
    }
}

/// Exchange the stored refresh token for a new access token, updating both the
/// in-memory state and the on-disk store.
///
/// Takes `&AppState` rather than `tauri::State` so the gRPC connection layer
/// can call it: a node whose stream drops after its access token has expired
/// needs a new one before it can reconnect, and nothing else refreshes while
/// the app is running - `check_stored_auth_internal` only runs at startup.
///
/// Returns the new access token, or None when the refresh token is itself gone
/// or rejected, which is the one case that genuinely needs the user.
pub(crate) async fn refresh_access_token_into_state(
    state: &AppState,
    app_handle: &tauri::AppHandle,
) -> Option<String> {
    let refresh_token = state.refresh_token.lock().await.clone()?;

    match auth::refresh_access_token(refresh_token).await {
        Ok(auth_result) => {
            *state.access_token.lock().await = Some(auth_result.access_token.clone());
            *state.refresh_token.lock().await = Some(auth_result.refresh_token.clone());
            if let Err(e) = save_tokens_to_store(
                app_handle,
                &auth_result.access_token,
                &auth_result.refresh_token,
            )
            .await
            {
                eprintln!("Failed to save refreshed tokens: {}", e);
            }
            println!("Access token refreshed");
            Some(auth_result.access_token)
        }
        Err(e) => {
            // Deliberately does NOT clear the stored tokens: a refresh can
            // fail because Supabase is unreachable, and throwing the refresh
            // token away would turn a blip into a forced sign-in.
            println!("Access token refresh failed: {}", e);
            None
        }
    }
}

/// Auto-login: Check if there are stored tokens and validate them
/// Returns the access token if valid, None otherwise
#[tauri::command]
async fn auto_login(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let is_authenticated = check_stored_auth_internal(state.clone(), app_handle).await?;

    if is_authenticated {
        let token = state.access_token.lock().await;
        Ok(token.clone())
    } else {
        Ok(None)
    }
}

/// Check if there are stored tokens and validate them
#[tauri::command]
async fn check_stored_auth(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<bool, String> {
    check_stored_auth_internal(state, app_handle).await
}

/// Internal helper for checking stored authentication
async fn check_stored_auth_internal(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<bool, String> {
    // Try to load tokens from store
    let (access_token, refresh_token) = match load_tokens_from_store(&app_handle).await {
        Ok(tokens) => tokens,
        Err(_) => return Ok(false),
    };

    // First, try to validate the access token
    let validation_result = auth::validate_access_token(access_token.clone()).await;

    if let Ok(_user) = validation_result {
        // Token is valid, update in-memory state
        let mut token = state.access_token.lock().await;
        *token = Some(access_token.clone());
        drop(token);

        let mut refresh = state.refresh_token.lock().await;
        *refresh = Some(refresh_token);
        drop(refresh);

        println!("Access token validated successfully");
        return Ok(true);
    } else if let Err(e) = validation_result {
        let error_msg = e.to_string();
        println!(
            "Access token validation failed: {}, attempting refresh...",
            error_msg
        );
    }

    // Access token is invalid, try to refresh it
    let refresh_result = auth::refresh_access_token(refresh_token.clone()).await;

    match refresh_result {
        Ok(auth_result) => {
            // Update both in-memory and persistent storage
            let mut token = state.access_token.lock().await;
            *token = Some(auth_result.access_token.clone());
            drop(token);

            let mut refresh = state.refresh_token.lock().await;
            *refresh = Some(auth_result.refresh_token.clone());
            drop(refresh);

            if let Err(e) = save_tokens_to_store(
                &app_handle,
                &auth_result.access_token,
                &auth_result.refresh_token,
            )
            .await
            {
                eprintln!("Failed to save refreshed tokens: {}", e);
            }

            println!("Token refreshed successfully");
            Ok(true)
        }
        Err(e) => {
            let error_msg = e.to_string();
            drop(e); // Explicitly drop the error before await
            println!(
                "Token refresh failed: {}, clearing stored tokens",
                error_msg
            );
            // Clear invalid tokens
            let _ = clear_tokens_from_store(&app_handle).await;
            Ok(false)
        }
    }
}

// Helper functions for token storage
fn get_token_file_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    // Create directory if it doesn't exist
    fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;

    Ok(app_data_dir.join(".auth_tokens.json"))
}

async fn save_tokens_to_store(
    app_handle: &tauri::AppHandle,
    access_token: &str,
    refresh_token: &str,
) -> Result<(), String> {
    let file_path = get_token_file_path(app_handle)?;

    let tokens = StoredTokens {
        access_token: access_token.to_string(),
        refresh_token: refresh_token.to_string(),
    };

    let json =
        serde_json::to_string(&tokens).map_err(|e| format!("Failed to serialize tokens: {}", e))?;

    // Owner-only: this file holds the refresh token.
    ai4all_tauri::private_file::write_private(&file_path, json.as_bytes())
        .map_err(|e| format!("Failed to write tokens: {}", e))?;

    Ok(())
}

async fn load_tokens_from_store(app_handle: &tauri::AppHandle) -> Result<(String, String), String> {
    let file_path = get_token_file_path(app_handle)?;

    if !file_path.exists() {
        return Err("No stored tokens found".to_string());
    }

    let json =
        fs::read_to_string(file_path).map_err(|e| format!("Failed to read tokens: {}", e))?;

    let tokens: StoredTokens =
        serde_json::from_str(&json).map_err(|e| format!("Failed to parse tokens: {}", e))?;

    Ok((tokens.access_token, tokens.refresh_token))
}

async fn clear_tokens_from_store(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let file_path = get_token_file_path(app_handle)?;

    if file_path.exists() {
        fs::remove_file(file_path).map_err(|e| format!("Failed to remove tokens: {}", e))?;
    }

    Ok(())
}

// Helper functions for device ID storage
fn get_device_id_file_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    // Create directory if it doesn't exist
    fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;

    Ok(app_data_dir.join(".device_id"))
}

fn save_device_id_to_store(app_handle: &tauri::AppHandle, device_id: &str) -> Result<(), String> {
    let file_path = get_device_id_file_path(app_handle)?;

    fs::write(file_path, device_id).map_err(|e| format!("Failed to write device ID: {}", e))?;

    println!("Device ID saved to persistent storage: {}", device_id);
    Ok(())
}

fn load_device_id_from_store(app_handle: &tauri::AppHandle) -> Result<String, String> {
    let file_path = get_device_id_file_path(app_handle)?;

    if !file_path.exists() {
        return Err("No stored device ID found".to_string());
    }

    let device_id =
        fs::read_to_string(file_path).map_err(|e| format!("Failed to read device ID: {}", e))?;

    println!("Device ID loaded from persistent storage: {}", device_id);
    Ok(device_id.trim().to_string())
}

fn get_or_create_device_id(app_handle: &tauri::AppHandle) -> String {
    // Try to load existing device ID
    match load_device_id_from_store(app_handle) {
        Ok(device_id) => {
            println!("Using existing device ID");
            device_id
        }
        Err(_) => {
            // Generate new device ID
            let device_id = uuid::Uuid::new_v4().to_string();
            println!("Generated new device ID: {}", device_id);

            // Save for future use
            if let Err(e) = save_device_id_to_store(app_handle, &device_id) {
                eprintln!("Warning: Failed to save device ID: {}", e);
            }

            device_id
        }
    }
}

fn main() {
    tauri::Builder::default()
        // Must be the first plugin. On Windows and Linux a nodea:// link
        // launches a second process; this hands it to the running one, and
        // its deep-link feature turns it into an on_open_url event below.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        // The scheme is declared in tauri.conf.json (plugins.deep-link), which
        // is also what puts it in the macOS bundle's Info.plist.
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            use tauri_plugin_deep_link::DeepLinkExt;

            // The installers register the scheme on Windows. An AppImage has
            // no installer and may move, so Linux registers it on every run;
            // so does a Windows dev build, which is never installed.
            #[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
            if let Err(e) = app.deep_link().register_all() {
                eprintln!("Failed to register nodea:// deep link scheme: {}", e);
            }

            // Forward incoming nodea:// links (invitation/confirm-email
            // emails) to the frontend, which navigates the router to the
            // link's path - see src/components/DeepLinkListener.tsx.
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    let request = url.to_string();
                    // A Google sign-in comes back on this same scheme and is
                    // consumed by the Rust side, which is what is waiting for
                    // it. Swallowed even when nothing is waiting: a stale
                    // callback is still not a route, and forwarding it would
                    // navigate the app to /callback and land the user on
                    // "not found".
                    if auth::is_oauth_callback(&request) {
                        auth::deliver_oauth_callback(&request);
                        continue;
                    }

                    if let Err(e) = handle.emit("scheme-request-received", request) {
                        eprintln!("Failed to emit deep link event: {}", e);
                    }
                }
            });

            // Get or create persistent device ID
            let device_id = get_or_create_device_id(app.handle());

            // Get app data directory for model cache
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            let model_cache_dir = app_data_dir.join("models");
            println!("Model cache directory: {:?}", model_cache_dir);

            // Store in app state
            app.manage(AppState {
                access_token: Arc::new(Mutex::new(None)),
                refresh_token: Arc::new(Mutex::new(None)),
                device_id: Arc::new(device_id),
                message_log: Arc::new(Mutex::new(Vec::new())),
                model_manager: model_manager::create_model_manager(model_cache_dir.clone()),
                connection_status: Arc::new(Mutex::new(status::ConnectionStatus::Disconnected)),
                model_status: Arc::new(Mutex::new(status::ModelStatus::NoModel)),
                active_connection_cancel: Arc::new(Mutex::new(None)),
                generations: Arc::new(ai4all_tauri::generations::Generations::new()),
                model_cache_dir: Arc::new(model_cache_dir),
                model_switch: Arc::new(Mutex::new(())),
                usage_sampler: Arc::new(std::sync::Mutex::new(None)),
            });

            build_tray(app)?;

            // Every state change goes to the frontend and the tray item.
            let handle = app.handle().clone();
            let updater = Arc::new(app_updater::AppUpdater::new(
                app_updater::PluginSource::new(app.handle().clone()),
                move |state| {
                    let _ = handle.emit("update://state", state);
                    if let Some(items) = handle.try_state::<TrayItems>() {
                        let (label, enabled) = state.tray_label();
                        let _ = items.update.set_text(label);
                        let _ = items.update.set_enabled(enabled);
                    }
                },
            ));
            app.manage(updater.clone());
            // Release builds only. A dev build reports 0.1.0, so it would
            // download the latest release and install it over itself; a
            // developer can still try the flow with "Check for updates".
            #[cfg(not(debug_assertions))]
            tauri::async_runtime::spawn(ai4all_tauri::updater::run_schedule(
                updater,
                Default::default(),
            ));
            #[cfg(debug_assertions)]
            drop(updater);

            // Keep running behind the tray on close/quit instead of exiting -
            // only the tray's own "Quit" item (see handle_menu_event) performs
            // a real app_handle.exit(0).
            if let Some(window) = app.get_webview_window("main") {
                window.clone().on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                });
            }

            Ok(())
        })
        .on_menu_event(handle_menu_event)
        .invoke_handler(tauri::generate_handler![
            login_email_password,
            login_oauth,
            sign_up,
            logout,
            is_authenticated,
            get_device_id,
            nodea_usage,
            auto_login,
            check_stored_auth,
            connect_grpc,
            disconnect_grpc,
            get_message_log,
            log_rest_api_call,
            log_frontend_event,
            load_model,
            generate_text,
            is_model_loaded,
            get_inference_backend,
            get_model_cache_info,
            get_system_memory,
            reveal_model_cache,
            load_model_auto,
            get_all_status,
            app_updater::get_update_state,
            app_updater::check_for_updates,
            app_updater::restart_to_update
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Every way out ends here: the tray's Quit, Cmd+Q, the OS
            // logging out. Closing the window only hides it, so it does not.
            if let tauri::RunEvent::Exit = event {
                // Before exit() runs llama.cpp's static destructors, or
                // quitting with a model loaded aborts on macOS.
                if let Some(state) = app_handle.try_state::<AppState>() {
                    let released = tauri::async_runtime::block_on(model_manager::release_for_exit(
                        &state.model_manager,
                        &state.generations,
                        std::time::Duration::from_secs(5),
                    ));
                    if !released {
                        eprintln!("Quitting with the model still loaded: its slot stayed busy");
                    }
                }
                if let Some(updater) = app_handle.try_state::<Arc<app_updater::AppUpdater>>() {
                    updater.install_on_exit();
                }
            }
        });
}
