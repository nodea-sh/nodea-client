//! Fixtures for the proto contract suite (notes/llama-cpp-4-migration.md §8.3).
//!
//! Every contract test goes through the real path the node uses:
//! `ChatCompletionRequest` -> `request::{chat_turns, generation_options}` ->
//! `Model::generate_streaming` -> `wire::generated_text`, and then checks the
//! `GeneratedText` messages the backend would receive against §8.6.
//!
//! A test whose model has not been fetched prints `SKIP` and passes, so plain
//! `cargo test` works anywhere. CI sets `NODEA_REQUIRE_TEST_MODELS=1`, which
//! turns a missing model into a failure.
#![allow(dead_code)]

use nodea_inference::proto::{ChatCompletionRequest, GeneratedText};
use nodea_inference::{
    request, test_models, wire, GenerationOutcome, LoadOptions, Model, RenderedPrompt,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// One runtime for the whole test binary. A model outlives any single test,
/// so it must not be tied to a per-test runtime that shuts down after it.
pub fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    })
}

/// The options every fixture loads with: CPU only, and a context small enough
/// to be quick but big enough for the tool prompts.
pub const FIXTURE_LOAD: LoadOptions = LoadOptions {
    n_ctx: 4096,
    n_gpu_layers: 0,
};

fn require_models() -> bool {
    std::env::var("NODEA_REQUIRE_TEST_MODELS").is_ok_and(|v| v == "1")
}

fn test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("<unnamed test>")
        .to_string()
}

/// The pinned file for `name`, or `None` (after printing SKIP) when it has not
/// been fetched and CI is not requiring it.
pub fn model_path(name: &str) -> Option<std::path::PathBuf> {
    let model = test_models::get(name).unwrap_or_else(|| panic!("unknown test model {name:?}"));
    let path = model.path();
    if path.exists() {
        return Some(path);
    }
    if require_models() {
        panic!(
            "test model {name} is missing at {}. Run:\n  cargo run -p nodea-inference --bin fetch-test-models -- --only {name}",
            path.display()
        );
    }
    eprintln!("SKIP {}: model {name} not fetched", test_name());
    None
}

/// The shared fixture for `name`, loaded once per test binary.
pub fn model(name: &str) -> Option<Arc<Model>> {
    static MODELS: OnceLock<Mutex<HashMap<String, Arc<Model>>>> = OnceLock::new();

    let path = model_path(name)?;
    let mut models = MODELS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(model) = models.get(name) {
        return Some(model.clone());
    }
    let loaded = Arc::new(
        rt().block_on(Model::load(&path, FIXTURE_LOAD))
            .unwrap_or_else(|e| panic!("could not load test model {name}: {e}")),
    );
    models.insert(name.to_string(), loaded.clone());
    Some(loaded)
}

/// Generations run one at a time, so CPU contention between parallel tests
/// cannot make the timing-sensitive ones flaky.
fn generation_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// A request from JSON with proto field names (see `request::from_json`).
pub fn req(json: &str) -> ChatCompletionRequest {
    request::from_json(json).unwrap_or_else(|e| panic!("{e}\n{json}"))
}

/// What one request put on the wire.
#[derive(Debug)]
pub struct Run {
    /// Every `GeneratedText` the backend would have received, in order. A
    /// failure is the terminal error message grpc_client sends.
    pub messages: Vec<GeneratedText>,
    pub outcome: Result<GenerationOutcome, String>,
    pub elapsed: Duration,
}

impl Run {
    /// The concatenated text of one choice.
    pub fn text(&self, choice: u32) -> String {
        self.messages
            .iter()
            .filter(|m| m.choice_index == Some(choice))
            .map(|m| m.text.as_str())
            .collect()
    }

    /// The message carrying this choice's finish_reason.
    pub fn terminal(&self, choice: u32) -> &GeneratedText {
        self.messages
            .iter()
            .find(|m| m.choice_index == Some(choice) && m.finish_reason.is_some())
            .unwrap_or_else(|| panic!("choice {choice} never finished: {:#?}", self.messages))
    }

    pub fn finish_reason(&self, choice: u32) -> &str {
        self.terminal(choice).finish_reason.as_deref().unwrap()
    }

    /// The tool calls on this choice's terminal message, parsed.
    pub fn tool_calls(&self, choice: u32) -> Vec<serde_json::Value> {
        match &self.terminal(choice).tool_calls {
            Some(raw) => serde_json::from_str(raw).expect("tool_calls is not JSON"),
            None => Vec::new(),
        }
    }

    /// The error grpc_client would have sent, if the generation failed.
    pub fn error(&self) -> Option<&str> {
        self.messages.iter().find_map(|m| m.error.as_deref())
    }

    pub fn usage(&self) -> nodea_inference::proto::Usage {
        self.messages
            .iter()
            .find_map(|m| m.usage.clone())
            .unwrap_or_else(|| panic!("no usage: {:#?}", self.messages))
    }
}

pub fn run(model: &Model, req: &ChatCompletionRequest) -> Run {
    run_with(model, req, &CancellationToken::new(), |_| {})
}

/// `run`, with a hook called on every message as it arrives (before it is
/// recorded), which is how the cancellation test cancels mid-stream.
pub fn run_with(
    model: &Model,
    req: &ChatCompletionRequest,
    cancel: &CancellationToken,
    mut on_message: impl FnMut(&GeneratedText) + Send,
) -> Run {
    let _serialised = generation_lock();
    let turns = request::chat_turns(req);
    let options = request::generation_options(req);
    let mut messages = Vec::new();

    let start = Instant::now();
    let outcome = rt().block_on(
        model.generate_streaming(&turns, &options, cancel, |update| {
            let message = wire::generated_text("test", &update);
            on_message(&message);
            messages.push(message);
            Ok(())
        }),
    );
    let elapsed = start.elapsed();

    if let Err(error) = &outcome {
        messages.push(wire::generation_error("test", error));
    }
    Run {
        messages,
        outcome,
        elapsed,
    }
}

/// The prompt a request renders to.
pub fn render(model: &Model, req: &ChatCompletionRequest) -> RenderedPrompt {
    model
        .render_prompt(&request::chat_turns(req), &request::generation_options(req))
        .expect("render_prompt failed")
}

/// Template and control markers that must never reach a client as text.
pub const MARKERS: &[&str] = &[
    "<|im_start|>",
    "<|im_end|>",
    "<tool_call>",
    "</tool_call>",
    "<think>",
    "</think>",
    "[TOOL_CALLS]",
    "<|channel|>",
    "<|endoftext|>",
    "<end_of_turn>",
    "<start_of_turn>",
    "<|eot_id|>",
];

const FINISH_REASONS: &[&str] = &["stop", "length", "tool_calls"];

/// The §8.6 invariants. Every contract test calls this.
pub fn assert_invariants(req: &ChatCompletionRequest, run: &Run) {
    let messages = &run.messages;
    let dump = || format!("{messages:#?}");

    match &run.outcome {
        // 9. An error run is exactly one message: last, empty text, error set.
        Err(_) => {
            assert_eq!(messages.len(), 1, "an error run is one message: {}", dump());
            let only = &messages[0];
            assert!(
                only.last && only.text.is_empty() && only.error.is_some(),
                "{}",
                dump()
            );
            return;
        }
        // A cancelled run sends nothing terminal: the backend already gave up.
        Ok(GenerationOutcome::Cancelled) => {
            assert!(
                messages
                    .iter()
                    .all(|m| !m.last && m.finish_reason.is_none()),
                "a cancelled run must not send a terminal message: {}",
                dump()
            );
            return;
        }
        Ok(GenerationOutcome::Completed) => {}
    }

    let n = req.n.unwrap_or(1).max(1);

    // 1. Exactly one `last`, and it is the final message.
    assert_eq!(
        messages.iter().filter(|m| m.last).count(),
        1,
        "exactly one message has last = true: {}",
        dump()
    );
    assert!(
        messages.last().unwrap().last,
        "last must be the final message: {}",
        dump()
    );

    for message in messages {
        // choice_index is always set, in 0..n.
        let choice = message.choice_index.expect("choice_index is always set");
        assert!(
            choice < n,
            "choice_index {choice} outside 0..{n}: {}",
            dump()
        );
        assert!(message.error.is_none(), "{}", dump());

        // 3. usage only on the last message.
        if message.usage.is_some() {
            assert!(message.last, "usage only on last = true: {}", dump());
        }

        // 4. finish_reason vocabulary, and tool_calls iff "tool_calls".
        if let Some(reason) = &message.finish_reason {
            assert!(
                FINISH_REASONS.contains(&reason.as_str()),
                "finish_reason {reason:?}"
            );
            assert_eq!(
                reason == "tool_calls",
                message.tool_calls.is_some(),
                "finish_reason tool_calls iff tool_calls is set: {}",
                dump()
            );
        } else {
            assert!(
                message.tool_calls.is_none(),
                "tool_calls only on a terminal: {}",
                dump()
            );
        }

        // 5. tool_calls shape.
        if let Some(raw) = &message.tool_calls {
            assert_tool_calls_shape(raw);
        }

        // 6. logprobs only when asked for, then on every chunk with text.
        if req.logprobs == Some(true) {
            if !message.text.is_empty() {
                let raw = message
                    .logprobs
                    .as_deref()
                    .unwrap_or_else(|| panic!("a text chunk without logprobs: {}", dump()));
                serde_json::from_str::<serde_json::Value>(raw).expect("logprobs is not JSON");
            }
        } else {
            assert!(
                message.logprobs.is_none(),
                "logprobs nobody asked for: {}",
                dump()
            );
        }

        // 7. No template or control markers.
        for marker in MARKERS {
            assert!(
                !message.text.contains(marker),
                "{marker} reached the client: {}",
                dump()
            );
        }
    }

    // 2. Each choice finishes exactly once, and nothing follows its finish.
    for choice in 0..n {
        let of_choice: Vec<_> = messages
            .iter()
            .filter(|m| m.choice_index == Some(choice))
            .collect();
        let finishes: Vec<_> = of_choice
            .iter()
            .enumerate()
            .filter(|(_, m)| m.finish_reason.is_some())
            .collect();
        assert_eq!(
            finishes.len(),
            1,
            "choice {choice} finishes exactly once: {}",
            dump()
        );
        assert_eq!(
            finishes[0].0,
            of_choice.len() - 1,
            "choice {choice} sent something after it finished: {}",
            dump()
        );

        // 8. No stop string reaches the client.
        let text = run.text(choice);
        for stop in &req.stop {
            assert!(
                !text.contains(stop.as_str()),
                "stop {stop:?} was emitted: {text:?}"
            );
        }
    }

    // 3. usage on the final message, counted honestly.
    let usage = messages
        .last()
        .unwrap()
        .usage
        .clone()
        .unwrap_or_else(|| panic!("the final message carries usage: {}", dump()));
    assert!(usage.prompt_tokens > 0, "{}", dump());
    let text_chunks = messages.iter().filter(|m| !m.text.is_empty()).count() as u32;
    assert!(
        usage.completion_tokens >= text_chunks,
        "completion_tokens {} < {text_chunks} text chunks",
        usage.completion_tokens
    );
}

fn assert_tool_calls_shape(raw: &str) {
    let calls: Vec<serde_json::Value> =
        serde_json::from_str(raw).unwrap_or_else(|e| panic!("tool_calls {raw:?}: {e}"));
    assert!(!calls.is_empty(), "an empty tool_calls array: {raw}");
    for call in calls {
        assert!(
            call["id"].as_str().is_some_and(|id| !id.is_empty()),
            "tool call without an id: {call}"
        );
        assert_eq!(call["type"], "function", "{call}");
        assert!(call["function"]["name"].as_str().is_some(), "{call}");
        let arguments = call["function"]["arguments"]
            .as_str()
            .unwrap_or_else(|| panic!("arguments must be a JSON string: {call}"));
        serde_json::from_str::<serde_json::Value>(arguments)
            .unwrap_or_else(|e| panic!("arguments {arguments:?} is not JSON: {e}"));
    }
}

/// The capability -> tests table behind `capabilities_match_the_suite`. A
/// capability with no row here fails that test.
pub const CAPABILITY_TESTS: &[(&str, &[&str])] = &[
    (
        "structured_messages",
        &[
            "contract_chat::whole_conversation_reaches_the_template",
            "contract_chat::default_system_prompt_only_when_none_sent",
            "contract_tools::tool_reply_is_replayed_and_answered",
        ],
    ),
    (
        "terminal_metadata",
        &[
            "contract_chat::plain_chat_streams_and_stops",
            "contract_chat::max_tokens_yields_length",
            "contract_chat::stop_sequence_is_honoured_and_not_emitted",
        ],
    ),
    (
        "response_format",
        &[
            "contract_tools::json_object_produces_an_object",
            "contract_tools::json_schema_is_enforced",
        ],
    ),
    (
        "tools",
        &[
            "contract_tools::tool_call_is_parsed_into_openai_shape",
            "contract_tools::named_tool_choice_forces_that_tool",
            "contract_tools::tool_choice_none_answers_in_prose",
            "contract_tools::unparseable_tools_fail_loudly",
        ],
    ),
    (
        "logprobs",
        &["contract_logprobs::logprobs_have_openai_shape"],
    ),
    (
        "n_choices",
        &["contract_chat::n_choices_are_indexed_and_terminated"],
    ),
    (
        "logit_bias",
        &[
            "contract_chat::logit_bias_bans_a_token",
            "contract_chat::out_of_vocab_logit_bias_is_an_error",
        ],
    ),
    (
        "reasoning_effort",
        &[
            "contract_tools::reasoning_effort_reaches_the_template",
            "contract_chat::unsupported_reasoning_effort_is_an_error",
        ],
    ),
    (
        "cancel",
        &["contract_lifecycle::cancel_stops_within_one_step"],
    ),
    // App-level: the reload path is driven with a fake model slot.
    ("scheduler_v1", &["src-tauri/src/reload.rs tests"]),
    // The backend routes architectures beyond the previous engine's list only
    // to nodes that report this (notes/llama-cpp-4-migration.md §9).
    (
        "llama_cpp_engine",
        &[
            "contract_chat::new_arch_lfm2_chats",
            "contract_chat::new_arch_gemma3_chats",
        ],
    ),
];
