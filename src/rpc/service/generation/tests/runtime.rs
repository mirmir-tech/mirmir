//! A model runtime whose model resolution blocks until the test releases it,
//! so a generation is held before its first token.

use std::{
    sync::{Mutex, mpsc},
    time::Duration,
};

use libmir::{EmbeddingOutput, EmbeddingRequest, ProgressEvent, RerankOutput, RerankRequest};

use crate::{
    application::{
        Error, LocalModelInfo, ModelEntry, ModelInfo, ModelInspection, ModelRuntimePort, Result,
        RuntimeTelemetry,
    },
    config::{GenerationConfig, HubModelConfig, StateRecovery},
};

const HELD: Duration = Duration::from_secs(10);

pub struct Blocked {
    entered: Mutex<mpsc::Sender<()>>,
    release: Mutex<mpsc::Receiver<()>>,
}

/// The test's side: learns when resolution started and releases it.
pub struct Gate {
    pub entered: mpsc::Receiver<()>,
    pub release: mpsc::Sender<()>,
}

impl Blocked {
    pub fn new() -> (Self, Gate) {
        let (entered, entered_receiver) = mpsc::channel();
        let (release_sender, release) = mpsc::channel();
        let runtime = Self {
            entered: Mutex::new(entered),
            release: Mutex::new(release),
        };
        (
            runtime,
            Gate {
                entered: entered_receiver,
                release: release_sender,
            },
        )
    }
}

fn unused<T>() -> Result<T> {
    Err(Error::Inference("not used by this test".to_owned()))
}

impl ModelRuntimePort for Blocked {
    fn telemetry(&self) -> Result<RuntimeTelemetry> {
        unused()
    }

    fn models(&self) -> Result<Vec<ModelInfo>> {
        Ok(Vec::new())
    }

    fn active_models(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn take_state_recovery(&self) -> Result<Option<StateRecovery>> {
        Ok(None)
    }

    fn local_models(&self) -> Result<Vec<LocalModelInfo>> {
        Ok(Vec::new())
    }

    fn inspect_model(&self, _selector: &str) -> Result<ModelInspection> {
        unused()
    }

    fn prepare_load(
        &self,
        _selector: &str,
        _config_id: &str,
        _hub: Option<HubModelConfig>,
        _generation: Option<GenerationConfig>,
    ) -> Result<String> {
        unused()
    }

    fn save_generation_defaults(
        &self,
        _selector: &str,
        _generation: GenerationConfig,
    ) -> Result<String> {
        unused()
    }

    fn load_model(
        &self,
        _selector: &str,
        _force: bool,
        _progress: &mut dyn FnMut(ProgressEvent),
    ) -> Result<ModelEntry> {
        let poisoned = || Error::StatePoisoned("blocked runtime");
        let _entered = self.entered.lock().map_err(|_| poisoned())?.send(());
        let released = self.release.lock().map_err(|_| poisoned())?.recv_timeout(HELD);
        Err(Error::Inference(format!("model resolution ended: {released:?}")))
    }

    fn unload_model(&self, _selector: &str) -> Result<bool> {
        Ok(false)
    }

    fn embed(&self, _selector: &str, _request: EmbeddingRequest) -> Result<EmbeddingOutput> {
        unused()
    }

    fn rerank(&self, _selector: &str, _request: RerankRequest) -> Result<RerankOutput> {
        unused()
    }
}
