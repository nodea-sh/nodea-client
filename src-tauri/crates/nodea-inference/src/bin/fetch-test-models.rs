//! Download and verify the pinned test GGUFs (tests/models.toml).
//!
//!   cargo run -p nodea-inference --bin fetch-test-models -- [--only smoke,tools]
//!
//! Uses the production download path, so that is exercised too. A file that
//! is already present and verified is left alone; one that fails verification
//! is deleted, so a re-run starts clean.

use nodea_inference::download::{download_model_to, DownloadTimeouts};
use nodea_inference::test_models::{self, models_dir, verify};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let only: Option<Vec<String>> = match args.as_slice() {
        [] => None,
        [flag, list] if flag == "--only" => Some(list.split(',').map(str::to_string).collect()),
        _ => {
            eprintln!("usage: fetch-test-models [--only smoke,tools,lfm2,gemma3]");
            std::process::exit(2);
        }
    };

    let manifest = test_models::manifest();
    if let Some(names) = &only {
        for name in names {
            if !manifest.contains_key(name) {
                eprintln!("unknown test model {name:?}; known: {:?}", manifest.keys());
                std::process::exit(2);
            }
        }
    }

    let dir = models_dir();
    std::fs::create_dir_all(&dir).expect("cannot create the test model directory");
    println!("test models directory: {}", dir.display());

    // The default 15-minute total is for an app download with a UI behind it;
    // a slow CI runner fetching 800 MB needs longer.
    let timeouts = DownloadTimeouts {
        total: Duration::from_secs(3600),
        ..DownloadTimeouts::default()
    };

    let mut failed = false;
    for (name, model) in &manifest {
        if only.as_ref().is_some_and(|names| !names.contains(name)) {
            continue;
        }
        let path = model.path();

        if path.exists() {
            match verify(&path, model) {
                Ok(()) => {
                    println!("{name}: ok ({})", model.file);
                    continue;
                }
                Err(e) => {
                    println!("{name}: {e}; downloading again");
                    std::fs::remove_file(&path).ok();
                }
            }
        }

        println!("{name}: downloading {}", model.url());
        let result = download_model_to(&dir, &model.url(), &model.file, timeouts, |_| {})
            .await
            .and_then(|path| verify(&path, model));
        match result {
            Ok(()) => println!("{name}: ok ({})", model.file),
            Err(e) => {
                eprintln!("{name}: FAILED: {e}");
                std::fs::remove_file(&path).ok();
                failed = true;
            }
        }
    }

    if failed {
        std::process::exit(1);
    }
}
