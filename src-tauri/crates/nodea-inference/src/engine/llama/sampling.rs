//! The sampler chain (notes/llama-cpp-4-migration.md §5.2).
//!
//! `SamplerConfig` is the pure decision, built and validated before any
//! generation starts, so a bad `logit_bias` fails the request instead of a
//! half-finished stream. `build` turns it into llama.cpp samplers.

use crate::types::{GenerationOptions, DEFAULT_TEMPERATURE};
use llama_cpp_4::sampling::LlamaSampler;
use llama_cpp_4::token::LlamaToken;
use std::hash::{BuildHasher, Hasher};

/// How far back the frequency and presence penalties look.
const PENALTY_LAST_N: i32 = 64;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct SamplerConfig {
    /// Temperature <= 0 is greedy in the OpenAI API.
    pub greedy: bool,
    pub temperature: f32,
    pub top_p: Option<f32>,
    /// (frequency, presence), only when either was sent.
    pub penalties: Option<(f32, f32)>,
    pub logit_bias: Vec<(u32, f32)>,
    /// The seed for choice 0; choice `i` uses `seed + i`.
    pub seed: u32,
}

pub(super) fn config(options: &GenerationOptions, n_vocab: u32) -> Result<SamplerConfig, String> {
    let temperature = options.temperature.unwrap_or(DEFAULT_TEMPERATURE);

    let mut logit_bias: Vec<(u32, f32)> = options
        .logit_bias
        .iter()
        .map(|(&id, &bias)| (id, bias))
        .collect();
    logit_bias.sort_by_key(|(id, _)| *id);
    if let Some((id, _)) = logit_bias.iter().find(|(id, _)| *id >= n_vocab) {
        return Err(format!(
            "logit_bias token id {id} is outside this model's vocabulary of {n_vocab} tokens"
        ));
    }

    let penalties = (options.frequency_penalty.is_some() || options.presence_penalty.is_some())
        .then(|| {
            (
                options.frequency_penalty.unwrap_or(0.0),
                options.presence_penalty.unwrap_or(0.0),
            )
        });

    Ok(SamplerConfig {
        greedy: temperature <= 0.0,
        temperature: temperature as f32,
        top_p: options.top_p.map(|p| p as f32),
        penalties,
        logit_bias,
        seed: options.seed.map(fold_seed).unwrap_or_else(random_seed),
    })
}

/// The proto's int64 seed, folded into llama.cpp's u32 so that seeds differing
/// only in their high bits still differ.
pub(super) fn fold_seed(seed: i64) -> u32 {
    let seed = seed as u64;
    (seed ^ (seed >> 32)) as u32
}

fn random_seed() -> u32 {
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish() as u32
}

/// The stage names `build` will chain, in order. What the L0 tests check.
pub(super) fn stages(config: &SamplerConfig, has_grammar: bool) -> Vec<&'static str> {
    let mut stages = Vec::new();
    if !config.logit_bias.is_empty() {
        stages.push("logit_bias");
    }
    if config.penalties.is_some() {
        stages.push("penalties");
    }
    if has_grammar {
        stages.push("grammar");
    }
    if config.greedy {
        stages.push("greedy");
    } else {
        if config.top_p.is_some() {
            stages.push("top_p");
        }
        stages.push("temp");
        stages.push("dist");
    }
    stages
}

/// The chain for one choice. `grammar` comes from `ChatParams::grammar_sampler`.
pub(super) fn build(
    config: &SamplerConfig,
    choice: usize,
    n_vocab: i32,
    mut grammar: Option<LlamaSampler>,
) -> LlamaSampler {
    let mut chain = Vec::new();
    for stage in stages(config, grammar.is_some()) {
        match stage {
            "logit_bias" => {
                let biases: Vec<(LlamaToken, f32)> = config
                    .logit_bias
                    .iter()
                    .map(|&(id, bias)| (LlamaToken(id as i32), bias))
                    .collect();
                chain.push(LlamaSampler::logit_bias(n_vocab, &biases));
            }
            "penalties" => {
                let (frequency, presence) = config.penalties.unwrap();
                chain.push(LlamaSampler::penalties(
                    n_vocab,
                    PENALTY_LAST_N,
                    1.0,
                    frequency,
                    presence,
                ));
            }
            "grammar" => chain.extend(grammar.take()),
            "greedy" => chain.push(LlamaSampler::greedy()),
            "top_p" => chain.push(LlamaSampler::top_p(config.top_p.unwrap(), 1)),
            "temp" => chain.push(LlamaSampler::temp(config.temperature)),
            "dist" => chain.push(LlamaSampler::dist(config.seed.wrapping_add(choice as u32))),
            other => unreachable!("unknown sampler stage {other}"),
        }
    }
    LlamaSampler::chain_simple(chain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn options() -> GenerationOptions {
        GenerationOptions::default()
    }

    #[test]
    fn defaults_sample_at_openai_temperature() {
        let config = config(&options(), 1000).unwrap();
        assert!(!config.greedy);
        assert_eq!(config.temperature, DEFAULT_TEMPERATURE as f32);
        assert_eq!(stages(&config, false), vec!["temp", "dist"]);
    }

    #[test]
    fn zero_temperature_is_greedy_and_drops_temp_and_dist() {
        let config = config(&GenerationOptions::greedy(10), 1000).unwrap();
        assert!(config.greedy);
        assert_eq!(stages(&config, false), vec!["greedy"]);
    }

    #[test]
    fn the_chain_order_is_bias_penalties_grammar_top_p_temp_dist() {
        let config = config(
            &GenerationOptions {
                temperature: Some(0.7),
                top_p: Some(0.9),
                frequency_penalty: Some(0.5),
                logit_bias: HashMap::from([(5, -100.0)]),
                ..options()
            },
            1000,
        )
        .unwrap();

        assert_eq!(
            stages(&config, true),
            vec![
                "logit_bias",
                "penalties",
                "grammar",
                "top_p",
                "temp",
                "dist"
            ]
        );
        assert_eq!(config.penalties, Some((0.5, 0.0)));
        assert_eq!(config.top_p, Some(0.9));
    }

    #[test]
    fn penalties_are_added_only_when_sent() {
        assert_eq!(config(&options(), 1000).unwrap().penalties, None);
        let presence = config(
            &GenerationOptions {
                presence_penalty: Some(-0.5),
                ..options()
            },
            1000,
        )
        .unwrap();
        assert_eq!(presence.penalties, Some((0.0, -0.5)));
    }

    #[test]
    fn an_out_of_vocabulary_bias_is_an_error() {
        let error = config(
            &GenerationOptions {
                logit_bias: HashMap::from([(1000, 1.0)]),
                ..options()
            },
            1000,
        )
        .unwrap_err();
        assert!(error.contains("1000"), "{error}");
        assert!(config(
            &GenerationOptions {
                logit_bias: HashMap::from([(999, 1.0)]),
                ..options()
            },
            1000
        )
        .is_ok());
    }

    #[test]
    fn a_seed_is_honoured_and_folded_into_u32() {
        let config = config(
            &GenerationOptions {
                seed: Some(42),
                ..options()
            },
            1000,
        )
        .unwrap();
        assert_eq!(config.seed, 42);
        // Seeds that differ only in their high 32 bits must still differ.
        assert_ne!(fold_seed(1), fold_seed(1 | (1 << 40)));
        assert_eq!(fold_seed(-1), 0);
    }
}
