//! A request, as llama.cpp's chat layer wants it (notes/llama-cpp-4-migration.md
//! §4.4 step 1, §5.1).
//!
//! Everything here is a pure function of the request, so a request the node
//! cannot honour fails before any generation starts. Templates, tool-call
//! formats and reasoning are llama.cpp's (`common/chat`): we never parse model
//! markers ourselves (D7).

use crate::types::{ChatTurn, GenerationOptions, DEFAULT_SYSTEM_PROMPT};
use llama_cpp_4::chat::{ChatApplyParams, ToolChoice};
use serde_json::{json, Value};

/// The inputs to `ChatTemplates::apply`, as plain data so they can be tested.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ChatInputs {
    /// OpenAI-shaped `messages`.
    pub messages: Value,
    /// OpenAI-shaped `tools`, already narrowed to a named tool_choice.
    pub tools: Option<Value>,
    pub tool_choice: ToolChoice,
    /// From response_format.
    pub json_schema: Option<Value>,
    /// Extra template variables, e.g. `reasoning_effort`.
    pub template_kwargs: Option<Value>,
}

impl ChatInputs {
    pub fn from_request(turns: &[ChatTurn], options: &GenerationOptions) -> Result<Self, String> {
        if options.web_search_options.is_some() {
            return Err("web_search is not supported by this node".to_string());
        }

        let (tools, tool_choice) = tools(options.tools.as_deref(), options.tool_choice.as_deref())?;

        Ok(Self {
            messages: messages(turns)?,
            tools,
            tool_choice,
            json_schema: json_schema(options.response_format.as_deref())?,
            template_kwargs: reasoning_kwargs(options.reasoning_effort.as_deref())?,
        })
    }

    pub fn has_tools(&self) -> bool {
        self.tools.is_some()
    }

    /// `add_bos`/`add_eos` come from the model's vocabulary: llama.cpp strips a
    /// template-rendered BOS when the tokenizer will add one itself.
    pub fn apply_params(&self, add_bos: bool, add_eos: bool) -> ChatApplyParams {
        let mut params = ChatApplyParams::new(self.messages.to_string())
            .with_tool_choice(self.tool_choice)
            .with_add_bos_eos(add_bos, add_eos);
        if let Some(tools) = &self.tools {
            params = params.with_tools(tools.to_string());
        }
        if let Some(schema) = &self.json_schema {
            params = params.with_json_schema(schema.to_string());
        }
        if let Some(kwargs) = &self.template_kwargs {
            params = params.with_template_kwargs(kwargs.to_string());
        }
        params
    }
}

fn role(turn: &ChatTurn) -> &'static str {
    match turn.role.as_str() {
        "system" | "developer" => "system",
        "assistant" => "assistant",
        "tool" => "tool",
        // An unknown role is kept as a user turn rather than dropped: dropping
        // it would silently remove content the model was meant to see.
        _ => "user",
    }
}

/// The conversation as OpenAI-shaped messages.
pub(super) fn messages(turns: &[ChatTurn]) -> Result<Value, String> {
    let mut out = Vec::with_capacity(turns.len() + 1);

    // Only supply a default system prompt when the caller gave none: adding
    // ours on top of theirs is how the caller's instructions got buried.
    if !turns.iter().any(ChatTurn::is_system) {
        out.push(json!({"role": "system", "content": DEFAULT_SYSTEM_PROMPT}));
    }

    for turn in turns {
        let mut message = json!({"role": role(turn), "content": turn.content});
        // An assistant turn that called a tool. Replaying it matters: the
        // model must see that it asked before it sees the answer.
        if let Some(raw) = &turn.tool_calls {
            let calls: Value = serde_json::from_str(raw)
                .map_err(|e| format!("Could not parse tool_calls: {}", e))?;
            if !calls.is_array() {
                return Err("Could not parse tool_calls: not an array".to_string());
            }
            message["tool_calls"] = calls;
        }
        // A tool's reply, which needs the id of the call it answers.
        if let Some(id) = &turn.tool_call_id {
            message["tool_call_id"] = json!(id);
        }
        out.push(message);
    }
    Ok(Value::Array(out))
}

/// OpenAI's tools and tool_choice. A named tool_choice keeps only that tool and
/// requires a call, which is how llama.cpp forces one function.
///
/// `"none"` drops the tools from the prompt. llama.cpp's own `None` still
/// shows them to the model and then does not parse calls, so a model that
/// calls one anyway leaks its raw tool-call markup as text; a model that
/// never sees the tools answers in prose, which is what "none" asks for.
pub(super) fn tools(
    tools: Option<&str>,
    tool_choice: Option<&str>,
) -> Result<(Option<Value>, ToolChoice), String> {
    let Some(raw) = tools else {
        return Ok((None, ToolChoice::Auto));
    };
    let mut tools: Vec<Value> =
        serde_json::from_str(raw).map_err(|e| format!("Could not parse tools: {}", e))?;

    let choice = match tool_choice {
        None => ToolChoice::Auto,
        Some(raw) => {
            let value: Value = serde_json::from_str(raw)
                .map_err(|e| format!("Could not parse tool_choice: {}", e))?;
            match &value {
                Value::String(word) => match word.as_str() {
                    "auto" => ToolChoice::Auto,
                    "none" => return Ok((None, ToolChoice::None)),
                    "required" => ToolChoice::Required,
                    other => return Err(format!("Unsupported tool_choice: {other}")),
                },
                Value::Object(_) => {
                    let name = value["function"]["name"]
                        .as_str()
                        .ok_or("Could not parse tool_choice: no function.name")?;
                    tools.retain(|tool| tool["function"]["name"] == name);
                    if tools.is_empty() {
                        return Err(format!("tool_choice names {name:?}, which is not in tools"));
                    }
                    ToolChoice::Required
                }
                _ => return Err(format!("Unsupported tool_choice: {value}")),
            }
        }
    };
    Ok((Some(Value::Array(tools)), choice))
}

/// OpenAI's response_format as a JSON schema.
///
/// `json_object` is the schema `{"type": "object"}`: that is exactly what it
/// means, and it reuses the json_schema path instead of a second grammar.
pub(super) fn json_schema(response_format: Option<&str>) -> Result<Option<Value>, String> {
    let Some(raw) = response_format else {
        return Ok(None);
    };
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("Could not parse response_format: {}", e))?;

    match value.get("type").and_then(|t| t.as_str()) {
        Some("json_object") => Ok(Some(json!({"type": "object"}))),
        Some("json_schema") => value
            .get("json_schema")
            .and_then(|w| w.get("schema"))
            .cloned()
            .map(Some)
            .ok_or_else(|| "response_format.json_schema.schema is missing".to_string()),
        // "text" is OpenAI's default and the backend does not send it; anything
        // else the backend already rejected.
        _ => Ok(None),
    }
}

/// OpenAI's reasoning_effort as a template variable. Templates that do not
/// read it ignore it. The backend has already narrowed it to these three, so
/// anything else is a bug, not something to round.
///
/// Each value is JSON *text*: llama.cpp parses every template kwarg as JSON,
/// and the shim passes a string value through unquoted, so `"high"` must be
/// sent as `"\"high\""`.
pub(super) fn reasoning_kwargs(effort: Option<&str>) -> Result<Option<Value>, String> {
    match effort {
        None => Ok(None),
        Some(effort @ ("low" | "medium" | "high")) => Ok(Some(
            json!({"reasoning_effort": Value::from(effort).to_string()}),
        )),
        Some(other) => Err(format!("Unsupported reasoning_effort: {}", other)),
    }
}

/// llama.cpp's parsed message (`ChatParams::parse`), reduced to what we send.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Parsed {
    pub content: String,
    pub tool_calls: Vec<Value>,
}

pub(super) fn parsed_message(json: &str) -> Result<Parsed, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("parsed message is not JSON: {e}"))?;
    Ok(Parsed {
        content: value["content"].as_str().unwrap_or_default().to_string(),
        tool_calls: value["tool_calls"].as_array().cloned().unwrap_or_default(),
    })
}

/// Tool calls in OpenAI's wire shape: `type: "function"`, an id, and
/// `arguments` as a JSON *string*. An id the parser left empty becomes
/// `call_{choice}_{k}`.
pub(super) fn openai_tool_calls(calls: &[Value], choice: usize) -> Value {
    calls
        .iter()
        .enumerate()
        .map(|(k, call)| {
            let function = &call["function"];
            let arguments = match &function["arguments"] {
                Value::String(s) => s.clone(),
                Value::Null => "{}".to_string(),
                other => other.to_string(),
            };
            let id = call["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("call_{choice}_{k}"));
            json!({
                "id": id,
                "type": "function",
                "function": {"name": function["name"], "arguments": arguments},
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(role: &str, content: &str) -> ChatTurn {
        ChatTurn {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn roles_and_content(turns: &[ChatTurn]) -> Vec<(String, String)> {
        messages(turns)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    m["role"].as_str().unwrap().to_string(),
                    m["content"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    const TOOLS: &str = r#"[{"type":"function","function":{"name":"get_weather",
        "parameters":{"type":"object","properties":{"city":{"type":"string"}}}}},
        {"type":"function","function":{"name":"get_time"}}]"#;

    #[test]
    fn the_whole_conversation_reaches_the_template() {
        // Regression guard: the node used to take messages.last() only.
        let rendered = roles_and_content(&[
            turn("system", "You are terse."),
            turn("user", "Hi"),
            turn("assistant", "Hello."),
            turn("user", "Again"),
        ]);
        assert_eq!(
            rendered,
            vec![
                ("system".into(), "You are terse.".into()),
                ("user".into(), "Hi".into()),
                ("assistant".into(), "Hello.".into()),
                ("user".into(), "Again".into()),
            ]
        );
    }

    #[test]
    fn the_default_system_prompt_is_added_only_when_the_caller_sent_none() {
        let rendered = roles_and_content(&[ChatTurn::user("Hi")]);
        assert_eq!(rendered[0], ("system".into(), DEFAULT_SYSTEM_PROMPT.into()));
        assert_eq!(rendered.len(), 2);

        let theirs = roles_and_content(&[turn("system", "Be terse."), ChatTurn::user("Hi")]);
        assert_eq!(theirs.len(), 2);
        assert!(!theirs.iter().any(|(_, text)| text == DEFAULT_SYSTEM_PROMPT));
    }

    #[test]
    fn developer_is_treated_as_system() {
        let rendered = roles_and_content(&[turn("developer", "Be terse."), ChatTurn::user("Hi")]);
        assert_eq!(rendered[0].0, "system");
        assert_eq!(rendered.len(), 2);
    }

    #[test]
    fn an_unknown_role_is_kept_as_a_user_turn_rather_than_dropped() {
        let rendered = roles_and_content(&[turn("moderator", "Be careful")]);
        assert_eq!(rendered[1], ("user".into(), "Be careful".into()));
    }

    #[test]
    fn a_tool_call_and_its_reply_are_replayed_in_openai_shape() {
        let turns = vec![
            ChatTurn::user("weather?"),
            ChatTurn {
                role: "assistant".to_string(),
                content: String::new(),
                tool_calls: Some(
                    r#"[{"id":"call_1","type":"function",
                    "function":{"name":"get_weather","arguments":"{\"city\":\"Oslo\"}"}}]"#
                        .to_string(),
                ),
                tool_call_id: None,
            },
            ChatTurn {
                role: "tool".to_string(),
                content: "sunny".to_string(),
                tool_calls: None,
                tool_call_id: Some("call_1".to_string()),
            },
        ];

        let messages = messages(&turns).unwrap();

        assert_eq!(
            messages[2]["tool_calls"][0]["function"]["name"],
            "get_weather"
        );
        assert_eq!(
            messages[2]["tool_calls"][0]["function"]["arguments"],
            r#"{"city":"Oslo"}"#
        );
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["tool_call_id"], "call_1");
    }

    #[test]
    fn unparseable_tool_calls_on_a_message_are_an_error() {
        let mut bad = turn("assistant", "");
        bad.tool_calls = Some("not json".to_string());
        assert!(messages(&[bad]).is_err());
    }

    #[test]
    fn tool_choice_defaults_to_auto_and_understands_the_three_words() {
        assert_eq!(tools(Some(TOOLS), None).unwrap().1, ToolChoice::Auto);
        assert_eq!(
            tools(Some(TOOLS), Some(r#""none""#)).unwrap(),
            (None, ToolChoice::None),
            "none hides the tools from the model"
        );
        assert_eq!(
            tools(Some(TOOLS), Some(r#""auto""#)).unwrap().1,
            ToolChoice::Auto
        );
        assert_eq!(
            tools(Some(TOOLS), Some(r#""required""#)).unwrap().1,
            ToolChoice::Required
        );
    }

    #[test]
    fn a_named_tool_choice_keeps_only_that_tool_and_requires_it() {
        let (tools, choice) = tools(
            Some(TOOLS),
            Some(r#"{"type":"function","function":{"name":"get_time"}}"#),
        )
        .unwrap();

        let tools = tools.unwrap();
        assert_eq!(tools.as_array().unwrap().len(), 1);
        assert_eq!(tools[0]["function"]["name"], "get_time");
        assert_eq!(choice, ToolChoice::Required);
    }

    #[test]
    fn naming_a_tool_that_is_not_there_is_an_error() {
        assert!(tools(
            Some(TOOLS),
            Some(r#"{"type":"function","function":{"name":"nope"}}"#)
        )
        .is_err());
    }

    #[test]
    fn unparseable_tools_are_an_error_not_a_silent_prose_answer() {
        assert!(tools(Some("not json"), None).is_err());
        assert!(tools(Some(TOOLS), Some("not json")).is_err());
    }

    #[test]
    fn no_tools_means_none() {
        assert_eq!(
            tools(None, Some(r#""required""#)).unwrap(),
            (None, ToolChoice::Auto)
        );
    }

    #[test]
    fn json_object_constrains_to_any_object() {
        assert_eq!(
            json_schema(Some(r#"{"type":"json_object"}"#)).unwrap(),
            Some(json!({"type": "object"}))
        );
    }

    #[test]
    fn json_schema_passes_the_callers_schema_through_verbatim() {
        let raw = r#"{"type":"json_schema","json_schema":{"name":"p",
            "schema":{"type":"object","properties":{"a":{"type":"integer"}},"required":["a"]}}}"#;
        assert_eq!(
            json_schema(Some(raw)).unwrap(),
            Some(json!({"type":"object","properties":{"a":{"type":"integer"}},"required":["a"]}))
        );
    }

    #[test]
    fn an_unusable_response_format_is_an_error_not_silent_prose() {
        assert!(json_schema(Some("not json")).is_err());
        assert!(json_schema(Some(r#"{"type":"json_schema"}"#)).is_err());
        assert_eq!(json_schema(None).unwrap(), None);
    }

    #[test]
    fn reasoning_effort_becomes_a_template_variable() {
        assert_eq!(
            reasoning_kwargs(Some("high")).unwrap(),
            Some(json!({"reasoning_effort": "\"high\""}))
        );
        assert_eq!(reasoning_kwargs(None).unwrap(), None);
        for effort in ["minimal", "none", "xhigh", "max"] {
            assert!(reasoning_kwargs(Some(effort)).is_err(), "{effort}");
        }
    }

    #[test]
    fn web_search_options_are_refused() {
        let error = ChatInputs::from_request(
            &[ChatTurn::user("news?")],
            &GenerationOptions {
                web_search_options: Some("{}".to_string()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.contains("web_search"), "{error}");
    }

    #[test]
    fn every_field_reaches_the_inputs() {
        let inputs = ChatInputs::from_request(
            &[ChatTurn::user("hi")],
            &GenerationOptions {
                tools: Some(TOOLS.to_string()),
                tool_choice: Some(r#""required""#.to_string()),
                response_format: Some(r#"{"type":"json_object"}"#.to_string()),
                reasoning_effort: Some("low".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(inputs.messages.as_array().unwrap().len(), 2);
        assert_eq!(inputs.tools.as_ref().unwrap().as_array().unwrap().len(), 2);
        assert_eq!(inputs.tool_choice, ToolChoice::Required);
        assert_eq!(inputs.json_schema, Some(json!({"type": "object"})));
        assert_eq!(
            inputs.template_kwargs,
            Some(json!({"reasoning_effort": "\"low\""}))
        );
    }

    #[test]
    fn parsed_tool_calls_get_openais_shape_and_ids() {
        let calls = vec![
            json!({"type":"function","id":"","function":{"name":"a","arguments":{"x":1}}}),
            json!({"id":"keep","function":{"name":"b","arguments":"{}"}}),
        ];

        let shaped = openai_tool_calls(&calls, 2);

        assert_eq!(shaped[0]["id"], "call_2_0");
        assert_eq!(shaped[0]["type"], "function");
        assert_eq!(shaped[0]["function"]["arguments"], r#"{"x":1}"#);
        assert_eq!(shaped[1]["id"], "keep");
        assert_eq!(shaped[1]["function"]["arguments"], "{}");
    }

    #[test]
    fn a_parsed_message_reads_content_and_tool_calls() {
        let parsed = parsed_message(
            r#"{"role":"assistant","content":"hi","reasoning_content":"hmm","tool_calls":[{"function":{"name":"f"}}]}"#,
        )
        .unwrap();
        assert_eq!(parsed.content, "hi");
        assert_eq!(parsed.tool_calls.len(), 1);

        let empty = parsed_message(r#"{"role":"assistant","content":null}"#).unwrap();
        assert_eq!(empty, Parsed::default());
    }
}
