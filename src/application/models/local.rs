use super::LocalModelInfo;
use crate::application::{Application, Result};

impl Application {
    pub fn local_models(&self) -> Result<Vec<LocalModelInfo>> {
        self.runtime.local_models()
    }
}
