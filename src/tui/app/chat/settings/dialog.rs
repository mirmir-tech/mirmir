use crate::rpc::proto;

pub const CHAT_FIELD_COUNT: usize = 6;

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
}

#[derive(Debug, Clone)]
pub struct ChatSettingsDialog {
    pub model: String,
    pub fields: [String; CHAT_FIELD_COUNT],
    pub selected: usize,
    pub save_default: bool,
    pub persisted: bool,
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
            status: ChatSettingsStatus::Editing,
            error: None,
        }
    }

    pub(super) fn parameters(&self) -> Result<ChatParameters, String> {
        let parameters = ChatParameters {
            model: self.model.clone(),
            max_tokens: parse(&self.fields[0], "max tokens")?,
            temperature: parse(&self.fields[1], "temperature")?,
            top_p: parse(&self.fields[2], "top p")?,
            top_k: parse(&self.fields[3], "top k")?,
            repetition_penalty: parse(&self.fields[4], "repetition penalty")?,
            seed: optional(&self.fields[5], "seed")?,
        };
        parameters.validate()?;
        Ok(parameters)
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

    fn validate(&self) -> Result<(), String> {
        if self.max_tokens == 0 || self.repetition_penalty <= 0.0 {
            return Err("max tokens and repetition penalty must be positive".to_owned());
        }
        if !self.temperature.is_finite() || self.temperature < 0.0 {
            return Err("temperature must be finite and non-negative".to_owned());
        }
        if !self.top_p.is_finite() || !(0.0..=1.0).contains(&self.top_p) {
            return Err("top p must be between 0 and 1".to_owned());
        }
        Ok(())
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
    fn validates_ranges_and_optional_seed() {
        let mut dialog = ChatSettingsDialog::inspecting("model".to_owned());
        dialog.fields = ["128", "0.7", "0.9", "40", "1.1", ""].map(str::to_owned);
        assert_eq!(dialog.parameters().expect("valid settings").seed, None);
        dialog.fields[2] = "1.1".to_owned();
        assert!(dialog.parameters().is_err());
    }

    #[test]
    fn accepts_zero_top_k_as_unbounded_sampling() {
        let mut dialog = ChatSettingsDialog::inspecting("model".to_owned());
        dialog.fields = ["128", "1", "1", "0", "1", ""].map(str::to_owned);

        assert_eq!(dialog.parameters().expect("unbounded top-k should be valid").top_k, 0);
    }
}
