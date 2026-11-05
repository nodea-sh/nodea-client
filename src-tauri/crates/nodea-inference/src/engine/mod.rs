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

#[cfg(not(feature = "engine-mistralrs"))]
compile_error!("enable the `engine-mistralrs` feature");

#[cfg(feature = "engine-mistralrs")]
pub mod mistralrs;
#[cfg(feature = "engine-mistralrs")]
pub use self::mistralrs::{web_search_enabled, Model};
