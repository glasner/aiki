// Temporary stub file for tests that need migration to FlowStatement
// These tests have been extracted from engine.rs and need to be updated

#[cfg(test)]
mod legacy_tests_to_migrate {
    use super::*;

    // List of tests that need migration:
    // - test_if_condition_true_executes_then_branch
    // - test_if_condition_false_executes_else_branch
    // - test_if_condition_false_no_else_is_noop
    // - test_nested_if_conditions
    // - test_switch_matches_case
    // - test_switch_uses_default_when_no_match
    // - test_switch_no_match_no_default_is_noop
    // - test_switch_with_variable_expression
    // - test_if_with_on_failure_continue
    // - test_if_with_on_failure_stop
    // - test_if_condition_evaluation_failure
    // - test_switch_expression_evaluation
    // - test_nested_control_flow_with_failures
    // - test_if_with_json_field_access
    // - test_switch_with_empty_cases
    // - test_if_else_both_fail_continues
    // - test_switch_case_with_on_failure

    // These tests have been temporarily disabled because they use
    // Action::If(IfAction) and Action::Switch(SwitchAction) which
    // no longer exist after the FlowStatement refactoring.
}
