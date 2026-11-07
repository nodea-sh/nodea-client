//! Log probabilities in OpenAI's shape, computed from the raw logits.
//!
//! "Raw" means before logit_bias, penalties and the grammar: OpenAI reports
//! the model's own distribution, not the one the sampler drew from. The values
//! are natural logs over the full vocabulary.

/// OpenAI's value for "so unlikely the real value is not representable";
/// serde_json would otherwise write -inf as `null` for a required number.
const UNREPRESENTABLE: f32 = -9999.0;

/// Most alternatives OpenAI allows.
pub(super) const MAX_TOP_LOGPROBS: usize = 20;

fn representable(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        UNREPRESENTABLE
    }
}

/// The sampled token's logprob, and the `top` most likely tokens with theirs,
/// most likely first.
pub(super) fn log_softmax_top(
    logits: &[f32],
    sampled: usize,
    top: usize,
) -> (f32, Vec<(usize, f32)>) {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let log_sum = logits.iter().map(|l| (l - max).exp()).sum::<f32>().ln() + max;
    let logprob = |i: usize| representable(logits[i] - log_sum);

    let top = top.min(MAX_TOP_LOGPROBS).min(logits.len());
    let mut best: Vec<usize> = Vec::with_capacity(top + 1);
    if top > 0 {
        for (i, &logit) in logits.iter().enumerate() {
            if best.len() == top && logit <= logits[*best.last().unwrap()] {
                continue;
            }
            let at = best.partition_point(|&j| logits[j] >= logit);
            best.insert(at, i);
            best.truncate(top);
        }
    }

    (
        logprob(sampled),
        best.into_iter().map(|i| (i, logprob(i))).collect(),
    )
}

fn entry(bytes: &[u8], logprob: f32) -> serde_json::Value {
    serde_json::json!({
        "token": String::from_utf8_lossy(bytes),
        "logprob": representable(logprob),
        "bytes": bytes,
    })
}

/// One ChatCompletionTokenLogprob object.
pub(super) fn openai_logprob(
    token_bytes: &[u8],
    logprob: f32,
    alternatives: &[(Vec<u8>, f32)],
) -> serde_json::Value {
    let mut value = entry(token_bytes, logprob);
    value["top_logprobs"] = alternatives
        .iter()
        .map(|(bytes, logprob)| entry(bytes, *logprob))
        .collect();
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_natural_log_probabilities() {
        // Two equally likely tokens: ln(0.5) each.
        let (sampled, _) = log_softmax_top(&[3.0, 3.0], 0, 0);
        assert!(
            (sampled - (-std::f32::consts::LN_2)).abs() < 1e-5,
            "{sampled}"
        );
    }

    #[test]
    fn alternatives_are_the_most_likely_in_descending_order() {
        let logits = [0.0, 5.0, 1.0, 4.0, 2.0];
        let (_, top) = log_softmax_top(&logits, 0, 3);

        assert_eq!(
            top.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![1, 3, 4]
        );
        assert!(top.windows(2).all(|w| w[0].1 >= w[1].1));
        assert!(top.iter().all(|(_, v)| *v <= 0.0));
    }

    #[test]
    fn at_most_twenty_alternatives() {
        let logits: Vec<f32> = (0..100).map(|i| i as f32).collect();
        assert_eq!(log_softmax_top(&logits, 0, 50).1.len(), MAX_TOP_LOGPROBS);
    }

    #[test]
    fn an_impossible_token_gets_openais_sentinel_not_null() {
        let (sampled, _) = log_softmax_top(&[0.0, f32::NEG_INFINITY], 1, 0);
        assert_eq!(sampled, UNREPRESENTABLE);

        let json = openai_logprob(b"x", sampled, &[(b"y".to_vec(), f32::NEG_INFINITY)]);
        assert!(!json.to_string().contains("null"), "{json}");
    }

    #[test]
    fn the_shape_is_openais() {
        let json = openai_logprob(b"Hi", -0.25, &[(b"Hey".to_vec(), -1.5)]);

        assert_eq!(json["token"], "Hi");
        assert_eq!(json["logprob"], -0.25);
        assert_eq!(json["bytes"], serde_json::json!([72, 105]));
        assert_eq!(json["top_logprobs"][0]["token"], "Hey");
        assert_eq!(
            json["top_logprobs"][0]["bytes"],
            serde_json::json!([72, 101, 121])
        );
    }

    #[test]
    fn a_partial_utf8_token_keeps_its_real_bytes() {
        let first = &"é".as_bytes()[..1];
        let json = openai_logprob(first, -1.0, &[]);
        assert_eq!(json["bytes"], serde_json::json!([0xc3]));
    }
}
