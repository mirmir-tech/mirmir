use super::context::ContextControl;
use crate::{
    rpc::proto,
    tui::ranges::{FieldRange, GENERATION_FIELDS, generation_ranges, token_limit},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadStatus {
    Inspecting,
    Editing,
    Loading,
}

/// One editable row of the load dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadRow {
    Context,
    Field(usize),
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
    pub context: Option<ContextControl>,
    pub fields: [String; GENERATION_FIELDS],
    pub selected: usize,
    pub has_mirmir_overrides: bool,
    pub memory: Option<proto::ModelMemoryEstimate>,
    pub force: bool,
    pub progress: Option<proto::ModelLifecycleEvent>,
    pub error: Option<String>,
}

impl LoadDialog {
    /// The editable rows in display order: the context first when the model has
    /// one, then the generation settings of a generation model.
    pub fn rows(&self) -> impl Iterator<Item = LoadRow> {
        let fields = if self.task == "generation" {
            GENERATION_FIELDS
        } else {
            0
        };
        self.context
            .is_some()
            .then_some(LoadRow::Context)
            .into_iter()
            .chain((0..fields).map(LoadRow::Field))
    }

    pub fn row(&self) -> LoadRow {
        self.rows().nth(self.selected).unwrap_or(LoadRow::Field(GENERATION_FIELDS - 1))
    }

    pub fn row_count(&self) -> usize {
        self.rows().count()
    }

    /// The generation ranges, with max tokens bounded by the chosen context.
    pub fn ranges(&self) -> [FieldRange; GENERATION_FIELDS] {
        let context = self.context.as_ref().map(ContextControl::token_limit);
        generation_ranges(token_limit(context, self.capabilities.as_ref()))
    }

    pub(super) fn request(&self) -> Result<proto::LoadModelRequest, String> {
        let settings = if self.task == "generation" {
            let [tokens, temperature, top_p, top_k, penalty] = self.ranges();
            Some(proto::GenerationSettings {
                max_tokens: tokens.clamped(&self.fields[0])?,
                temperature: temperature.clamped(&self.fields[1])?,
                top_p: top_p.clamped(&self.fields[2])?,
                top_k: top_k.clamped(&self.fields[3])?,
                repetition_penalty: penalty.clamped(&self.fields[4])?,
            })
        } else {
            None
        };
        let context = self.context.as_ref().map(|context| context.choice.to_proto());
        Ok(proto::LoadModelRequest {
            selector: self.target.selector.clone(),
            settings,
            config_id: self.target.config_id.clone(),
            repo_id: self.target.repo_id.clone(),
            revision: self.target.revision.clone(),
            commit: self.target.commit.clone(),
            force: self.force,
            context,
        })
    }
}

#[cfg(test)]
pub(super) fn dialog(task: &str, context: Option<ContextControl>) -> LoadDialog {
    LoadDialog {
        target: LoadTarget {
            selector: "Qwen--Test".to_owned(),
            config_id: "Qwen--Test".to_owned(),
            repo_id: "Qwen/Test".to_owned(),
            revision: "main".to_owned(),
            commit: "abc123".to_owned(),
        },
        task: task.to_owned(),
        capabilities: None,
        status: LoadStatus::Editing,
        context,
        fields: ["2048", "0.7", "0.9", "40", "1.1"].map(str::to_owned),
        selected: 0,
        has_mirmir_overrides: false,
        memory: None,
        force: false,
        progress: None,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::context::{ContextChoice, limits},
        *,
    };

    #[test]
    fn embedding_load_omits_generation_settings() {
        let request = dialog("embedding", None).request().expect("load request");
        assert!(request.settings.is_none());
        assert!(request.context.is_none());
    }

    #[test]
    fn generation_load_carries_the_context_and_clamped_settings() {
        let mut control = ContextControl::new(limits(ContextChoice::Automatic));
        control.choice = ContextChoice::Tokens(1_024);
        let mut dialog = dialog("generation", Some(control));
        dialog.fields = ["99999", "9", "0.5", "999", "0.1"].map(str::to_owned);
        let request = dialog.request().expect("load request");
        assert_eq!(request.context, Some(ContextChoice::Tokens(1_024).to_proto()));
        let settings = request.settings.expect("generation settings");
        assert_eq!(settings.max_tokens, 1_024);
        assert_eq!(settings.top_k, 200);
        assert_eq!(settings.temperature.to_string(), "2");
        assert_eq!(settings.repetition_penalty.to_string(), "0.5");
    }

    #[test]
    fn context_row_leads_only_when_the_model_has_one() {
        let mut dialog = dialog("generation", None);
        assert_eq!(dialog.row(), LoadRow::Field(0));
        assert_eq!(dialog.row_count(), GENERATION_FIELDS);
        dialog.context = Some(ContextControl::new(limits(ContextChoice::Automatic)));
        assert_eq!(dialog.row(), LoadRow::Context);
        dialog.selected = 1;
        assert_eq!(dialog.row(), LoadRow::Field(0));
        assert_eq!(dialog.row_count(), GENERATION_FIELDS + 1);
        assert_eq!(dialog.ranges()[0].maximum.to_string(), "40960");
    }

    #[test]
    fn a_reranker_with_a_cache_offers_and_sends_only_its_context() -> Result<(), String> {
        let control = ContextControl::new(proto::ModelContext {
            block_tokens: 16,
            min_tokens: 16,
            max_tokens: 8_192,
            saved: Some(ContextChoice::Automatic.to_proto()),
            automatic_tokens: 4_096,
            resident_tokens: None,
        });
        let mut dialog = dialog("rerank", Some(control));
        dialog.context.as_mut().ok_or("context")?.choice = ContextChoice::Tokens(2_048);
        assert_eq!(dialog.rows().collect::<Vec<_>>(), vec![LoadRow::Context]);
        let request = dialog.request()?;
        assert!(request.settings.is_none());
        assert_eq!(request.context, Some(ContextChoice::Tokens(2_048).to_proto()));
        Ok(())
    }
}
