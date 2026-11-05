//! Every capability the node advertises has a contract test behind it.

mod support;

#[test]
fn capabilities_match_the_suite() {
    for capability in nodea_inference::node_capabilities() {
        let row = support::CAPABILITY_TESTS
            .iter()
            .find(|(name, _)| *name == capability);
        assert!(
            row.is_some_and(|(_, tests)| !tests.is_empty()),
            "{capability} is advertised but no contract test covers it; add one and a row to \
             tests/support CAPABILITY_TESTS"
        );
    }
}
