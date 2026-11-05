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
    let cancelled_at: Arc<Mutex<Option<Instant>>> = Arc::default();

    let run = run_with(&model, &request, &cancel, {
        let cancel = cancel.clone();
        let cancelled_at = cancelled_at.clone();
        move |_| {
            cancelled_at
                .lock()
                .unwrap()
                .get_or_insert_with(Instant::now);
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
    let since_cancel = cancelled_at.lock().unwrap().unwrap().elapsed();
    assert!(
        since_cancel < Duration::from_millis(500),
        "took {since_cancel:?} to stop after the cancel"
    );
}

#[test]
#[cfg_attr(
    feature = "engine-mistralrs",
    ignore = "mistralrs sizes its own context and ignores LoadOptions::n_ctx"
)]
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
