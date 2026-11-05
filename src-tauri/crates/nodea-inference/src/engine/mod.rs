//! The one public engine type, `Model`, selected at compile time.
//!
//! There is no trait: a binary never needs two engines, so a Cargo feature
//! picks the implementation and every engine exposes the same inherent API:
//!
//! - `Model::load(path, LoadOptions) -> Result<Model, String>` (async)
//! - `Model::info() -> ModelInfo`
//! - `Model::render_prompt(turns, options) -> Result<RenderedPrompt, String>`
//! - `Model::tokenize(text) -> Result<Vec<u32>, String>`
//! - `Model::generate_streaming(turns, options, cancel, callback)` (async)

#[cfg(all(feature = "engine-mistralrs", feature = "engine-llama"))]
compile_error!(
    "enable exactly one of the `engine-mistralrs` and `engine-llama` features \
     (for llama: --no-default-features --features engine-llama)"
);

#[cfg(not(any(feature = "engine-mistralrs", feature = "engine-llama")))]
compile_error!("enable one of the `engine-mistralrs` and `engine-llama` features");

#[cfg(feature = "engine-mistralrs")]
pub mod mistralrs;
#[cfg(feature = "engine-mistralrs")]
pub use self::mistralrs::{web_search_enabled, Model};

#[cfg(feature = "engine-llama")]
pub mod llama;
#[cfg(feature = "engine-llama")]
pub use self::llama::{web_search_enabled, Model};
