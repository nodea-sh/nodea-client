//! The llama.cpp engine, through `llama-cpp-4`.

mod chat;
mod generate;
mod logprobs;
mod sampling;
mod stop;
mod utf8;

use crate::types::{
    ChatTurn, GenerationOptions, GenerationOutcome, LoadOptions, ModelInfo, RenderedPrompt,
    TokenUpdate,
};
use chat::ChatInputs;
use llama_cpp_4::chat::{ChatParams, ChatTemplates};
use llama_cpp_4::llama_backend::LlamaBackend;
use llama_cpp_4::model::params::LlamaModelParams;
use llama_cpp_4::model::{AddBos, LlamaModel};
use std::ffi::{c_char, c_void, CStr};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use tokio_util::sync::CancellationToken;

/// llama.cpp must be initialised exactly once per process, test binaries
/// included.
fn backend() -> &'static LlamaBackend {
    static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();
    BACKEND.get_or_init(|| {
        install_log_callback();
        LlamaBackend::init().expect("llama.cpp backend was initialised outside nodea-inference")
    })
}

/// llama.cpp's most recent error-level log lines. It reports *why* a load
/// failed only in its log (the Rust error is a bare null result), and that
/// reason - an unknown architecture, out of memory - is what the backend
/// needs in `ModelLoadStatus.error`.
static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

fn install_log_callback() {
    const ERROR: u32 = 4; // GGML_LOG_LEVEL_ERROR
    const WARN: u32 = 3;
    const CONT: u32 = 5;
    static VERBOSE: OnceLock<bool> = OnceLock::new();
    static LAST_LEVEL: Mutex<u32> = Mutex::new(0);

    // The level's integer type is the C enum's, which differs by platform.
    #[allow(clippy::unnecessary_cast)]
    unsafe extern "C" fn log(
        level: llama_cpp_sys_4::ggml_log_level,
        text: *const c_char,
        _: *mut c_void,
    ) {
        let level = level as u32;
        if text.is_null() {
            return;
        }
        // SAFETY: llama.cpp passes a NUL-terminated string valid for the call.
        let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
        let mut last = LAST_LEVEL.lock().unwrap_or_else(|e| e.into_inner());
        let level = if level == CONT { *last } else { level };
        *last = level;
        if level == ERROR {
            let mut error = LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner());
            error.push_str(&text);
        }
        // Loading a model logs every tensor; only warnings and errors by
        // default, everything with NODEA_LLAMA_LOG=1.
        if level >= WARN || *VERBOSE.get_or_init(|| std::env::var_os("NODEA_LLAMA_LOG").is_some()) {
            eprint!("{text}");
        }
    }

    // SAFETY: `log` is a plain function that lives for the whole process and
    // touches only statics; no user data pointer is passed.
    unsafe {
        llama_cpp_sys_4::llama_log_set(Some(log), std::ptr::null_mut());
    }
}

fn take_last_error() -> String {
    let mut error = LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *error).trim().to_string()
}

/// A loaded model, its chat templates and the context size it runs with.
pub struct Model {
    inner: Arc<Inner>,
}

pub(super) struct Inner {
    model: LlamaModel,
    templates: ChatTemplates,
    n_ctx: u32,
    info: ModelInfo,
}

impl Model {
    pub async fn load(path: &Path, options: LoadOptions) -> Result<Model, String> {
        // llama-cpp-4 debug-asserts the file exists; fail as an Err instead.
        if !path.exists() {
            return Err(format!("Model file not found: {:?}", path));
        }
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || load_blocking(&path, options))
            .await
            .map_err(|e| format!("Model load panicked: {e}"))?
    }

    pub fn info(&self) -> ModelInfo {
        self.inner.info.clone()
    }

    /// The prompt, grammar and chat format a request renders to.
    pub fn render_prompt(
        &self,
        turns: &[ChatTurn],
        options: &GenerationOptions,
    ) -> Result<RenderedPrompt, String> {
        let applied = self
            .inner
            .apply(&ChatInputs::from_request(turns, options)?)?;
        Ok(RenderedPrompt {
            prompt: applied.prompt.clone(),
            grammar: applied.grammar.clone(),
            grammar_lazy: applied.grammar_lazy,
            chat_format: applied.format_name().unwrap_or_default(),
        })
    }

    /// Token ids for `text`, with no BOS and no special-token parsing.
    pub fn tokenize(&self, text: &str) -> Result<Vec<u32>, String> {
        self.inner
            .model
            .str_to_token(text, AddBos::Never)
            .map(|tokens| tokens.into_iter().map(|t| t.0 as u32).collect())
            .map_err(|e| format!("Could not tokenize: {e}"))
    }

    /// Generate a response, streaming each `TokenUpdate` to `callback`.
    ///
    /// Decoding runs on a blocking thread (the llama.cpp context is not
    /// `Send`), and updates come back over a channel. If `callback` fails -
    /// the gRPC stream closed - the channel closes and decoding stops at its
    /// next token.
    pub async fn generate_streaming<F>(
        &self,
        turns: &[ChatTurn],
        options: &GenerationOptions,
        cancel: &CancellationToken,
        mut callback: F,
    ) -> Result<GenerationOutcome, String>
    where
        F: FnMut(TokenUpdate) -> Result<(), String>,
    {
        if turns.is_empty() {
            return Err("No messages in chat request".to_string());
        }

        // Everything that can reject the request does so here, before a
        // single token is generated.
        let inputs = ChatInputs::from_request(turns, options)?;
        let applied = self.inner.apply(&inputs)?;
        let plan = generate::Plan::new(&self.inner, &inputs, applied, options)?;

        eprintln!(
            "Generating streaming response for {} message(s)",
            turns.len()
        );

        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let inner = self.inner.clone();
        let cancel = cancel.clone();
        let task = tokio::task::spawn_blocking(move || generate::run(&inner, &plan, &cancel, &tx));

        while let Some(update) = rx.recv().await {
            if let Err(error) = callback(update) {
                drop(rx);
                let _ = task.await;
                return Err(error);
            }
        }
        task.await
            .map_err(|e| format!("Generation panicked: {e}"))?
    }
}

impl Inner {
    fn apply(&self, inputs: &ChatInputs) -> Result<ChatParams, String> {
        let params = inputs.apply_params(self.model.add_bos_token(), self.model.add_eos_token());
        self.templates
            .apply(&params)
            .map_err(|e| format!("The chat template could not render this request: {e}"))
    }
}

fn load_blocking(path: &Path, options: LoadOptions) -> Result<Model, String> {
    eprintln!("Loading GGUF model from: {:?}", path);
    let backend = backend();
    take_last_error();

    let params = LlamaModelParams::default().with_n_gpu_layers(options.n_gpu_layers);
    let model = LlamaModel::load_from_file(backend, path, &params).map_err(|e| {
        let reason = take_last_error();
        if reason.is_empty() {
            format!("Failed to load model: {e}")
        } else {
            format!("Failed to load model: {reason}")
        }
    })?;

    let n_ctx_train = model.n_ctx_train();
    let n_ctx = if n_ctx_train > 0 {
        options.n_ctx.min(n_ctx_train)
    } else {
        options.n_ctx
    };

    let templates = ChatTemplates::from_model(&model, None)
        .map_err(|e| format!("Failed to load model: it has no usable chat template: {e}"))?;

    let mut inner = Inner {
        info: ModelInfo::default(),
        model,
        templates,
        n_ctx,
    };
    inner.info = describe(&inner);
    eprintln!("Model loaded successfully! {:?}", inner.info);
    Ok(Model {
        inner: Arc::new(inner),
    })
}

/// What the model can do, probed once at load.
fn describe(inner: &Inner) -> ModelInfo {
    let probe = |tools: bool| {
        let mut options = GenerationOptions::default();
        if tools {
            options.tools = Some(
                r#"[{"type":"function","function":{"name":"probe","parameters":{"type":"object","properties":{}}}}]"#
                    .to_string(),
            );
        }
        ChatInputs::from_request(&[ChatTurn::user("hi")], &options)
            .and_then(|inputs| inner.apply(&inputs))
    };
    let caps: serde_json::Value = inner
        .templates
        .caps_json()
        .ok()
        .and_then(|caps| serde_json::from_str(&caps).ok())
        .unwrap_or_default();
    let with_tools = probe(true);
    let plain = probe(false);

    ModelInfo {
        engine: "llama.cpp",
        architecture: inner.model.meta_val_str("general.architecture", 128).ok(),
        n_ctx: Some(inner.n_ctx),
        n_vocab: Some(inner.model.n_vocab() as u32),
        chat_format: with_tools
            .as_ref()
            .or(plain.as_ref())
            .ok()
            .and_then(|applied| applied.format_name().ok()),
        // The template must accept tools *and* llama.cpp must have a parser
        // for its output; a template it cannot parse fails `apply`.
        supports_tools: caps["supports_tool_calls"].as_bool().unwrap_or(false)
            && with_tools.is_ok(),
        supports_thinking: plain.map(|a| a.supports_thinking).unwrap_or(false)
            || inner.templates.supports_enable_thinking(),
    }
}
