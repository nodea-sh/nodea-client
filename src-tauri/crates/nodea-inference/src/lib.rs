//! Everything model-related on the node: loading a GGUF, generating, and
//! mapping between the proto and the engine. No Tauri, no auth, no `env!()`,
//! so it builds, tests and runs from the command line on its own (see
//! `src/bin/nodea-infer.rs` and notes/llama-cpp-4-migration.md §8).

pub mod capabilities;
pub mod download;
pub mod engine;
pub mod memory;
pub mod request;
pub mod test_models;
pub mod types;
pub mod wire;

pub use capabilities::node_capabilities;
pub use engine::Model;
pub use nodea_proto as proto;
pub use types::*;
