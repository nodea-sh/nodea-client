//! Proto contract: logprobs.

mod support;

use support::{assert_invariants, model, req, run};

#[test]
fn logprobs_have_openai_shape() {
    let Some(model) = model("smoke") else { return };
    let request = req(r#"{"messages":[{"role":"user","content":"Say hello."}],
        "max_tokens":8,"temperature":0,"logprobs":true,"top_logprobs":3}"#);

    let run = run(&model, &request);

    assert_invariants(&request, &run);
    let chunks: Vec<serde_json::Value> = run
        .messages
        .iter()
        .filter(|m| !m.text.is_empty())
        .map(|m| serde_json::from_str(m.logprobs.as_deref().unwrap()).unwrap())
        .collect();
    assert!(!chunks.is_empty());

    for logprob in &chunks {
        let token = logprob["token"].as_str().expect("token is a string");
        let value = logprob["logprob"].as_f64().expect("logprob is a number");
        assert!(value <= 0.0, "{logprob}");
        let bytes: Vec<u8> = logprob["bytes"]
            .as_array()
            .expect("bytes is an array")
            .iter()
            .map(|b| b.as_u64().unwrap() as u8)
            .collect();
        assert_eq!(bytes, token.as_bytes(), "{logprob}");

        let top = logprob["top_logprobs"].as_array().expect("top_logprobs");
        assert_eq!(top.len(), 3, "{logprob}");
        let values: Vec<f64> = top.iter().map(|t| t["logprob"].as_f64().unwrap()).collect();
        assert!(
            values.windows(2).all(|w| w[0] >= w[1]),
            "descending: {values:?}"
        );
        assert!(values.iter().all(|v| *v <= 0.0), "{values:?}");
    }
}
