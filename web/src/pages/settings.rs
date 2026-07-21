use leptos::prelude::*;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use crate::{
    api,
    state::RuntimeState,
    types::{Setting, catalog::UpdatedConfiguration},
};

#[component]
pub fn SettingsPage() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    view! { <section class="view active" id="configuration">
        <div class="table-wrap configuration-table"><table><thead><tr><th>Setting</th><th>Effective value</th><th>Source</th><th>Restart</th><th>Action</th></tr></thead><tbody>
            <For each=move || state.configuration.get().map_or_else(Vec::new, |value| value.values) key=|setting| setting.key.clone() children=move |setting| view! { <SettingRow setting=setting /> } />
        </tbody></table></div>
        <details class="raw-config"><summary>"Raw config.toml"</summary><div class="config-paths"><span>{move || state.configuration.get().map(|value| value.config_path).unwrap_or_default()}</span><span>{move || state.configuration.get().map(|value| value.secrets_path).unwrap_or_default()}</span></div><pre>{move || state.configuration.get().map(|value| value.raw_toml).unwrap_or_default()}</pre></details>
    </section> }
}

#[component]
fn SettingRow(setting: Setting) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let editing = RwSignal::new(false);
    let value = RwSignal::new(String::new());
    let key = StoredValue::new(setting.key.clone());
    let secret = setting.kind == "secret";
    let can_edit = setting.actions.iter().any(|action| action == "edit");
    let can_test = setting.actions.iter().any(|action| action == "test");
    let can_remove = setting.actions.iter().any(|action| action == "remove");
    view! { <tr><td><code>{setting.key.clone()}</code></td><td>
        <span hidden=move || editing.get()>{setting.value.clone()}</span>
        <input hidden=move || !editing.get() type=if secret { "password" } else { "text" } prop:value=move || value.get() on:input=move |event| value.set(event_target_value(&event)) />
    </td><td><span class="source">{setting.source}</span></td><td>{if setting.restart_required { "required" } else { "live" }}</td><td>
        <span class="inline-actions" hidden=move || editing.get()>
            <Show when=move || can_edit><button type="button" on:click=move |_| editing.set(true)>"Edit"</button></Show>
            <Show when=move || can_test><button type="button" on:click=move |_| update(state, &key.get_value(), "test_value", "", editing)>"Test"</button></Show>
            <Show when=move || can_remove><button type="button" class="danger" on:click=move |_| update(state, &key.get_value(), "remove_value", "", editing)>"Remove"</button></Show>
        </span>
        <span class="inline-actions" hidden=move || !editing.get()><button type="button" on:click=move |_| update(state, &key.get_value(), "set_value", &value.get_untracked(), editing)>"Save"</button><button type="button" class="quiet" on:click=move |_| editing.set(false)>"Cancel"</button></span>
    </td></tr> }
}

fn update(
    state: RuntimeState,
    key: &str,
    operation: &'static str,
    value: &str,
    editing: RwSignal<bool>,
) {
    let body = if operation == "set_value" {
        json!({"operation": operation, "key": key, "value": value})
    } else {
        json!({"operation": operation, "key": key})
    };
    spawn_local(async move {
        match api::post::<UpdatedConfiguration, _>(state, "/configuration", &body).await {
            Ok(updated) => {
                state.configuration.set(Some(updated.configuration));
                let suffix = if updated.restart_required {
                    " · restart required"
                } else {
                    ""
                };
                state.notify(format!("{}{suffix}", updated.message), false);
                editing.set(false);
            },
            Err(error) => state.notify(error, true),
        }
    });
}
