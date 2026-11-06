//! The app's one model slot. All inference lives in `nodea_inference`; this
//! only owns "which model is loaded, from where" and the cache directory, so
//! `SharedModelManager`, the `ModelSlot` impl in reload.rs and the gRPC
//! handlers keep their shapes.

use nodea_inference::Model;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub use nodea_inference::download::{download_model_to, DownloadProgress, DownloadTimeouts};
pub use nodea_inference::{
    describe_prompt, ChatTurn, GenerationOptions, GenerationOutcome, GenerationUsage, LoadOptions,
    TokenUpdate, DEFAULT_MAX_TOKENS, DEFAULT_SYSTEM_PROMPT, DEFAULT_TEMPERATURE,
};

pub struct ModelManager {
    model: Option<Model>,
    model_path: Option<PathBuf>,
    cache_dir: PathBuf,
}

impl ModelManager {
    pub fn new(cache_dir: PathBuf) -> Self {
        // Create cache directory if it doesn't exist
        std::fs::create_dir_all(&cache_dir).ok();

        Self {
            model: None,
            model_path: None,
            cache_dir,
        }
    }

    /// Load a model from a local path
    pub async fn load_model(&mut self, model_path: impl AsRef<Path>) -> Result<(), String> {
        let path = model_path.as_ref();

        // Free the resident model before building the next: building while
        // it is still loaded peaks both at once, which is what "This model
        // does not fit on the devices" was.
        self.unload();

        let model = Model::load(path, LoadOptions::default()).await?;

        self.model = Some(model);
        self.model_path = Some(path.to_path_buf());
        Ok(())
    }

    /// Generate a reply to a single prompt, greedily.
    pub async fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String, String> {
        println!("{}", describe_prompt(prompt, max_tokens));

        let mut text = String::new();
        self.generate_streaming(
            &[ChatTurn::user(prompt)],
            &GenerationOptions::greedy(max_tokens),
            &CancellationToken::new(),
            |update| {
                text.push_str(&update.text);
                Ok(())
            },
        )
        .await?;

        println!("Generation complete!");
        Ok(text)
    }

    /// Generate a response to a conversation with a streaming callback.
    /// The callback should return Ok(()) to continue or Err to stop.
    pub async fn generate_streaming<F>(
        &self,
        turns: &[ChatTurn],
        options: &GenerationOptions,
        cancel: &CancellationToken,
        callback: F,
    ) -> Result<GenerationOutcome, String>
    where
        F: FnMut(TokenUpdate) -> Result<(), String> + Send,
    {
        let model = self.model.as_ref().ok_or("Model not loaded")?;
        model
            .generate_streaming(turns, options, cancel, callback)
            .await
    }

    pub fn is_loaded(&self) -> bool {
        self.model.is_some()
    }

    /// Drop the resident model, freeing its memory.
    pub fn unload(&mut self) {
        self.model = None;
        self.model_path = None;
    }

    /// The file the resident model was loaded from; None when nothing is loaded.
    pub fn loaded_model_path(&self) -> Option<&Path> {
        self.model.as_ref().and(self.model_path.as_deref())
    }

    /// Whether `model_name` is already loaded in memory - guards against
    /// reloading (and briefly double-allocating GPU/Metal memory for) a
    /// model the server asks for again on every fresh connection, even
    /// though pausing/resuming sharing never unloads it (by design: the
    /// model stays warm so resuming is instant).
    pub fn is_model_loaded_and_matches(&self, model_name: &str) -> bool {
        self.model.is_some()
            && self
                .model_path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|f| f == std::ffi::OsStr::new(model_name))
                .unwrap_or(false)
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Get the local path where a model would be cached
    pub fn get_cached_model_path(&self, model_name: &str) -> PathBuf {
        self.cache_dir.join(model_name)
    }

    /// Check if a model exists in cache
    pub fn is_model_cached(&self, model_name: &str) -> bool {
        let path = self.get_cached_model_path(model_name);
        path.exists()
    }

    /// Download a model from a URL to the cache directory
    pub async fn download_model<F>(
        &self,
        url: &str,
        model_name: &str,
        progress_callback: F,
    ) -> Result<PathBuf, String>
    where
        F: Fn(DownloadProgress) + Send + 'static,
    {
        self.download_model_with_timeouts(
            url,
            model_name,
            DownloadTimeouts::default(),
            progress_callback,
        )
        .await
    }

    pub async fn download_model_with_timeouts<F>(
        &self,
        url: &str,
        model_name: &str,
        timeouts: DownloadTimeouts,
        progress_callback: F,
    ) -> Result<PathBuf, String>
    where
        F: Fn(DownloadProgress) + Send + 'static,
    {
        download_model_to(
            &self.cache_dir,
            url,
            model_name,
            timeouts,
            progress_callback,
        )
        .await
    }
}

// Thread-safe global model manager
pub type SharedModelManager = Arc<Mutex<ModelManager>>;

pub fn create_model_manager(cache_dir: PathBuf) -> SharedModelManager {
    Arc::new(Mutex::new(ModelManager::new(cache_dir)))
}

/// Free the loaded model before the process exits.
///
/// Quitting with a model still loaded aborts on macOS: `exit()` runs
/// llama.cpp's static destructors, which free the Metal device while the
/// model's buffers still hold GPU memory, and llama.cpp fails an assertion
/// (`GGML_ASSERT([rsets->data count] == 0)`) instead of exiting. Dropping the
/// model first releases that memory.
///
/// A generation holds the model_manager lock for its whole run, so in-flight
/// generations are cancelled first (they stop within one token); the lock is
/// then waited on for at most `lock_timeout`. Returns whether the model slot
/// was released - false means something kept the lock, e.g. a model still
/// loading.
pub async fn release_for_exit(
    model_manager: &SharedModelManager,
    generations: &crate::generations::Generations,
    lock_timeout: std::time::Duration,
) -> bool {
    generations.drain(std::time::Duration::ZERO).await;
    match tokio::time::timeout(lock_timeout, model_manager.lock()).await {
        Ok(mut manager) => {
            manager.unload();
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generations::Generations;
    use std::time::Duration;

    #[tokio::test]
    async fn release_for_exit_empties_the_slot() {
        let dir = tempfile::tempdir().unwrap();
        let shared = create_model_manager(dir.path().to_path_buf());
        shared.lock().await.model_path = Some(dir.path().join("model.gguf"));

        let released = release_for_exit(&shared, &Generations::new(), Duration::from_secs(1)).await;

        assert!(released);
        let manager = shared.lock().await;
        assert!(!manager.is_loaded());
        assert!(manager.model_path.is_none());
    }

    #[tokio::test]
    async fn release_for_exit_cancels_in_flight_generations_first() {
        // A generation holds the model_manager lock; without the cancel, exit
        // would wait the whole timeout and then quit with the model loaded.
        let dir = tempfile::tempdir().unwrap();
        let shared = create_model_manager(dir.path().to_path_buf());
        let generations = Arc::new(Generations::new());
        let cancel = generations.begin("req-1").await.unwrap();

        let generation = {
            let shared = shared.clone();
            let generations = generations.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                let _lock = shared.lock().await;
                cancel.cancelled().await;
                generations.finish("req-1").await;
            })
        };
        tokio::task::yield_now().await;

        let released = release_for_exit(&shared, &generations, Duration::from_secs(5)).await;

        assert!(cancel.is_cancelled());
        assert!(released);
        generation.await.unwrap();
    }

    #[tokio::test]
    async fn release_for_exit_gives_up_when_the_lock_stays_held() {
        // Exit must not hang on a lock it cannot get, e.g. a model mid-load.
        let dir = tempfile::tempdir().unwrap();
        let shared = create_model_manager(dir.path().to_path_buf());
        let _held = shared.lock().await;

        let released =
            release_for_exit(&shared, &Generations::new(), Duration::from_millis(50)).await;

        assert!(!released);
    }

    #[test]
    fn describe_prompt_carries_lengths_not_content() {
        let line = describe_prompt("my secret prompt", 64);
        assert!(!line.contains("secret"));
        assert!(line.contains("16 chars"));
        assert!(line.contains("max_tokens 64"));
    }

    #[test]
    fn unload_leaves_nothing_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = ModelManager::new(dir.path().to_path_buf());
        manager.model_path = Some(dir.path().join("model.gguf"));

        manager.unload();

        assert!(manager.model.is_none());
        assert!(!manager.is_loaded());
        assert!(manager.loaded_model_path().is_none());
        assert!(!manager.is_model_loaded_and_matches("model.gguf"));
    }

    #[tokio::test]
    async fn a_pre_cancelled_generation_does_not_even_load() {
        // Cheap guard that the cancel argument is threaded through at all: with
        // no model loaded this fails before the loop, but the signature and the
        // call site are what regress silently.
        let manager = ModelManager::new(std::env::temp_dir());
        let cancel = CancellationToken::new();
        cancel.cancel();

        let result = manager
            .generate_streaming(
                &[ChatTurn::user("hi")],
                &GenerationOptions::default(),
                &cancel,
                |_| Ok(()),
            )
            .await;

        assert!(
            result.is_err(),
            "no model is loaded, so this cannot succeed"
        );
    }

    #[test]
    fn a_cancelled_outcome_is_distinct_from_a_completed_one() {
        // handle_chat_completion branches on this: a cancellation is not a
        // failure, because the backend already gave up on the request, so
        // nothing should be reported back for it.
        assert_ne!(GenerationOutcome::Cancelled, GenerationOutcome::Completed);
    }
}
