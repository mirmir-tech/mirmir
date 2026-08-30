use super::*;

#[test]
fn cancellation_is_idempotent_and_updates_activity() {
    let activity = Activity::new();
    let token = CancellationToken::new();
    let operation = activity.begin(ActivityKind::Generate, "model", Some(token.clone()));
    assert_eq!(activity.cancel(operation.id()), CancelOutcome::Requested);
    assert_eq!(activity.cancel(operation.id()), CancelOutcome::Requested);
    assert!(token.is_cancelled());
    operation.finish(ActivityOutcome::Cancelled, "cancelled by test");
    assert!(matches!(
        activity.cancel(operation.id()),
        CancelOutcome::NotCancellable(ActivityStatus::Finished(ActivityOutcome::Cancelled))
    ));
}

#[test]
fn queued_operation_becomes_running_on_first_work_stage() {
    let activity = Activity::new();
    let operation = activity.enqueue(ActivityKind::Pull, "model", CancellationToken::new());
    assert_eq!(activity.history()[0].status, ActivityStatus::Queued);
    operation.progress(
        ActivityStage::Resolving,
        "resolving model",
        Some(ActivityProgress::new(0, None)),
    );
    assert_eq!(
        activity.history()[0].status,
        ActivityStatus::Running {
            stage: ActivityStage::Resolving,
            progress: Some(ActivityProgress::new(0, None)),
        }
    );
}

#[test]
fn history_limit_never_evicts_a_running_operation() {
    let activity = Activity::new();
    let oldest = activity.begin(ActivityKind::Generate, "oldest", Some(CancellationToken::new()));
    for index in 0..HISTORY_LIMIT {
        activity
            .begin(ActivityKind::Load, &index.to_string(), None)
            .finish(ActivityOutcome::Completed, "done");
    }
    assert_eq!(activity.cancel(oldest.id()), CancelOutcome::Requested);
}
