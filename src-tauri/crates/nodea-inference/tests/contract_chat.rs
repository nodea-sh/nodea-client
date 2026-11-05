//! Proto contract: plain chat and the sampling fields of
//! `ChatCompletionRequest` (notes/llama-cpp-4-migration.md §5.1, §8.5).

mod support;

use nodea_inference::DEFAULT_SYSTEM_PROMPT;
use support::{assert_invariants, model, render, req, run};

#[test]
fn plain_chat_streams_and_stops() {
    let Some(model) = model("smoke") else { return };
    let request = req(r#"{"messages":[{"role":"user","content":"Say hello."}],
        "max_tokens":32,"temperature":0}"#);

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert!(!run.text(0).trim().is_empty(), "{:#?}", run.messages);
    assert!(["stop", "length"].contains(&run.finish_reason(0)));
    assert!(run.usage().prompt_tokens > 0);
}

#[test]
fn whole_conversation_reaches_the_template() {
    // Regression guard: the node used to take messages.last() only, so a
    // multi-turn request lost its system prompt and all history.
    let Some(model) = model("tools") else { return };
    let request = req(r#"{"messages":[
        {"role":"system","content":"SYSTEM-MARKER be terse."},
        {"role":"user","content":"FIRST-USER-MARKER hi"},
        {"role":"assistant","content":"ASSISTANT-MARKER hello"},
        {"role":"user","content":"SECOND-USER-MARKER again"}]}"#);

    let prompt = render(&model, &request).prompt;

    let positions: Vec<usize> = [
        "SYSTEM-MARKER",
        "FIRST-USER-MARKER",
        "ASSISTANT-MARKER",
        "SECOND-USER-MARKER",
    ]
    .iter()
    .map(|marker| {
        prompt
            .find(marker)
            .unwrap_or_else(|| panic!("{marker} missing from the prompt:\n{prompt}"))
    })
    .collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "out of order:\n{prompt}"
    );

    // And it generates.
    let run = run(&model, &request.clone_with_max_tokens(8));
    assert_invariants(&request, &run);
}

#[test]
fn default_system_prompt_only_when_none_sent() {
    let Some(model) = model("tools") else { return };

    let with_system = render(
        &model,
        &req(r#"{"messages":[{"role":"system","content":"Be terse."},
            {"role":"user","content":"Hi"}]}"#),
    );
    let without = render(
        &model,
        &req(r#"{"messages":[{"role":"user","content":"Hi"}]}"#),
    );
    let developer = render(
        &model,
        &req(r#"{"messages":[{"role":"developer","content":"Be terse."},
            {"role":"user","content":"Hi"}]}"#),
    );

    assert!(
        !with_system.prompt.contains(DEFAULT_SYSTEM_PROMPT),
        "{}",
        with_system.prompt
    );
    assert!(
        without.prompt.contains(DEFAULT_SYSTEM_PROMPT),
        "{}",
        without.prompt
    );
    // OpenAI's rename of system is a system prompt, not a user turn.
    assert!(
        !developer.prompt.contains(DEFAULT_SYSTEM_PROMPT),
        "{}",
        developer.prompt
    );
}

#[test]
fn max_tokens_yields_length() {
    let Some(model) = model("smoke") else { return };
    let request = req(
        r#"{"messages":[{"role":"user","content":"Write a long story about a dragon."}],
        "max_tokens":3,"temperature":0}"#,
    );

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert_eq!(run.finish_reason(0), "length");
    assert_eq!(run.usage().completion_tokens, 3);
}

#[test]
fn stop_sequence_is_honoured_and_not_emitted() {
    let Some(model) = model("smoke") else { return };
    let request = req(r#"{"messages":[{"role":"user",
        "content":"Continue this sequence exactly: 1, 2, 3, 4, 5, 6, 7, 8"}],
        "max_tokens":40,"temperature":0,"stop":[", 4"]}"#);

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert!(!run.text(0).contains(", 4"), "{:?}", run.text(0));
    // Only meaningful if the model actually got that far.
    if run.text(0).contains(", 3") {
        assert_eq!(run.finish_reason(0), "stop", "{:?}", run.text(0));
    }
}

#[test]
fn utf8_is_never_split() {
    let Some(model) = model("smoke") else { return };
    let request = req(r#"{"messages":[{"role":"user",
        "content":"Write three words in French with accents, and three emoji."}],
        "max_tokens":48,"temperature":0}"#);

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    for message in &run.messages {
        // Valid UTF-8 holds by type; a split character shows up as U+FFFD.
        assert!(!message.text.contains('\u{FFFD}'), "{:?}", message.text);
    }
}

#[test]
fn n_choices_are_indexed_and_terminated() {
    let Some(model) = model("smoke") else { return };
    let request = req(r#"{"messages":[{"role":"user","content":"Name a colour."}],
        "max_tokens":12,"temperature":0.8,"seed":7,"n":3}"#);

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    for choice in 0..3 {
        run.terminal(choice);
    }
    let usage = run.usage();
    let generated: u32 = (0..3)
        .map(|choice| {
            run.messages
                .iter()
                .filter(|m| m.choice_index == Some(choice) && !m.text.is_empty())
                .count() as u32
        })
        .sum();
    assert!(usage.completion_tokens >= generated);
    assert!(usage.completion_tokens <= 3 * 12, "{usage:?}");
}

#[test]
fn same_seed_same_output() {
    let Some(model) = model("smoke") else { return };
    let request = req(
        r#"{"messages":[{"role":"user","content":"Tell me a fun fact."}],
        "max_tokens":24,"temperature":0.9,"seed":42}"#,
    );

    let first = run(&model, &request);
    let second = run(&model, &request);

    assert_invariants(&request, &first);
    assert_eq!(first.text(0), second.text(0));
}

#[test]
fn logit_bias_bans_a_token() {
    let Some(model) = model("smoke") else { return };
    let paris = model.tokenize(" Paris").unwrap();
    assert_eq!(paris.len(), 1, "\" Paris\" should be one token: {paris:?}");
    let request = req(&format!(
        r#"{{"messages":[{{"role":"user","content":"What is the capital of France? Answer in one sentence."}}],
        "max_tokens":24,"temperature":0,"logit_bias":{{"{}":-100}}}}"#,
        paris[0]
    ));

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert!(!run.text(0).contains(" Paris"), "{:?}", run.text(0));
}

#[test]
fn out_of_vocab_logit_bias_is_an_error() {
    let Some(model) = model("smoke") else { return };
    let n_vocab = model.info().n_vocab.expect("the engine reports n_vocab");
    let request = req(&format!(
        r#"{{"messages":[{{"role":"user","content":"Hi"}}],"max_tokens":4,
        "logit_bias":{{"{}":5}}}}"#,
        n_vocab + 1
    ));

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert!(run.error().is_some(), "{:#?}", run.messages);
}

#[test]
fn unsupported_reasoning_effort_is_an_error() {
    // The backend rejects these before they reach the node; rounding here
    // would hide a bug and charge the caller reasoning tokens they tried to
    // avoid.
    let Some(model) = model("smoke") else { return };
    let request = req(
        r#"{"messages":[{"role":"user","content":"Hi"}],"max_tokens":4,
        "reasoning_effort":"minimal"}"#,
    );

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert!(
        run.error().unwrap().contains("reasoning_effort"),
        "{:?}",
        run.error()
    );
}

#[test]
fn web_search_options_is_rejected() {
    let Some(model) = model("smoke") else { return };
    let request = req(
        r#"{"messages":[{"role":"user","content":"News?"}],"max_tokens":4,
        "web_search_options":"{}"}"#,
    );

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert!(
        run.error().unwrap().contains("web_search"),
        "{:?}",
        run.error()
    );
}

#[test]
fn new_arch_lfm2_chats() {
    new_architecture_chats("lfm2");
}

#[test]
fn new_arch_gemma3_chats() {
    new_architecture_chats("gemma3");
}

fn new_architecture_chats(name: &str) {
    let Some(model) = model(name) else { return };
    let request = req(
        r#"{"messages":[{"role":"user","content":"What is 2+2? Answer with a number."}],
        "max_tokens":24,"temperature":0}"#,
    );

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert!(run.text(0).contains('4'), "{name}: {:?}", run.text(0));
    assert_eq!(
        model.info().architecture.as_deref(),
        Some(
            nodea_inference::test_models::get(name)
                .unwrap()
                .arch
                .as_str()
        )
    );
}

trait WithMaxTokens {
    fn clone_with_max_tokens(&self, max_tokens: u32) -> Self;
}

impl WithMaxTokens for nodea_inference::proto::ChatCompletionRequest {
    fn clone_with_max_tokens(&self, max_tokens: u32) -> Self {
        Self {
            max_tokens: Some(max_tokens),
            temperature: Some(0.0),
            ..self.clone()
        }
    }
}
