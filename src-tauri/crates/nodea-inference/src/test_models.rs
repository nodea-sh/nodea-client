//! The pinned GGUFs the contract suite runs against (`tests/models.toml`),
//! where they live on disk, and how to check one is the file we pinned.
//!
//! Library code rather than test-only code because three things share it:
//! `fetch-test-models`, `nodea-infer --model <name>` and `tests/support`.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const MANIFEST: &str = include_str!("../tests/models.toml");

#[derive(Clone, Debug, serde::Deserialize)]
pub struct TestModel {
    pub repo: String,
    pub revision: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub arch: String,
}

impl TestModel {
    /// The Hugging Face download URL, pinned to a revision.
    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, self.file
        )
    }

    pub fn path(&self) -> PathBuf {
        models_dir().join(&self.file)
    }
}

/// Every pinned model, by name (`smoke`, `tools`, ...).
pub fn manifest() -> BTreeMap<String, TestModel> {
    toml::from_str(MANIFEST).expect("tests/models.toml is not valid")
}

pub fn get(name: &str) -> Option<TestModel> {
    manifest().remove(name)
}

/// `$NODEA_TEST_MODELS_DIR`, or `<repo root>/.cache/test-models`.
pub fn models_dir() -> PathBuf {
    match std::env::var_os("NODEA_TEST_MODELS_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.cache/test-models"),
    }
}

/// Ok when `path` has the pinned size and sha256.
pub fn verify(path: &Path, model: &TestModel) -> Result<(), String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("{}: {}", path.display(), e))?
        .len();
    if size != model.size {
        return Err(format!(
            "{}: size {} but {} was pinned",
            path.display(),
            size,
            model.size
        ));
    }

    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if digest != model.sha256 {
        return Err(format!(
            "{}: sha256 {} but {} was pinned",
            path.display(),
            digest,
            model.sha256
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_pins_every_model_the_suite_names() {
        let manifest = manifest();
        for name in ["smoke", "tools", "lfm2", "gemma3"] {
            let model = &manifest[name];
            assert!(model.file.ends_with(".gguf"), "{name}");
            assert_eq!(model.sha256.len(), 64, "{name}");
            assert_eq!(
                model.revision.len(),
                40,
                "{name}: pin a commit, not a branch"
            );
        }
    }

    #[test]
    fn urls_are_pinned_to_the_revision() {
        let smoke = get("smoke").unwrap();
        assert!(smoke
            .url()
            .contains(&format!("/resolve/{}/", smoke.revision)));
    }

    #[test]
    fn verify_rejects_a_file_of_the_wrong_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.gguf");
        std::fs::write(&path, b"short").unwrap();

        assert!(verify(&path, &get("smoke").unwrap()).is_err());
    }
}
