use crate::rpc::proto;

pub(super) const FIELD_COUNT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadStatus {
    Inspecting,
    Editing,
    Loading,
}

#[derive(Debug, Clone)]
pub struct LoadTarget {
    pub selector: String,
    pub config_id: String,
    pub repo_id: String,
    pub revision: String,
    pub commit: String,
}

#[derive(Debug, Clone)]
pub struct LoadDialog {
    pub target: LoadTarget,
    pub task: String,
    pub capabilities: Option<proto::ModelTaskCapabilities>,
    pub status: LoadStatus,
    pub fields: [String; FIELD_COUNT],
    pub selected: usize,
    pub has_mirmir_overrides: bool,
    pub memory: Option<proto::ModelMemoryEstimate>,
    pub force: bool,
    pub progress: Option<proto::ModelLifecycleEvent>,
    pub error: Option<String>,
    pub restore: Option<RestorePosition>,
}

#[derive(Debug, Clone, Copy)]
pub struct RestorePosition {
    pub current: usize,
    pub total: usize,
}

impl LoadDialog {
    pub(super) fn request(&self) -> Result<proto::LoadModelRequest, String> {
        let settings = if self.task == "generation" {
            Some(proto::GenerationSettings {
                max_tokens: parse(&self.fields[0], "max tokens")?,
                temperature: parse(&self.fields[1], "temperature")?,
                top_p: parse(&self.fields[2], "top p")?,
                top_k: parse(&self.fields[3], "top k")?,
                repetition_penalty: parse(&self.fields[4], "repetition penalty")?,
            })
        } else {
            None
        };
        Ok(proto::LoadModelRequest {
            selector: self.target.selector.clone(),
            settings,
            config_id: self.target.config_id.clone(),
            repo_id: self.target.repo_id.clone(),
            revision: self.target.revision.clone(),
            commit: self.target.commit.clone(),
            force: self.force,
        })
    }
}

fn parse<T: std::str::FromStr>(value: &str, name: &str) -> Result<T, String> {
    value.parse().map_err(|_| format!("invalid {name}: `{value}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_load_omits_generation_settings() {
        let dialog = LoadDialog {
            target: LoadTarget {
                selector: "Qwen--Embedding".to_owned(),
                config_id: "Qwen--Embedding".to_owned(),
                repo_id: String::new(),
                revision: String::new(),
                commit: String::new(),
            },
            task: "embedding".to_owned(),
            capabilities: None,
            status: LoadStatus::Editing,
            fields: std::array::from_fn(|_| String::new()),
            selected: 0,
            has_mirmir_overrides: false,
            memory: None,
            force: false,
            progress: None,
            error: None,
            restore: None,
        };

        assert!(dialog.request().expect("load request").settings.is_none());
    }
}
