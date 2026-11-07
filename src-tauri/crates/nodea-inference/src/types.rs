//! The engine-neutral vocabulary: what a generation is asked for, and what it
//! streams back. Nothing in here names an inference engine.

/// Generation cap when the caller does not send `max_tokens`.
pub const DEFAULT_MAX_TOKENS: usize = 500;
/// Matches the OpenAI API default when the caller does not send `temperature`.
pub const DEFAULT_TEMPERATURE: f64 = 1.0;
/// The system prompt used only when the caller supplied none of its own.
pub const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful AI assistant.";

/// OpenAI-style sampling options for one generation. `None` means "use the
/// default", which is what an unset field in the gRPC request maps to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GenerationOptions {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_tokens: Option<usize>,
    /// From the proto's int64 seed. Engines that cannot seed per request
    /// ignore it.
    pub seed: Option<i64>,
    pub stop: Vec<String>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    /// OpenAI's response_format object as JSON, exactly as the caller sent it.
    /// The backend has already validated the envelope.
    pub response_format: Option<String>,
    /// OpenAI's tools array as JSON, in OpenAI's own wire shape.
    pub tools: Option<String>,
    /// OpenAI's tool_choice as JSON (a string or an object).
    pub tool_choice: Option<String>,
    /// Whether the caller asked for log probabilities.
    pub logprobs: bool,
    /// How many alternatives per token, from OpenAI's top_logprobs.
    pub top_logprobs: Option<usize>,
    /// How many completions to generate, from OpenAI's n. None means one.
    pub n: Option<usize>,
    /// Per-token logit adjustments, keyed by *this* model's token ids.
    pub logit_bias: std::collections::HashMap<u32, f32>,
    /// OpenAI's web_search_options object as JSON.
    pub web_search_options: Option<String>,
    /// OpenAI's reasoning_effort, already narrowed by the backend to what the
    /// engine can express (low, medium, high).
    pub reasoning_effort: Option<String>,
}

impl GenerationOptions {
    /// Greedy decoding, used by the local `generate_text` command.
    pub fn greedy(max_tokens: usize) -> Self {
        Self {
            temperature: Some(0.0),
            max_tokens: Some(max_tokens),
            ..Self::default()
        }
    }
}

/// One turn of a conversation, as the backend sends it.
///
/// Roles arrive already normalised by the backend ("developer" is OpenAI's
/// rename of "system").
#[derive(Clone, Debug, PartialEq)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
    /// An assistant turn's tool calls, as OpenAI's tool_calls array in JSON.
    pub tool_calls: Option<String>,
    /// A tool turn's tool_call_id.
    pub tool_call_id: Option<String>,
}

impl ChatTurn {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Whether this turn is a system prompt ("developer" is OpenAI's rename).
    pub fn is_system(&self) -> bool {
        matches!(self.role.as_str(), "system" | "developer")
    }
}

/// A log line for a single-prompt generation. Metadata only: the prompt is
/// someone's inference traffic and must never reach stdout.
pub fn describe_prompt(prompt: &str, max_tokens: usize) -> String {
    format!(
        "Generating response: prompt {} chars, max_tokens {}",
        prompt.chars().count(),
        max_tokens
    )
}

/// Why a generation failed.
///
/// `code` is set when the request itself was the cause, so the backend can
/// answer with a 400 its client can act on instead of a 502 it would retry.
/// It travels as `GeneratedText.error_code`.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationError {
    pub code: Option<&'static str>,
    pub message: String,
}

/// The prompt does not fit the model's context window.
pub const ERROR_CONTEXT_LENGTH_EXCEEDED: &str = "context_length_exceeded";
/// Something in the request the node cannot serve, e.g. a `logit_bias` id
/// outside this model's vocabulary.
pub const ERROR_INVALID_REQUEST: &str = "invalid_request";

impl GenerationError {
    pub fn context_length_exceeded(message: impl Into<String>) -> Self {
        Self {
            code: Some(ERROR_CONTEXT_LENGTH_EXCEEDED),
            message: message.into(),
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: Some(ERROR_INVALID_REQUEST),
            message: message.into(),
        }
    }
}

/// An error with no code: the node's own failure, not the request's.
impl From<String> for GenerationError {
    fn from(message: String) -> Self {
        Self {
            code: None,
            message,
        }
    }
}

impl From<&str> for GenerationError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

impl std::fmt::Display for GenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GenerationError {}

/// How a generation ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GenerationOutcome {
    /// The model finished, or hit a stop condition.
    Completed,
    /// The backend asked us to stop, because its client went away.
    Cancelled,
}

/// Real token counts, as the engine reports them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GenerationUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// One callback invocation: a piece of text, and on the terminal one, the
/// metadata that only exists at the end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenUpdate {
    /// Which of the n requested completions this belongs to.
    pub choice_index: usize,
    pub text: String,
    /// This choice has finished; its finish_reason is set.
    pub is_last: bool,
    /// Every choice has finished, so there are no more tokens for this request
    /// at all. Equal to `is_last` whenever n == 1, which is the default.
    pub is_final: bool,
    /// Set only on the terminal update. Already mapped to OpenAI's vocabulary.
    pub finish_reason: Option<String>,
    /// Set only on the terminal update, and only when the engine reported it.
    pub usage: Option<GenerationUsage>,
    /// Tool calls the model emitted, as OpenAI's tool_calls array in JSON.
    ///
    /// The engine parses these out of the generated text and emits each one
    /// complete in a single chunk, alongside finish_reason "tool_calls" - it
    /// does not stream partial argument deltas.
    pub tool_calls: Option<String>,
    /// This chunk's log probabilities, as one OpenAI ChatCompletionTokenLogprob
    /// object in JSON.
    pub logprobs: Option<String>,
}

/// How large a context window to load a model with. Either way it is never
/// larger than the context the model was trained for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContextSize {
    /// Exactly this many tokens.
    Fixed(u32),
    /// As large as fits the device's free memory, between `min` and `max`.
    /// `min` is kept even when it does not strictly fit, so a device never
    /// gets less than that.
    Fit { min: u32, max: u32 },
}

/// The app's context: at least 8,192 tokens, up to 32,768 where memory allows.
pub const DEFAULT_CONTEXT: ContextSize = ContextSize::Fit {
    min: 8192,
    max: 32768,
};

/// How to load a model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadOptions {
    /// Context window, clamped to the model's trained context.
    pub n_ctx: ContextSize,
    /// Layers to offload to the GPU. `u32::MAX` is all of them; 0 forces CPU.
    pub n_gpu_layers: u32,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            n_ctx: DEFAULT_CONTEXT,
            n_gpu_layers: u32::MAX,
        }
    }
}

/// What a loaded model can tell us about itself. Fields an engine cannot
/// report are `None`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct ModelInfo {
    pub engine: &'static str,
    /// GGUF `general.architecture`, e.g. "llama" or "qwen3".
    pub architecture: Option<String>,
    /// The context window this load uses.
    pub n_ctx: Option<u32>,
    pub n_vocab: Option<u32>,
    /// The chat format llama.cpp detected for the template, e.g. "Hermes 2 Pro".
    pub chat_format: Option<String>,
    pub supports_tools: bool,
    pub supports_thinking: bool,
}

/// The prompt a request renders to, for debugging templates and tool calls.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct RenderedPrompt {
    pub prompt: String,
    /// GBNF grammar constraining output; empty when unconstrained.
    pub grammar: String,
    /// The grammar only engages once a trigger appears in the output.
    pub grammar_lazy: bool,
    pub chat_format: String,
}
