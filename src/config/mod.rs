mod editing;
mod file;
mod generation;
mod models;
mod paths;
mod presentation;
mod runtime;
mod schema;
mod state;
mod store;

use std::path::Path;

pub use file::write_toml;
pub use models::model_key;
pub use paths::Paths;
pub use presentation::{ConfigPresentation, SecretPresentation};
pub use schema::{AppConfig, GenerationConfig, HubModelConfig, ModelConfig, ServerSettings};
pub use store::Store;

use crate::error::Result;

pub fn load_dotenv(root: &Path) -> Result<()> {
    for name in [".env.local", ".env"] {
        let path = root.join(name);
        if path.exists() {
            dotenvy::from_path(path)?;
        }
    }
    Ok(())
}
