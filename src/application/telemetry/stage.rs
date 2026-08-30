use libmir::ProgressStage;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    Starting,
    Resolving,
    Runtime(ProgressStage),
}

impl Stage {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Resolving => "resolving",
            Self::Runtime(stage) => stage.as_str(),
        }
    }
}
