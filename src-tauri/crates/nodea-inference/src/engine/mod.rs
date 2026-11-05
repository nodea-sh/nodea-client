//! The inference engine: llama.cpp, through `llama-cpp-4`.
//!
//! `Model` is the one public engine type:
//!
//! - `Model::load(path, LoadOptions) -> Result<Model, String>` (async)
//! - `Model::info() -> ModelInfo`
//! - `Model::render_prompt(turns, options) -> Result<RenderedPrompt, String>`
//! - `Model::tokenize(text) -> Result<Vec<u32>, String>`
//! - `Model::generate_streaming(turns, options, cancel, callback)` (async)
//!
//! A future engine implements the same API under `engine/` and has to pass
//! the same contract suite (tests/contract_*.rs).

pub mod llama;
pub use self::llama::Model;
