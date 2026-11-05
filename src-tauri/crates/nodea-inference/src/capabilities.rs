//! The capability strings this build advertises in `DeviceIdentity`.
//!
//! They live beside the engine because nearly all of them are claims about it,
//! and because tests/contract_capabilities.rs checks that each one has a
//! contract test with a real model behind it.

/// What this build can do, sent in the DeviceIdentity handshake and matched
/// exactly by the backend.
///
/// This, not `app_version`, is the feature-gating mechanism: every build
/// reports version 0.1.0 and nothing bumps it, so a version compare would be a
/// no-op by construction. A capability is added here in the same commit that
/// makes it true, and the backend rejects a request needing one this list does
/// not contain rather than silently answering as if the field had not been
/// sent.
pub fn node_capabilities() -> Vec<String> {
    let mut capabilities = vec![
        "structured_messages".to_string(),
        "terminal_metadata".to_string(),
        "response_format".to_string(),
        "tools".to_string(),
        "logprobs".to_string(),
        "n_choices".to_string(),
        "logit_bias".to_string(),
        "reasoning_effort".to_string(),
        "cancel".to_string(),
        // Switches models on a LoadModel without leaving service for the
        // download, reports each step as a ModelLoadStatus, and reports what it
        // has loaded and cached in DeviceIdentity - so the backend may
        // reassign it after connect. Nodes without it keep connect-time-only
        // assignment.
        "scheduler_v1".to_string(),
    ];

    // Web search needs an embedding model download and network egress at load
    // time, so it is a deployment decision rather than something every node
    // can do. Only claim it when the node was actually started with it on.
    if crate::web_search_enabled() {
        capabilities.push("web_search".to_string());
    }

    capabilities
}

#[cfg(test)]
mod tests {
    use super::node_capabilities;

    #[test]
    fn capabilities_claim_what_this_build_can_actually_do() {
        let capabilities = node_capabilities();

        // Each is implemented in this build; the backend rejects requests
        // needing a capability the node does not list, so a missing entry
        // silently disables a working feature.
        for expected in [
            "structured_messages",
            "terminal_metadata",
            "response_format",
            "tools",
            "logprobs",
            "n_choices",
            "logit_bias",
            "reasoning_effort",
            "cancel",
            "scheduler_v1",
        ] {
            assert!(
                capabilities.contains(&expected.to_string()),
                "{expected} is implemented but not advertised",
            );
        }
    }

    #[test]
    fn web_search_is_not_claimed_unless_it_was_enabled() {
        // It needs an embedding model download and network egress at load
        // time, so claiming it unconditionally would make the backend route
        // search requests to a node that cannot serve them.
        if !crate::web_search_enabled() {
            assert!(!node_capabilities().contains(&"web_search".to_string()));
        }
    }
}
