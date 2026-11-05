//! Proto contract: tools, response_format and reasoning
//! (notes/llama-cpp-4-migration.md §5.1, §8.5).
//!
//! The `tools` fixture is Qwen3-0.6B, a thinking model. Prompts end in
//! `/no_think`, Qwen3's own switch, so a small `max_tokens` is not spent on
//! reasoning; `reasoning_is_not_forwarded` covers the thinking path.

mod support;

use support::{assert_invariants, model, req, run};

const WEATHER_TOOL: &str = r#"{"type":"function","function":{"name":"get_weather",
    "description":"Get the current weather for a city",
    "parameters":{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}}}"#;

const TIME_TOOL: &str = r#"{"type":"function","function":{"name":"get_time",
    "description":"Get the current local time in a city",
    "parameters":{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}}}"#;

#[test]
fn json_object_produces_an_object() {
    let Some(model) = model("tools") else { return };
    let request = req(r#"{"messages":[{"role":"user",
        "content":"Describe a cat as JSON with a name and a colour. /no_think"}],
        "max_tokens":96,"temperature":0,"response_format":{"type":"json_object"}}"#);

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    let value: serde_json::Value = serde_json::from_str(run.text(0).trim())
        .unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", run.text(0)));
    assert!(value.is_object(), "{value}");
}

#[test]
fn json_schema_is_enforced() {
    let Some(model) = model("tools") else { return };
    let request = req(r#"{"messages":[{"role":"user",
        "content":"Invent a fictional person. Reply with JSON giving their first name and their age. /no_think"}],
        "max_tokens":96,"temperature":0,
        "response_format":{"type":"json_schema","json_schema":{"name":"person",
          "schema":{"type":"object","properties":{"name":{"type":"string"},"age":{"type":"integer"}},
          "required":["name","age"],"additionalProperties":false}}}}"#);

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    let value: serde_json::Value = serde_json::from_str(run.text(0).trim())
        .unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", run.text(0)));
    let object = value.as_object().expect("an object");
    assert!(object["name"].is_string(), "{value}");
    assert!(object["age"].is_i64() || object["age"].is_u64(), "{value}");
    assert_eq!(object.len(), 2, "additionalProperties is false: {value}");
}

#[test]
#[cfg_attr(
    feature = "engine-mistralrs",
    ignore = "mistralrs's forced (required) tool call repeats the same call several times"
)]
fn tool_call_is_parsed_into_openai_shape() {
    let Some(model) = model("tools") else { return };
    let request = req(&format!(
        r#"{{"messages":[{{"role":"user","content":"What is the weather in Oslo? /no_think"}}],
        "max_tokens":128,"temperature":0,"tools":[{WEATHER_TOOL}],"tool_choice":"required"}}"#
    ));

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert_eq!(run.finish_reason(0), "tool_calls", "{:#?}", run.messages);
    let calls = run.tool_calls(0);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0]["function"]["name"], "get_weather");
    let arguments: serde_json::Value =
        serde_json::from_str(calls[0]["function"]["arguments"].as_str().unwrap()).unwrap();
    assert!(arguments["city"].is_string(), "{arguments}");
    assert!(!run.text(0).contains("<tool_call>"), "{:?}", run.text(0));
}

#[test]
fn named_tool_choice_forces_that_tool() {
    let Some(model) = model("tools") else { return };
    // Two tools and a prompt that favours neither: the call going to
    // get_time is the named choice at work. (A weather question forced onto
    // get_time invites prose first, which llama.cpp's grammar allows before
    // the call, and on x86 the greedy path ran out of tokens doing it.)
    let request = req(&format!(
        r#"{{"messages":[{{"role":"user","content":"Use one of your tools for Oslo. /no_think"}}],
        "max_tokens":128,"temperature":0,"tools":[{WEATHER_TOOL},{TIME_TOOL}],
        "tool_choice":{{"type":"function","function":{{"name":"get_time"}}}}}}"#
    ));

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert_eq!(run.finish_reason(0), "tool_calls", "{:#?}", run.messages);
    let calls = run.tool_calls(0);
    assert!(!calls.is_empty());
    assert!(
        calls.iter().all(|c| c["function"]["name"] == "get_time"),
        "{calls:?}"
    );
}

#[test]
#[cfg_attr(
    feature = "engine-mistralrs",
    ignore = "with tool_choice none, mistralrs passes the model's raw <tool_call> markup through as text"
)]
fn tool_choice_none_answers_in_prose() {
    let Some(model) = model("tools") else { return };
    let request = req(&format!(
        r#"{{"messages":[{{"role":"user","content":"What is the weather in Oslo? /no_think"}}],
        "max_tokens":48,"temperature":0,"tools":[{WEATHER_TOOL}],"tool_choice":"none"}}"#
    ));

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert_ne!(run.finish_reason(0), "tool_calls");
    assert!(run.tool_calls(0).is_empty());
}

#[test]
fn tool_reply_is_replayed_and_answered() {
    let Some(model) = model("tools") else { return };
    let request = req(&format!(
        r#"{{"messages":[
          {{"role":"user","content":"What is the weather in Oslo? /no_think"}},
          {{"role":"assistant","content":"","tool_calls":[{{"id":"call_1","type":"function",
            "function":{{"name":"get_weather","arguments":"{{\"city\":\"Oslo\"}}"}}}}]}},
          {{"role":"tool","tool_call_id":"call_1","content":"{{\"temperature_c\":17,\"sky\":\"sunny\"}}"}}],
        "max_tokens":256,"temperature":0,"tools":[{WEATHER_TOOL}]}}"#
    ));

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert_eq!(run.finish_reason(0), "stop", "{:#?}", run.messages);
    let text = run.text(0);
    assert!(
        text.contains("17") || text.to_lowercase().contains("sunny"),
        "the answer should use the tool result: {text:?}"
    );
}

#[test]
fn unparseable_tools_fail_loudly() {
    // A half-working tools implementation is worse than an honest failure,
    // because clients retry-loop instead of failing.
    let Some(model) = model("smoke") else { return };
    let request = req(
        r#"{"messages":[{"role":"user","content":"Hi"}],"max_tokens":4,
        "tools":"not json"}"#,
    );

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert!(run.error().is_some(), "{:#?}", run.messages);
}

#[test]
fn reasoning_is_not_forwarded() {
    let Some(model) = model("tools") else { return };
    // Thinking on (no /no_think): Qwen3 reasons in <think> first.
    let request = req(
        r#"{"messages":[{"role":"user","content":"What is 17 + 25?"}],
        "max_tokens":400,"temperature":0}"#,
    );

    let run = run(&model, &request);

    // assert_invariants already refuses <think> and </think> in any text.
    assert_invariants(&request, &run);
}

#[test]
fn reasoning_effort_reaches_the_template() {
    // That the kwarg reaches the template is an L0 test on the request
    // builder; here it only has to be accepted end to end.
    let Some(model) = model("tools") else { return };
    let request = req(r#"{"messages":[{"role":"user","content":"Hi /no_think"}],
        "max_tokens":16,"temperature":0,"reasoning_effort":"high"}"#);

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    assert!(run.error().is_none(), "{:?}", run.error());
}
