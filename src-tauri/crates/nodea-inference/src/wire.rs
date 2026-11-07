//! What the node puts on the wire for a generation: `TokenUpdate`s and
//! failures, as the proto's `GeneratedText`.

use crate::types::{GenerationError, TokenUpdate};
use nodea_proto::{GeneratedText, Usage};

/// One streamed update. Terminal metadata rides the terminal token, so the
/// backend never has to correlate two messages.
pub fn generated_text(request_id: &str, update: &TokenUpdate) -> GeneratedText {
    GeneratedText {
        request_id: request_id.to_string(),
        text: update.text.clone(),
        // `last` means "no more tokens for this request at all", not "this
        // choice finished" - with n > 1 the backend would otherwise stop
        // reading after the first completion to end.
        last: update.is_final,
        choice_index: Some(update.choice_index as u32),
        usage: update.usage.map(|usage| Usage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
        }),
        finish_reason: update.finish_reason.clone(),
        error: None,
        error_code: None,
        tool_calls: update.tool_calls.clone(),
        logprobs: update.logprobs.clone(),
    }
}

/// A terminal generation failure.
///
/// Shaped as a GeneratedText with `last` set and `error` populated - the same
/// shape the backend's own timeout sentinel already produces - so no new
/// message type or correlation logic is needed.
///
/// `error_code` says when the request itself was the cause (see
/// `GenerationError`); a backend that predates it reads `error` alone.
pub fn generation_error(request_id: &str, error: &GenerationError) -> GeneratedText {
    GeneratedText {
        request_id: request_id.to_string(),
        text: String::new(),
        last: true,
        usage: None,
        finish_reason: None,
        error: Some(error.message.clone()),
        error_code: error.code.map(str::to_string),
        tool_calls: None,
        logprobs: None,
        choice_index: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GenerationUsage;

    #[test]
    fn last_follows_is_final_not_is_last() {
        // With n > 1 a choice finishing is not the request finishing.
        let update = TokenUpdate {
            choice_index: 0,
            is_last: true,
            is_final: false,
            finish_reason: Some("stop".to_string()),
            ..Default::default()
        };

        let message = generated_text("req", &update);

        assert!(!message.last);
        assert_eq!(message.finish_reason.as_deref(), Some("stop"));
        assert_eq!(message.choice_index, Some(0));
    }

    #[test]
    fn terminal_metadata_is_carried() {
        let update = TokenUpdate {
            choice_index: 2,
            text: "hi".to_string(),
            is_last: true,
            is_final: true,
            finish_reason: Some("tool_calls".to_string()),
            usage: Some(GenerationUsage {
                prompt_tokens: 11,
                completion_tokens: 7,
            }),
            tool_calls: Some("[]".to_string()),
            logprobs: Some("{}".to_string()),
        };

        let message = generated_text("req-1", &update);

        assert_eq!(message.request_id, "req-1");
        assert_eq!(message.text, "hi");
        assert!(message.last);
        assert_eq!(message.choice_index, Some(2));
        assert_eq!(message.usage.unwrap().prompt_tokens, 11);
        assert_eq!(message.tool_calls.as_deref(), Some("[]"));
        assert_eq!(message.logprobs.as_deref(), Some("{}"));
        assert_eq!(message.error, None);
    }

    #[test]
    fn an_error_is_one_terminal_message_with_no_text() {
        let message = generation_error("req", &"boom".into());

        assert!(message.last);
        assert!(message.text.is_empty());
        assert_eq!(message.error.as_deref(), Some("boom"));
        assert_eq!(message.error_code, None);
        assert_eq!(message.finish_reason, None);
    }

    #[test]
    fn a_request_error_carries_its_code() {
        let message = generation_error(
            "req",
            &GenerationError::context_length_exceeded("The prompt is 9000 tokens"),
        );

        assert_eq!(
            message.error_code.as_deref(),
            Some("context_length_exceeded")
        );
        assert_eq!(message.error.as_deref(), Some("The prompt is 9000 tokens"));
    }
}
