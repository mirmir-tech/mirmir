use crate::rpc::proto;

pub struct LifecycleState<'a> {
    pub current: u64,
    pub total: Option<u64>,
    pub unit: &'a str,
    pub detail: &'a str,
    pub model: Option<proto::ModelInfo>,
}

pub fn event(
    operation_id: &str,
    selector: &str,
    phase: &str,
    state: LifecycleState<'_>,
) -> proto::ModelLifecycleEvent {
    proto::ModelLifecycleEvent {
        operation_id: operation_id.to_owned(),
        selector: selector.to_owned(),
        phase: phase.to_owned(),
        current: state.current,
        total: state.total,
        unit: state.unit.to_owned(),
        detail: state.detail.to_owned(),
        model: state.model,
    }
}

pub fn checking_memory(operation_id: &str, selector: &str) -> proto::ModelLifecycleEvent {
    event(
        operation_id,
        selector,
        "checking_memory",
        LifecycleState {
            current: 0,
            total: None,
            unit: "byte",
            detail: "checking weights, KV cache, workspace, and device budget",
            model: None,
        },
    )
}
