mod context;
mod dialogs;
mod inspect;
mod memory;
mod row;
mod search;

use leptos::prelude::*;

use self::{
    dialogs::{LoadDialog, RemoveDialog},
    row::LocalRow,
    search::SearchPopup,
};
use crate::{
    state::RuntimeState,
    types::{
        Model, ModelState,
        catalog::{ContextChoice, GenerationSettings, Inspection},
    },
};

#[derive(Clone, Copy)]
pub(super) struct ModelUi {
    pub search_open: RwSignal<bool>,
    pub load_target: RwSignal<Option<Model>>,
    pub inspection: RwSignal<Option<Inspection>>,
    pub inspect_error: RwSignal<Option<String>>,
    /// Generation parameters as edited; re-inspection leaves them alone.
    pub settings: RwSignal<GenerationSettings>,
    pub context: RwSignal<ContextChoice>,
    /// The latest inspection request; older answers are ignored.
    pub request: StoredValue<u64>,
    pub remove_target: RwSignal<Option<String>>,
}

#[component]
pub fn ModelsPage() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let ui = ModelUi {
        search_open: RwSignal::new(false),
        load_target: RwSignal::new(None),
        inspection: RwSignal::new(None),
        inspect_error: RwSignal::new(None),
        settings: RwSignal::new(GenerationSettings::default()),
        context: RwSignal::new(ContextChoice::default()),
        request: StoredValue::new(0),
        remove_target: RwSignal::new(None),
    };
    provide_context(ui);
    view! {
        <section class="view active" id="models">
            <SearchPopup />
            <div class="table-wrap models-table">
                <table>
                    <thead><tr><th>Name</th><th>Format</th><th>Capabilities</th><th>Size</th><th>State</th><th><span class="visually-hidden">Actions</span></th></tr></thead>
                    <tbody><For each=move || state.models.get() key=model_key children=move |model| view! { <LocalRow model=model /> } /></tbody>
                </table>
                <Show when=move || !state.models_loaded.get()>
                    <p class="empty loading" aria-busy="true">"Loading local models…"</p>
                </Show>
                <Show when=move || state.models_loaded.get() && state.models.get().is_empty()>
                    <p class="empty">"No local models."</p>
                </Show>
            </div>

            <LoadDialog />
            <RemoveDialog />
        </section>
    }
}

fn model_key(model: &Model) -> (String, ModelState, String, u64) {
    (
        model.selector.clone(),
        model.state,
        model.load_unavailable_reason.clone(),
        model.size_bytes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_state_changes_replace_the_keyed_row() {
        let ready = Model {
            selector: "Qwen/Test".to_owned(),
            state: ModelState::Ready,
            ..Default::default()
        };
        let mut unloaded = ready.clone();
        unloaded.state = ModelState::Available;

        assert_ne!(model_key(&ready), model_key(&unloaded));
    }
}
