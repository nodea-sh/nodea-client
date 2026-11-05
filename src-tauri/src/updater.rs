// In-app updates: check quietly, download in the background, install when
// the user restarts or quits. Never interrupts a running request.
//
// The policy lives here with no Tauri dependency so it can be tested; main.rs
// implements UpdateSource over tauri-plugin-updater and calls restart() and
// install_on_exit() from the user's actions and the app's run events.

use crate::generations::Generations;
use futures::future::BoxFuture;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Where updates come from. tauri-plugin-updater in the app; a fake in tests.
pub trait UpdateSource: Send + Sync {
    /// Ask the update endpoint for a newer release. Ok(None) when up to date.
    fn check(&self) -> BoxFuture<'_, Result<Option<String>, String>>;

    /// Download and verify the release the last check found, reporting
    /// (bytes so far, total if known). Keeps the bytes for install().
    fn download<'a>(
        &'a self,
        progress: &'a mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> BoxFuture<'a, Result<(), String>>;

    /// Install the downloaded release. Blocking, because it also runs while
    /// the app is exiting. On Windows it ends the process.
    fn install(&self) -> Result<(), String>;
}

/// What the UI shows. Sent to the frontend as `update://state`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateState {
    Idle,
    Checking,
    Downloading {
        version: String,
        /// 0-100, None while the size is unknown.
        progress: Option<u8>,
    },
    /// Downloaded and verified; installs on restart or quit.
    Ready {
        version: String,
    },
    /// The user chose to restart. `finishing_requests` while running
    /// generations are allowed to complete first.
    Installing {
        version: String,
        finishing_requests: bool,
    },
}

impl UpdateState {
    /// The tray menu's update item: its text, and whether it can be clicked.
    /// Clicking it checks, or restarts once an update is ready.
    pub fn tray_label(&self) -> (String, bool) {
        match self {
            UpdateState::Idle => ("Check for Updates…".to_string(), true),
            UpdateState::Checking => ("Checking for Updates…".to_string(), false),
            UpdateState::Downloading { .. } => ("Downloading Update…".to_string(), false),
            UpdateState::Ready { version } => (format!("Restart to Update to {}", version), true),
            UpdateState::Installing { .. } => ("Restarting…".to_string(), false),
        }
    }
}

/// The answer to a check, for a manual "Check for updates" to show.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CheckOutcome {
    UpToDate,
    Ready {
        version: String,
    },
    /// A check is already running, or an install has started.
    Busy,
    Failed {
        error: String,
    },
}

pub struct Updater<S> {
    source: S,
    state: Mutex<UpdateState>,
    report: Box<dyn Fn(&UpdateState) + Send + Sync>,
}

impl<S: UpdateSource> Updater<S> {
    pub fn new(source: S, report: impl Fn(&UpdateState) + Send + Sync + 'static) -> Self {
        Self {
            source,
            state: Mutex::new(UpdateState::Idle),
            report: Box::new(report),
        }
    }

    pub fn state(&self) -> UpdateState {
        self.state.lock().unwrap().clone()
    }

    fn set(&self, next: UpdateState) {
        *self.state.lock().unwrap() = next.clone();
        (self.report)(&next);
    }

    /// Check, and download straight away if there is a newer release. Once an
    /// update is ready, later checks return it without asking again: one
    /// download per launch is enough.
    pub async fn check(&self) -> CheckOutcome {
        {
            let mut state = self.state.lock().unwrap();
            match &*state {
                UpdateState::Idle => *state = UpdateState::Checking,
                UpdateState::Ready { version } => {
                    return CheckOutcome::Ready {
                        version: version.clone(),
                    }
                }
                _ => return CheckOutcome::Busy,
            }
        }
        (self.report)(&UpdateState::Checking);

        let version = match self.source.check().await {
            Ok(Some(version)) => version,
            Ok(None) => {
                self.set(UpdateState::Idle);
                return CheckOutcome::UpToDate;
            }
            Err(error) => return self.fail("check", error),
        };

        self.set(UpdateState::Downloading {
            version: version.clone(),
            progress: None,
        });
        let mut last = None;
        let mut on_progress = |downloaded: u64, total: Option<u64>| {
            let percent = total
                .filter(|total| *total > 0)
                .map(|total| (downloaded.min(total) * 100 / total) as u8);
            // One event per percent, not per chunk.
            if percent != last {
                last = percent;
                self.set(UpdateState::Downloading {
                    version: version.clone(),
                    progress: percent,
                });
            }
        };
        if let Err(error) = self.source.download(&mut on_progress).await {
            return self.fail("download", error);
        }

        self.set(UpdateState::Ready {
            version: version.clone(),
        });
        CheckOutcome::Ready { version }
    }

    /// Errors are only logged: the next scheduled check tries again, and a
    /// manual check shows the returned outcome.
    fn fail(&self, step: &str, error: String) -> CheckOutcome {
        eprintln!("Update {} failed: {}", step, error);
        self.set(UpdateState::Idle);
        CheckOutcome::Failed { error }
    }

    /// The user clicked Restart: let running generations finish (cancelling
    /// any still running after `drain_timeout`), then install. On Ok the
    /// caller relaunches the app.
    pub async fn restart(
        &self,
        generations: &Generations,
        drain_timeout: Duration,
    ) -> Result<(), String> {
        let finishing_requests = generations.in_flight().await > 0;
        let version = match self.state() {
            UpdateState::Ready { version } => version,
            other => return Err(format!("No update ready to install ({:?})", other)),
        };
        self.set(UpdateState::Installing {
            version,
            finishing_requests,
        });

        let drained = generations.drain(drain_timeout).await;
        if drained.cancelled > 0 {
            println!(
                "Update drain timed out: cancelled {} generation(s)",
                drained.cancelled
            );
        }

        if let Err(error) = self.source.install() {
            generations.resume().await;
            self.fail("install", error.clone());
            return Err(error);
        }
        Ok(())
    }

    /// The app is exiting: install a ready update. Quitting already stops any
    /// running request, so this does not wait. A no-op when nothing is ready,
    /// including the exit that follows restart().
    ///
    /// The state changes without a report: this runs after the event loop
    /// has stopped, where updating the tray menu would wait forever for the
    /// main thread.
    pub fn install_on_exit(&self) {
        {
            let mut state = self.state.lock().unwrap();
            let version = match &*state {
                UpdateState::Ready { version } => version.clone(),
                _ => return,
            };
            *state = UpdateState::Installing {
                version,
                finishing_requests: false,
            };
        }
        if let Err(error) = self.source.install() {
            eprintln!("Update install on exit failed: {}", error);
        }
    }
}

/// When to check: shortly after launch (not competing with startup or a
/// model load), then periodically.
#[derive(Clone, Copy, Debug)]
pub struct Schedule {
    pub first: Duration,
    pub every: Duration,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            first: Duration::from_secs(30),
            every: Duration::from_secs(6 * 60 * 60),
        }
    }
}

/// Runs for the life of the app.
pub async fn run_schedule<S: UpdateSource>(updater: Arc<Updater<S>>, schedule: Schedule) {
    tokio::time::sleep(schedule.first).await;
    loop {
        updater.check().await;
        tokio::time::sleep(schedule.every).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Notify;

    #[derive(Default)]
    struct Fake {
        /// Some(version) offers that release; None is up to date.
        offers: Option<String>,
        check_error: Option<String>,
        download_error: Option<String>,
        install_error: Option<String>,
        /// (downloaded, total) steps download() reports.
        progress: Vec<(u64, Option<u64>)>,
        /// When set, check() waits for it before answering.
        gate: Option<Arc<Notify>>,
        checks: AtomicUsize,
        installs: AtomicUsize,
    }

    impl UpdateSource for Fake {
        fn check(&self) -> BoxFuture<'_, Result<Option<String>, String>> {
            Box::pin(async move {
                self.checks.fetch_add(1, Ordering::SeqCst);
                if let Some(gate) = &self.gate {
                    gate.notified().await;
                }
                match &self.check_error {
                    Some(error) => Err(error.clone()),
                    None => Ok(self.offers.clone()),
                }
            })
        }

        fn download<'a>(
            &'a self,
            progress: &'a mut (dyn FnMut(u64, Option<u64>) + Send),
        ) -> BoxFuture<'a, Result<(), String>> {
            Box::pin(async move {
                for (downloaded, total) in &self.progress {
                    progress(*downloaded, *total);
                }
                match &self.download_error {
                    Some(error) => Err(error.clone()),
                    None => Ok(()),
                }
            })
        }

        fn install(&self) -> Result<(), String> {
            self.installs.fetch_add(1, Ordering::SeqCst);
            match &self.install_error {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        }
    }

    fn offering(version: &str) -> Fake {
        Fake {
            offers: Some(version.to_string()),
            ..Fake::default()
        }
    }

    type Events = Arc<Mutex<Vec<UpdateState>>>;

    fn updater(source: Fake) -> (Arc<Updater<Fake>>, Events) {
        let events: Events = Default::default();
        let sink = events.clone();
        let updater = Updater::new(source, move |state| {
            sink.lock().unwrap().push(state.clone())
        });
        (Arc::new(updater), events)
    }

    fn ready(version: &str) -> UpdateState {
        UpdateState::Ready {
            version: version.to_string(),
        }
    }

    fn downloading(progress: Option<u8>) -> UpdateState {
        UpdateState::Downloading {
            version: "26.11.3".to_string(),
            progress,
        }
    }

    #[tokio::test]
    async fn a_new_release_downloads_and_becomes_ready() {
        let (updater, events) = updater(offering("26.11.3"));

        let outcome = updater.check().await;

        assert_eq!(
            outcome,
            CheckOutcome::Ready {
                version: "26.11.3".to_string()
            }
        );
        assert_eq!(
            *events.lock().unwrap(),
            [UpdateState::Checking, downloading(None), ready("26.11.3")]
        );
        assert_eq!(updater.source.installs.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn progress_is_reported_once_per_percent() {
        let (updater, events) = updater(Fake {
            progress: vec![
                (0, None),
                (10, Some(200)),
                (11, Some(200)),
                (200, Some(200)),
            ],
            ..offering("26.11.3")
        });

        updater.check().await;

        assert_eq!(
            *events.lock().unwrap(),
            [
                UpdateState::Checking,
                downloading(None),
                downloading(Some(5)),
                downloading(Some(100)),
                ready("26.11.3"),
            ]
        );
    }

    #[tokio::test]
    async fn up_to_date_goes_back_to_idle() {
        let (updater, events) = updater(Fake::default());

        assert_eq!(updater.check().await, CheckOutcome::UpToDate);
        assert_eq!(
            *events.lock().unwrap(),
            [UpdateState::Checking, UpdateState::Idle]
        );
    }

    #[tokio::test]
    async fn a_failed_check_goes_back_to_idle() {
        let (updater, _) = updater(Fake {
            check_error: Some("offline".to_string()),
            ..Fake::default()
        });

        assert_eq!(
            updater.check().await,
            CheckOutcome::Failed {
                error: "offline".to_string()
            }
        );
        assert_eq!(updater.state(), UpdateState::Idle);
    }

    #[tokio::test]
    async fn a_failed_download_goes_back_to_idle_and_retries_next_time() {
        let (updater, _) = updater(Fake {
            download_error: Some("bad signature".to_string()),
            ..offering("26.11.3")
        });

        assert!(matches!(updater.check().await, CheckOutcome::Failed { .. }));
        assert_eq!(updater.state(), UpdateState::Idle);

        updater.check().await;
        assert_eq!(updater.source.checks.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn once_ready_a_check_does_not_ask_again() {
        let (updater, _) = updater(offering("26.11.3"));
        updater.check().await;

        assert_eq!(
            updater.check().await,
            CheckOutcome::Ready {
                version: "26.11.3".to_string()
            }
        );
        assert_eq!(updater.source.checks.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_second_check_while_one_runs_is_busy() {
        let gate = Arc::new(Notify::new());
        let (updater, _) = updater(Fake {
            gate: Some(gate.clone()),
            ..Fake::default()
        });

        let first = tokio::spawn({
            let updater = updater.clone();
            async move { updater.check().await }
        });
        while updater.state() != UpdateState::Checking {
            tokio::task::yield_now().await;
        }

        assert_eq!(updater.check().await, CheckOutcome::Busy);
        gate.notify_one();
        assert_eq!(first.await.unwrap(), CheckOutcome::UpToDate);
        assert_eq!(updater.source.checks.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn restart_without_a_ready_update_is_refused() {
        let (updater, _) = updater(Fake::default());

        let result = updater
            .restart(&Generations::new(), Duration::from_secs(5))
            .await;

        assert!(result.is_err());
        assert_eq!(updater.source.installs.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn restart_waits_for_a_running_generation_before_installing() {
        let (updater, events) = updater(offering("26.11.3"));
        updater.check().await;
        let generations = Arc::new(Generations::new());
        generations.begin("req-1").await.unwrap();

        let restart = tokio::spawn({
            let (updater, generations) = (updater.clone(), generations.clone());
            async move { updater.restart(&generations, Duration::from_secs(60)).await }
        });
        while !generations.is_draining().await {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(updater.source.installs.load(Ordering::SeqCst), 0);

        generations.finish("req-1").await;
        restart.await.unwrap().unwrap();

        assert_eq!(updater.source.installs.load(Ordering::SeqCst), 1);
        assert!(events.lock().unwrap().contains(&UpdateState::Installing {
            version: "26.11.3".to_string(),
            finishing_requests: true,
        }));
    }

    #[tokio::test]
    async fn a_failed_install_on_restart_accepts_requests_again() {
        let (updater, _) = updater(Fake {
            install_error: Some("read-only".to_string()),
            ..offering("26.11.3")
        });
        updater.check().await;
        let generations = Generations::new();

        let result = updater.restart(&generations, Duration::from_secs(5)).await;

        assert_eq!(result, Err("read-only".to_string()));
        assert!(!generations.is_draining().await);
        assert_eq!(updater.state(), UpdateState::Idle);
    }

    #[tokio::test]
    async fn quitting_installs_a_ready_update_straight_away() {
        let (updater, _) = updater(offering("26.11.3"));
        updater.check().await;

        updater.install_on_exit();

        assert_eq!(updater.source.installs.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn quitting_with_nothing_ready_installs_nothing() {
        let (updater, _) = updater(Fake::default());
        updater.check().await;

        updater.install_on_exit();

        assert_eq!(updater.source.installs.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn the_exit_after_a_restart_does_not_install_twice() {
        let (updater, _) = updater(offering("26.11.3"));
        updater.check().await;
        updater
            .restart(&Generations::new(), Duration::from_secs(5))
            .await
            .unwrap();

        updater.install_on_exit();

        assert_eq!(updater.source.installs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_tray_item_is_clickable_only_when_it_can_act() {
        assert_eq!(
            UpdateState::Idle.tray_label(),
            ("Check for Updates…".to_string(), true)
        );
        assert_eq!(
            ready("26.11.3").tray_label(),
            ("Restart to Update to 26.11.3".to_string(), true)
        );
        for busy in [
            UpdateState::Checking,
            downloading(Some(40)),
            UpdateState::Installing {
                version: "26.11.3".to_string(),
                finishing_requests: true,
            },
        ] {
            assert!(!busy.tray_label().1, "{:?}", busy);
        }
    }

    #[test]
    fn the_state_serialises_as_the_frontend_expects() {
        assert_eq!(
            serde_json::to_value(downloading(Some(40))).unwrap(),
            serde_json::json!({ "state": "downloading", "version": "26.11.3", "progress": 40 })
        );
        assert_eq!(
            serde_json::to_value(CheckOutcome::UpToDate).unwrap(),
            serde_json::json!({ "outcome": "up_to_date" })
        );
    }

    #[tokio::test(start_paused = true)]
    async fn checks_shortly_after_launch_then_periodically() {
        let (updater, _) = updater(Fake::default());
        let checks = || updater.source.checks.load(Ordering::SeqCst);
        let schedule = Schedule::default();
        tokio::spawn(run_schedule(updater.clone(), schedule));

        tokio::time::sleep(schedule.first - Duration::from_secs(1)).await;
        assert_eq!(checks(), 0);

        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(checks(), 1);

        tokio::time::sleep(schedule.every).await;
        assert_eq!(checks(), 2);
    }
}
