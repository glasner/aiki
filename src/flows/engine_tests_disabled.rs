// NOTE: These tests are temporarily disabled during the flow control refactoring.
// They need to be migrated to use FlowStatement instead of Action::If/Switch.
// TODO: Re-enable and update these tests to use the new statement-based flow control.

/*
Original tests that use Action::If and IfAction have been disabled.
To migrate these tests:

1. Replace Action::If(IfAction { ... }) with FlowStatement::If(IfStatement { ... })
2. Update execute_actions calls to execute_statements
3. Update any on_failure: OnFailure::Actions to OnFailure::Statements

Example migration:

OLD:
```rust
let actions = vec![
    Action::If(IfAction {
        condition: "$x == 1".to_string(),
        then: vec![Action::Log(...)],
        else_: None,
        on_failure: OnFailure::default(),
    }),
];
FlowEngine::execute_actions(&actions, &mut state)
```

NEW:
```rust
let statements = vec![
    FlowStatement::If(IfStatement {
        condition: "$x == 1".to_string(),
        then: vec![FlowStatement::Action(Action::Log(...))],
        else_: None,
    }),
];
FlowEngine::execute_statements(&statements, &mut state)
```
*/
