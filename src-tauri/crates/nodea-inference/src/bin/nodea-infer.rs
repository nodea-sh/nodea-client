//! Run one `ChatCompletionRequest` through the engine and print exactly the
//! `GeneratedText` messages the node would send (notes/llama-cpp-4-migration.md §8.4).
//!
//!   cargo run -p nodea-inference --bin nodea-infer -- \
//!     --model smoke|tools|lfm2|gemma3|/path/to/file.gguf \
//!     --request request.json | --prompt "text" \
//!     [--n-ctx 4096] [--gpu-layers N] [--format jsonl|pretty] [--show-prompt] [--bench]
//!
//! No Tauri, no gRPC server, no GPU required (`--gpu-layers 0`).

use nodea_inference::proto::GeneratedText;
use nodea_inference::{request, test_models, wire, LoadOptions, Model};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const USAGE: &str =
    "usage: nodea-infer --model <name|path.gguf> (--request <file.json> | --prompt <text>)
                   [--n-ctx N] [--gpu-layers N] [--format jsonl|pretty] [--pretty]
                   [--show-prompt] [--bench]

  --model        a test model name from tests/models.toml (smoke, tools, lfm2, gemma3)
                 or a path to a GGUF
  --request      a ChatCompletionRequest as JSON, proto field names; tools, tool_choice,
                 response_format and tool_calls may be inline JSON
  --prompt       shortcut for one user message
  --format       jsonl (default): one GeneratedText per line, as on the wire
                 pretty: the text as it streams, then a summary per choice
  --show-prompt  print ModelInfo and the rendered prompt and grammar first
  --bench        print load time, time to first token and tokens/s";

#[derive(PartialEq)]
enum Format {
    Jsonl,
    Pretty,
}

struct Args {
    model: String,
    request: Option<PathBuf>,
    prompt: Option<String>,
    load: LoadOptions,
    format: Format,
    show_prompt: bool,
    bench: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        model: String::new(),
        request: None,
        prompt: None,
        load: LoadOptions {
            n_ctx: 4096,
            ..LoadOptions::default()
        },
        format: Format::Jsonl,
        show_prompt: false,
        bench: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--model" => args.model = value()?,
            "--request" => args.request = Some(PathBuf::from(value()?)),
            "--prompt" => args.prompt = Some(value()?),
            "--n-ctx" => args.load.n_ctx = value()?.parse().map_err(|e| format!("--n-ctx: {e}"))?,
            "--gpu-layers" => {
                args.load.n_gpu_layers =
                    value()?.parse().map_err(|e| format!("--gpu-layers: {e}"))?
            }
            "--format" => {
                args.format = match value()?.as_str() {
                    "jsonl" => Format::Jsonl,
                    "pretty" => Format::Pretty,
                    other => return Err(format!("unknown format {other:?}")),
                }
            }
            "--pretty" => args.format = Format::Pretty,
            "--show-prompt" => args.show_prompt = true,
            "--bench" => args.bench = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    if args.model.is_empty() {
        return Err("--model is required".to_string());
    }
    if args.request.is_some() == args.prompt.is_some() {
        return Err("pass exactly one of --request and --prompt".to_string());
    }
    Ok(args)
}

fn model_path(model: &str) -> Result<PathBuf, String> {
    if let Some(pinned) = test_models::get(model) {
        let path = pinned.path();
        if !path.exists() {
            return Err(format!(
                "{} is not fetched. Run:\n  cargo run -p nodea-inference --bin fetch-test-models -- --only {model}",
                path.display()
            ));
        }
        return Ok(path);
    }
    let path = PathBuf::from(model);
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("{model} is neither a test model name nor a file"))
    }
}

#[derive(Default)]
struct ChoiceSummary {
    text_chunks: usize,
    logprob_chunks: usize,
    finish_reason: Option<String>,
    tool_calls: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            if !error.is_empty() {
                eprintln!("{error}\n");
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    if let Err(error) = run(args).await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), String> {
    let json = match (&args.request, &args.prompt) {
        (Some(path), _) => {
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?
        }
        (None, Some(prompt)) => serde_json::json!({
            "messages": [{"role": "user", "content": prompt}]
        })
        .to_string(),
        (None, None) => unreachable!("checked in parse_args"),
    };
    let chat_request = request::from_json(&json)?;
    let turns = request::chat_turns(&chat_request);
    let options = request::generation_options(&chat_request);

    let path = model_path(&args.model)?;
    let load_start = Instant::now();
    let model = Model::load(&path, args.load).await?;
    let load_time = load_start.elapsed();

    if args.show_prompt {
        eprintln!("--- model info");
        eprintln!("{}", serde_json::to_string_pretty(&model.info()).unwrap());
        match model.render_prompt(&turns, &options) {
            Ok(rendered) => {
                eprintln!("--- chat format: {}", rendered.chat_format);
                eprintln!("--- prompt\n{}", rendered.prompt);
                if !rendered.grammar.is_empty() {
                    eprintln!(
                        "--- grammar ({})\n{}",
                        if rendered.grammar_lazy {
                            "lazy"
                        } else {
                            "eager"
                        },
                        rendered.grammar
                    );
                }
            }
            Err(e) => eprintln!("--- prompt unavailable: {e}"),
        }
        eprintln!("--- output");
    }

    let gen_start = Instant::now();
    let mut first_token: Option<Duration> = None;
    let mut summaries: BTreeMap<u32, ChoiceSummary> = BTreeMap::new();
    let mut final_message: Option<GeneratedText> = None;
    let format = &args.format;

    let outcome = model
        .generate_streaming(&turns, &options, &CancellationToken::new(), |update| {
            let message = wire::generated_text("nodea-infer", &update);
            if !message.text.is_empty() && first_token.is_none() {
                first_token = Some(gen_start.elapsed());
            }
            let choice = message.choice_index.unwrap_or(0);
            let summary = summaries.entry(choice).or_default();
            summary.text_chunks += usize::from(!message.text.is_empty());
            summary.logprob_chunks += usize::from(message.logprobs.is_some());
            if message.finish_reason.is_some() {
                summary.finish_reason = message.finish_reason.clone();
                summary.tool_calls = message.tool_calls.clone();
            }

            match format {
                Format::Jsonl => println!("{}", serde_json::to_string(&message).unwrap()),
                Format::Pretty => {
                    print!("{}", message.text);
                    std::io::stdout().flush().ok();
                }
            }
            if message.last {
                final_message = Some(message);
            }
            Ok(())
        })
        .await;
    let gen_time = gen_start.elapsed();

    if let Err(error) = &outcome {
        let message = wire::generation_error("nodea-infer", error);
        match format {
            Format::Jsonl => println!("{}", serde_json::to_string(&message).unwrap()),
            Format::Pretty => eprintln!("\n--- error: {error}"),
        }
    }

    if *format == Format::Pretty {
        println!();
        for (choice, summary) in &summaries {
            println!(
                "--- choice {choice}: finish_reason {:?}, {} text chunks, {} logprobs chunks",
                summary.finish_reason.as_deref().unwrap_or("-"),
                summary.text_chunks,
                summary.logprob_chunks
            );
            if let Some(calls) = &summary.tool_calls {
                println!("    tool_calls {calls}");
            }
        }
        if let Some(usage) = final_message.as_ref().and_then(|m| m.usage.clone()) {
            println!(
                "--- usage: {} prompt, {} completion tokens",
                usage.prompt_tokens, usage.completion_tokens
            );
        }
    }

    if args.bench {
        let completion = final_message
            .as_ref()
            .and_then(|m| m.usage.clone())
            .map(|u| u.completion_tokens)
            .unwrap_or(0);
        eprintln!(
            "--- bench: load {:.2}s, first token {}, {} tokens in {:.2}s = {:.1} tokens/s",
            load_time.as_secs_f64(),
            first_token
                .map(|t| format!("{:.0}ms", t.as_secs_f64() * 1000.0))
                .unwrap_or_else(|| "-".to_string()),
            completion,
            gen_time.as_secs_f64(),
            completion as f64 / gen_time.as_secs_f64()
        );
    }

    outcome.map(|_| ())
}
