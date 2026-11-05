//! Proto contract: load, unload, cancel and context limits
//! (notes/llama-cpp-4-migration.md §4.3, §5.5, §8.5).

mod support;

use nodea_inference::{GenerationOutcome, LoadOptions, Model};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::{assert_invariants, model, model_path, req, rt, run, run_with, FIXTURE_LOAD};
use tokio_util::sync::CancellationToken;

#[test]
fn cancel_stops_within_one_step() {
    let Some(model) = model("smoke") else { return };
    let request = req(
        r#"{"messages":[{"role":"user","content":"Write a very long story."}],
        "max_tokens":500,"temperature":0}"#,
    );
    let cancel = CancellationToken::new();
    let started = Instant::now();
    // When the first token arrived (= how long one step took, at least) and
    // when we cancelled.
    let first_token: Arc<Mutex<Option<(Duration, Instant)>>> = Arc::default();

    let run = run_with(&model, &request, &cancel, {
        let cancel = cancel.clone();
        let first_token = first_token.clone();
        move |_| {
            first_token
                .lock()
                .unwrap()
                .get_or_insert_with(|| (started.elapsed(), Instant::now()));
            cancel.cancel();
        }
    });

    assert_eq!(
        run.outcome,
        Ok(GenerationOutcome::Cancelled),
        "{:#?}",
        run.messages
    );
    assert_invariants(&request, &run);
    // "Within one step", measured against this run rather than a fixed
    // number: a slow CI CPU takes longer per token, and a cancel may take as
    // long as producing the first token did, but no longer.
    let (time_to_first_token, cancelled_at) = first_token.lock().unwrap().unwrap();
    let since_cancel = cancelled_at.elapsed();
    let bound = time_to_first_token.max(Duration::from_millis(500));
    assert!(
        since_cancel <= bound,
        "took {since_cancel:?} to stop after the cancel; one step took {time_to_first_token:?}"
    );
    assert_eq!(run.messages.len(), 1, "nothing is sent after the cancel");
}

#[test]
fn prompt_longer_than_context_fails_cleanly() {
    let Some(path) = model_path("smoke") else {
        return;
    };
    let model = rt()
        .block_on(Model::load(
            &path,
            LoadOptions {
                n_ctx: 512,
                ..FIXTURE_LOAD
            },
        ))
        .unwrap();
    let long = "word ".repeat(2000);
    let request = req(&format!(
        r#"{{"messages":[{{"role":"user","content":"{long}"}}],"max_tokens":8}}"#
    ));

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    let error = run.error().expect("an error, not a generation");
    // Both numbers, so the caller can tell how far over it was.
    assert!(error.contains("512"), "{error}");
    assert!(
        error.chars().filter(char::is_ascii_digit).count() > 3,
        "{error}"
    );
}

#[test]
fn unload_then_load_again() {
    let Some(path) = model_path("smoke") else {
        return;
    };
    let request =
        req(r#"{"messages":[{"role":"user","content":"Hi"}],"max_tokens":4,"temperature":0}"#);

    let first = rt().block_on(Model::load(&path, FIXTURE_LOAD)).unwrap();
    let run_one = run(&first, &request);
    assert_invariants(&request, &run_one);
    drop(first);

    let second = rt().block_on(Model::load(&path, FIXTURE_LOAD)).unwrap();
    let run_two = run(&second, &request);
    assert_invariants(&request, &run_two);
}

#[test]
fn missing_and_corrupt_files_fail_to_load() {
    let dir = tempfile::tempdir().unwrap();

    let missing = rt().block_on(Model::load(&dir.path().join("missing.gguf"), FIXTURE_LOAD));
    assert!(missing.is_err());

    let corrupt = dir.path().join("corrupt.gguf");
    std::fs::write(&corrupt, vec![0x5a; 1024]).unwrap();
    let loaded = rt().block_on(Model::load(&corrupt, FIXTURE_LOAD));
    assert!(loaded.is_err());
}
