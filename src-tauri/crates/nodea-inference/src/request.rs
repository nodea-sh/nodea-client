//! Translating the backend's gRPC `ChatCompletionRequest` into what the
//! inference engine wants.
//!
//! Deliberately pure, with no transport or engine dependencies: it is the whole
//! OpenAI-compatibility surface on the node side, and keeping it pure is what
//! lets the contract tests drive every field of a real protobuf request.

use crate::types::{ChatTurn, GenerationOptions};
use nodea_proto::ChatCompletionRequest;

/// Map the request's OpenAI sampling fields onto generation options. Unset
/// fields (older backends never set them) stay `None` and get the defaults.
pub fn generation_options(chat_request: &ChatCompletionRequest) -> GenerationOptions {
    GenerationOptions {
        temperature: chat_request.temperature.map(f64::from),
        top_p: chat_request.top_p.map(f64::from),
        max_tokens: chat_request.max_tokens.map(|n| n as usize),
        seed: chat_request.seed,
        stop: chat_request.stop.clone(),
        frequency_penalty: chat_request.frequency_penalty,
        presence_penalty: chat_request.presence_penalty,
        response_format: chat_request.response_format.clone(),
        tools: chat_request.tools.clone(),
        tool_choice: chat_request.tool_choice.clone(),
        logprobs: chat_request.logprobs.unwrap_or(false),
        top_logprobs: chat_request.top_logprobs.map(|n| n as usize),
        n: chat_request.n.map(|n| n as usize),
        logit_bias: chat_request.logit_bias.clone(),
        web_search_options: chat_request.web_search_options.clone(),
        reasoning_effort: chat_request.reasoning_effort.clone(),
    }
}

/// The conversation, as the engine's chat template will see it.
///
/// A backend predating structured messages sends exactly one flattened user
/// message, so this handles both shapes without a version check.
pub fn chat_turns(chat_request: &ChatCompletionRequest) -> Vec<ChatTurn> {
    chat_request
        .messages
        .iter()
        .map(|message| ChatTurn {
            role: message.role.clone(),
            content: message.content.clone(),
            tool_calls: message.tool_calls.clone(),
            tool_call_id: message.tool_call_id.clone(),
        })
        .collect()
}

/// Fields the proto carries as JSON *strings*. `from_json` also accepts them
/// as inline JSON, which keeps hand-written requests readable.
const JSON_STRING_FIELDS: &[&str] = &[
    "response_format",
    "tools",
    "tool_choice",
    "web_search_options",
];

/// A `ChatCompletionRequest` from JSON with proto field names, as the
/// `nodea-infer` harness and the contract tests write them.
///
/// `tools`, `tool_choice`, `response_format`, `web_search_options` and a
/// message's `tool_calls` may be given either as the JSON text the proto
/// carries or as inline JSON, which is serialised into that text. A bare
/// `tool_choice` word such as `"required"` is encoded as the JSON string the
/// backend would send (`"\"required\""`).
pub fn from_json(json: &str) -> Result<ChatCompletionRequest, String> {
    let mut value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("request is not JSON: {e}"))?;

    fn stringify(slot: &mut serde_json::Value) {
        if !slot.is_string() && !slot.is_null() {
            *slot = serde_json::Value::String(slot.to_string());
        }
    }

    if let Some(object) = value.as_object_mut() {
        for field in JSON_STRING_FIELDS {
            if let Some(slot) = object.get_mut(*field) {
                stringify(slot);
            }
        }
        if let Some(slot) = object.get_mut("tool_choice") {
            if let Some(word) = slot.as_str() {
                if serde_json::from_str::<serde_json::Value>(word).is_err() {
                    *slot = serde_json::Value::String(serde_json::Value::from(word).to_string());
                }
            }
        }
        if let Some(messages) = object.get_mut("messages").and_then(|m| m.as_array_mut()) {
            for message in messages {
                if let Some(slot) = message.get_mut("tool_calls") {
                    stringify(slot);
                }
            }
        }
    }

    serde_json::from_value(value).map_err(|e| format!("not a ChatCompletionRequest: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodea_proto::ChatMessage;

    #[test]
    fn generation_options_default_when_the_backend_sends_no_sampling_fields() {
        let request = ChatCompletionRequest::default();
        assert_eq!(generation_options(&request), GenerationOptions::default());
    }

    #[test]
    fn generation_options_carry_the_request_sampling_fields() {
        let request = ChatCompletionRequest {
            temperature: Some(0.5),
            top_p: Some(0.25),
            max_tokens: Some(128),
            seed: Some(7),
            stop: vec!["END".to_string()],
            frequency_penalty: Some(1.5),
            presence_penalty: Some(-1.0),
            ..Default::default()
        };

        assert_eq!(
            generation_options(&request),
            GenerationOptions {
                temperature: Some(0.5),
                top_p: Some(0.25),
                max_tokens: Some(128),
                seed: Some(7),
                stop: vec!["END".to_string()],
                frequency_penalty: Some(1.5),
                presence_penalty: Some(-1.0),
                ..Default::default()
            }
        );
    }

    #[test]
    fn chat_turns_carry_the_tool_fields() {
        let request = ChatCompletionRequest {
            messages: vec![ChatMessage {
                role: "tool".to_string(),
                content: "sunny".to_string(),
                name: Some("weather".to_string()),
                tool_calls: None,
                tool_call_id: Some("call_1".to_string()),
            }],
            ..Default::default()
        };

        let [turn] = &chat_turns(&request)[..] else {
            panic!("expected one turn")
        };

        assert_eq!(turn.role, "tool");
        assert_eq!(turn.tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn no_messages_yields_no_turns() {
        assert!(chat_turns(&ChatCompletionRequest::default()).is_empty());
    }

    #[test]
    fn inline_json_fields_become_the_strings_the_proto_carries() {
        let request = from_json(
            r#"{"messages":[{"role":"assistant","content":"",
                "tool_calls":[{"id":"c","type":"function","function":{"name":"f","arguments":"{}"}}]}],
              "tools":[{"type":"function","function":{"name":"f"}}],
              "tool_choice":"auto",
              "response_format":{"type":"json_object"}}"#,
        )
        .unwrap();

        let tools: serde_json::Value =
            serde_json::from_str(request.tools.as_deref().unwrap()).unwrap();
        assert_eq!(tools[0]["function"]["name"], "f");
        // A bare word becomes the JSON string the backend sends.
        assert_eq!(request.tool_choice.as_deref(), Some(r#""auto""#));
        assert_eq!(
            request.response_format.as_deref(),
            Some(r#"{"type":"json_object"}"#)
        );
        assert!(request.messages[0]
            .tool_calls
            .as_deref()
            .unwrap()
            .starts_with('['));
    }

    #[test]
    fn json_text_is_passed_through_unchanged() {
        let request = from_json(r#"{"tool_choice":"\"none\"","tools":"not json"}"#).unwrap();

        assert_eq!(request.tool_choice.as_deref(), Some(r#""none""#));
        // Left for the engine to reject: that is what the contract tests check.
        assert_eq!(request.tools.as_deref(), Some("not json"));
    }

    #[test]
    fn a_request_parses_from_json_with_proto_field_names() {
        // What the nodea-infer harness and the contract tests rely on.
        let request: ChatCompletionRequest = serde_json::from_str(
            r#"{"messages":[{"role":"user","content":"Hi"}],"max_tokens":8,"temperature":0}"#,
        )
        .unwrap();

        assert_eq!(request.messages[0].content, "Hi");
        assert_eq!(request.max_tokens, Some(8));
        assert_eq!(request.temperature, Some(0.0));
        assert_eq!(request.n, None);
    }
}
