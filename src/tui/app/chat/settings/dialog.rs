use crate::{
    rpc::proto,
    tui::ranges::{FieldRange, GENERATION_FIELDS, generation_ranges},
};

/// The generation fields followed by the one-off seed.
pub const CHAT_FIELD_COUNT: usize = GENERATION_FIELDS + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSettingsStatus {
    Inspecting,
    Editing,
    Saving,
}

#[derive(Debug, Clone)]
pub struct ChatParameters {
    pub model: String,
    pub max_tokens: u64,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u64,
    pub repetition_penalty: f32,
    pub seed: Option<u64>,
    /// The most tokens a request can generate for this model, when known.
    pub token_limit: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ChatSettingsDialog {
    pub model: String,
    pub fields: [String; CHAT_FIELD_COUNT],
    pub selected: usize,
    pub save_default: bool,
    pub persisted: bool,
    pub token_limit: Option<u64>,
    pub status: ChatSettingsStatus,
    pub error: Option<String>,
}

impl ChatSettingsDialog {
    pub(super) fn inspecting(model: String) -> Self {
        Self {
            model,
            fields: std::array::from_fn(|_| String::new()),
            selected: 0,
            save_default: false,
            persisted: false,
            token_limit: None,
            status: ChatSettingsStatus::Inspecting,
            error: None,
        }
    }

    pub(super) fn from_parameters(parameters: &ChatParameters) -> Self {
        Self {
            model: parameters.model.clone(),
            fields: settings_fields(&parameters.settings(), parameters.seed),
            selected: 0,
            save_default: false,
            persisted: false,
            token_limit: parameters.token_limit,
            status: ChatSettingsStatus::Editing,
            error: None,
        }
    }

    /// The generation ranges, with max tokens bounded by the model's limit.
    pub fn ranges(&self) -> [FieldRange; GENERATION_FIELDS] {
        generation_ranges(self.token_limit)
    }

    /// The parameters with every generation value clamped to its range.
    pub(super) fn parameters(&self) -> Result<ChatParameters, String> {
        let [tokens, temperature, top_p, top_k, penalty] = self.ranges();
        Ok(ChatParameters {
            model: self.model.clone(),
            max_tokens: tokens.clamped(&self.fields[0])?,
            temperature: temperature.clamped(&self.fields[1])?,
            top_p: top_p.clamped(&self.fields[2])?,
            top_k: top_k.clamped(&self.fields[3])?,
            repetition_penalty: penalty.clamped(&self.fields[4])?,
            seed: optional(&self.fields[5], "seed")?,
            token_limit: self.token_limit,
        })
    }
}

impl ChatParameters {
    pub(super) const fn settings(&self) -> proto::GenerationSettings {
        proto::GenerationSettings {
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            repetition_penalty: self.repetition_penalty,
        }
    }
}

pub(super) fn settings_fields(
    settings: &proto::GenerationSettings,
    seed: Option<u64>,
) -> [String; CHAT_FIELD_COUNT] {
    [
        settings.max_tokens.to_string(),
        settings.temperature.to_string(),
        settings.top_p.to_string(),
        settings.top_k.to_string(),
        settings.repetition_penalty.to_string(),
        seed.map_or_else(String::new, |seed| seed.to_string()),
    ]
}

fn parse<T: std::str::FromStr>(value: &str, name: &str) -> Result<T, String> {
    value.parse().map_or_else(|_| Err(format!("invalid {name}: `{value}`")), Ok)
}

fn optional<T: std::str::FromStr>(value: &str, name: &str) -> Result<Option<T>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        parse(value, name).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_ranges_and_keeps_optional_seed() {
        let mut dialog = ChatSettingsDialog::inspecting("model".to_owned());
        dialog.fields = ["128", "0.7", "0.9", "40", "1.1", ""].map(str::to_owned);
        assert_eq!(dialog.parameters().expect("valid settings").seed, None);
        dialog.fields[0] = "100000".to_owned();
        dialog.fields[2] = "1.1".to_owned();
        dialog.token_limit = Some(8_192);
        let parameters = dialog.parameters().expect("clamped settings");
        assert_eq!(parameters.max_tokens, 8_192);
        assert_eq!(parameters.top_p.to_string(), "1");
        dialog.fields[1] = ".".to_owned();
        assert!(dialog.parameters().is_err());
    }

    #[test]
    fn accepts_zero_top_k_as_unbounded_sampling() {
        let mut dialog = ChatSettingsDialog::inspecting("model".to_owned());
        dialog.fields = ["128", "1", "1", "0", "1", ""].map(str::to_owned);

        assert_eq!(dialog.parameters().expect("unbounded top-k should be valid").top_k, 0);
    }
}
