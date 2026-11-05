// tauri-plugin-updater behind ai4all_tauri::updater::UpdateSource, and the
// commands and tray action that drive it. The policy itself (when to check,
// when to install) is in the library's updater.rs.

use ai4all_tauri::updater::{CheckOutcome, UpdateSource, UpdateState, Updater};
use futures::future::BoxFuture;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// How long Restart lets running generations finish before cancelling them.
const RESTART_DRAIN: Duration = Duration::from_secs(60);

pub type AppUpdater = Updater<PluginSource>;

pub struct PluginSource {
    app: AppHandle,
    /// What the last check found, until it is downloaded.
    found: Mutex<Option<Update>>,
    /// Verified bytes, kept in memory until install.
    downloaded: Mutex<Option<(Update, Vec<u8>)>>,
}

impl PluginSource {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            found: Mutex::new(None),
            downloaded: Mutex::new(None),
        }
    }
}

impl UpdateSource for PluginSource {
    fn check(&self) -> BoxFuture<'_, Result<Option<String>, String>> {
        Box::pin(async move {
            let update = self
                .app
                .updater()
                .map_err(|e| e.to_string())?
                .check()
                .await
                .map_err(|e| e.to_string())?;
            let version = update.as_ref().map(|update| update.version.clone());
            *self.found.lock().unwrap() = update;
            Ok(version)
        })
    }

    fn download<'a>(
        &'a self,
        progress: &'a mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let update = self
                .found
                .lock()
                .unwrap()
                .take()
                .ok_or("No update found to download")?;
            let mut so_far = 0u64;
            // download() verifies the signature before returning the bytes.
            let bytes = update
                .download(
                    |chunk, total| {
                        so_far += chunk as u64;
                        progress(so_far, total);
                    },
                    || {},
                )
                .await
                .map_err(|e| e.to_string())?;
            *self.downloaded.lock().unwrap() = Some((update, bytes));
            Ok(())
        })
    }

    fn install(&self) -> Result<(), String> {
        let downloaded = self.downloaded.lock().unwrap();
        let (update, bytes) = downloaded.as_ref().ok_or("No update downloaded")?;
        update.install(bytes).map_err(|e| e.to_string())
    }
}

fn updater(app: &AppHandle) -> Arc<AppUpdater> {
    app.state::<Arc<AppUpdater>>().inner().clone()
}

/// Wait for running generations, install, relaunch. Returns only on failure.
async fn restart(app: &AppHandle) -> Result<(), String> {
    let generations = app.state::<crate::AppState>().generations.clone();
    updater(app).restart(&generations, RESTART_DRAIN).await?;
    app.restart()
}

#[tauri::command]
pub fn get_update_state(app: AppHandle) -> UpdateState {
    updater(&app).state()
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<CheckOutcome, String> {
    Ok(updater(&app).check().await)
}

#[tauri::command]
pub async fn restart_to_update(app: AppHandle) -> Result<(), String> {
    restart(&app).await
}

/// The tray's update item: restart when an update is ready, otherwise check.
/// The outcome of a check goes to the frontend, which shows it as a toast.
pub async fn tray_clicked(app: AppHandle) {
    if matches!(updater(&app).state(), UpdateState::Ready { .. }) {
        if let Err(error) = restart(&app).await {
            eprintln!("Restart to update failed: {}", error);
        }
    } else {
        let outcome = updater(&app).check().await;
        let _ = app.emit("update://check-result", outcome);
    }
}
