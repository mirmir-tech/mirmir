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

    pub(super) const fn priority(self) -> u8 {
        match self {
            Self::Starting => 0,
            Self::Resolving => 1,
            Self::Runtime(ProgressStage::LoadWeights) => 2,
            Self::Runtime(ProgressStage::InitializeRuntime) => 3,
            Self::Runtime(ProgressStage::Warmup) => 4,
            Self::Runtime(ProgressStage::PrefillTokens) => 5,
            Self::Runtime(ProgressStage::DecodeTokens) => 6,
        }
    }
}
