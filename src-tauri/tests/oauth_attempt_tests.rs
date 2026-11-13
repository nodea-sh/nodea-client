//! The lifecycle of a desktop OAuth sign-in: started, then finished by the
//! browser's callback, cancelled from the sign-in screen, or timed out.
//!
//! Only `begin_oauth` is used to start an attempt, never `start_oauth`, so no
//! browser is launched.

use ai4all_tauri::auth::{
    await_oauth_callback, begin_oauth, cancel_oauth, deliver_oauth_callback, OAUTH_CANCELLED,
    OAUTH_TIMED_OUT,
};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

const CALLBACK: &str = "nodea://auth/callback?code=abc";
const LONG_WAIT: Duration = Duration::from_secs(30);

/// The attempt in progress is process-wide state and these tests run on
/// parallel threads, so they take turns.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cancel_oauth();
    guard
}

#[test]
fn the_sign_in_url_asks_for_the_provider_with_a_pkce_challenge() {
    let _guard = serial();

    let url = begin_oauth("google");

    assert!(url.contains("/auth/v1/authorize?provider=google"));
    assert!(url.contains("code_challenge="));
    assert!(url.contains("code_challenge_method=s256"));
    assert!(url.contains("desktop-callback"));
}

// The guard only serialises these tests; each runtime is single-threaded.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn a_delivered_callback_finishes_the_waiting_sign_in() {
    let _guard = serial();
    begin_oauth("google");

    let (result, delivered) = tokio::join!(await_oauth_callback(LONG_WAIT), async {
        deliver_oauth_callback(CALLBACK)
    });

    assert!(delivered);
    let (url, verifier) = result.expect("the callback should finish the sign-in");
    assert_eq!(url, CALLBACK);
    assert!(verifier.len() >= 43);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn cancelling_ends_the_waiting_sign_in_at_once() {
    let _guard = serial();
    begin_oauth("google");

    // Waiting first, then cancelled from the sign-in screen: the wait must end
    // now, not when the five-minute timeout fires.
    let started = std::time::Instant::now();
    let (result, ()) = tokio::join!(await_oauth_callback(LONG_WAIT), async { cancel_oauth() });

    assert_eq!(result, Err(OAUTH_CANCELLED.to_string()));
    assert!(started.elapsed() < Duration::from_secs(5));
    // A browser that finishes after all is no longer listened to.
    assert!(!deliver_oauth_callback(CALLBACK));
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn cancelling_before_the_wait_leaves_nothing_to_wait_for() {
    let _guard = serial();
    begin_oauth("google");

    cancel_oauth();

    assert!(await_oauth_callback(LONG_WAIT).await.is_err());
    assert!(!deliver_oauth_callback(CALLBACK));
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn a_sign_in_with_no_callback_times_out_and_is_abandoned() {
    let _guard = serial();
    begin_oauth("google");

    let result = await_oauth_callback(Duration::from_millis(20)).await;

    assert_eq!(result, Err(OAUTH_TIMED_OUT.to_string()));
    // Abandoned: a late callback is not mistaken for a live sign-in.
    assert!(!deliver_oauth_callback(CALLBACK));
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn starting_again_replaces_the_attempt_still_waiting() {
    let _guard = serial();
    begin_oauth("google");

    let (first, _) = tokio::join!(await_oauth_callback(LONG_WAIT), async {
        begin_oauth("google")
    });
    assert_eq!(first, Err(OAUTH_CANCELLED.to_string()));

    // The new attempt is the one the browser's callback now finishes.
    let (second, delivered) = tokio::join!(await_oauth_callback(LONG_WAIT), async {
        deliver_oauth_callback(CALLBACK)
    });
    assert!(delivered);
    assert_eq!(second.unwrap().0, CALLBACK);
}
