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
        Ok(proto::LoadModelRequest {
            selector: self.target.selector.clone(),
            settings: Some(proto::GenerationSettings {
                max_tokens: parse(&self.fields[0], "max tokens")?,
                temperature: parse(&self.fields[1], "temperature")?,
                top_p: parse(&self.fields[2], "top p")?,
                top_k: parse(&self.fields[3], "top k")?,
                repetition_penalty: parse(&self.fields[4], "repetition penalty")?,
            }),
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
