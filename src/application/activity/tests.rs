use super::*;

#[test]
fn cancellation_is_idempotent_and_updates_activity() {
    let activity = Activity::new();
    let token = CancellationToken::new();
    let operation = activity.begin(ActivityKind::Generate, "model", Some(token.clone()));
    assert!(activity.cancel(operation.id()).accepted);
    assert!(activity.cancel(operation.id()).accepted);
    assert!(token.is_cancelled());
    operation.finish(ActivityState::Cancelled, "cancelled by test");
    assert!(!activity.cancel(operation.id()).accepted);
}

#[test]
fn queued_operation_becomes_running_on_first_work_stage() {
    let activity = Activity::new();
    let operation = activity.enqueue(ActivityKind::Pull, "model", CancellationToken::new());
    assert_eq!(activity.history()[0].state, ActivityState::Queued);
    operation.progress(ActivityStage::Resolving, "resolving model", Some(0), None);
    assert_eq!(activity.history()[0].state, ActivityState::Running);
}

#[test]
fn history_limit_never_evicts_a_running_operation() {
    let activity = Activity::new();
    let oldest = activity.begin(ActivityKind::Generate, "oldest", Some(CancellationToken::new()));
    for index in 0..HISTORY_LIMIT {
        activity
            .begin(ActivityKind::Load, &index.to_string(), None)
            .finish(ActivityState::Completed, "done");
    }
    assert!(activity.cancel(oldest.id()).accepted);
}
