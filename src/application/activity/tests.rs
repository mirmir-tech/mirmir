use super::*;

#[test]
fn cancellation_is_idempotent_and_updates_activity() {
    let activity = Activity::new();
    let token = CancellationToken::new();
    let operation = activity.begin("generate", "model", Some(token.clone()));
    assert!(activity.cancel(operation.id()).accepted);
    assert!(activity.cancel(operation.id()).accepted);
    assert!(token.is_cancelled());
    operation.finish("cancelled", "cancelled by test");
    assert!(!activity.cancel(operation.id()).accepted);
}

#[test]
fn queued_operation_becomes_running_on_first_work_stage() {
    let activity = Activity::new();
    let operation = activity.enqueue("pull", "model", CancellationToken::new());
    assert_eq!(activity.history()[0].state, "queued");
    operation.progress("resolving", "resolving model", Some(0), None);
    assert_eq!(activity.history()[0].state, "running");
}

#[test]
fn history_limit_never_evicts_a_running_operation() {
    let activity = Activity::new();
    let oldest = activity.begin("generate", "oldest", Some(CancellationToken::new()));
    for index in 0..HISTORY_LIMIT {
        activity.begin("load", &index.to_string(), None).finish("completed", "done");
    }
    assert!(activity.cancel(oldest.id()).accepted);
}
