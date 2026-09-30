use leptos::prelude::*;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use crate::{
    api,
    components::Icon,
    state::RuntimeState,
    types::{Setting, catalog::UpdatedConfiguration},
};

#[component]
pub fn SettingsPage() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let groups =
        move || grouped(state.configuration.get().map_or_else(Vec::new, |value| value.values));
    view! { <section class="view active" id="configuration">
        <Show when=move || !state.pending_restart.get().is_empty()>
            <div class="restart-banner" role="status">
                <Icon name="restart" />
                <div><strong>{move || restart_title(state.pending_restart.get().len())}</strong><span>{move || state.pending_restart.get().join(", ")}</span></div>
                <span class="restart-hint">"Restart mirmir serve to apply."</span>
                <button type="button" class="settings-action quiet" on:click=move |_| state.pending_restart.set(Vec::new())>"Dismiss"</button>
            </div>
        </Show>
        {move || groups().into_iter().map(|(group, settings)| view! {
            <section class="settings-group" aria-label=group_title(&group)>
                <h2>{group_title(&group)}</h2>
                <div class="table-wrap configuration-table"><table><thead><tr><th>"Setting"</th><th>"Value"</th><th>"Source"</th><th>"Applies"</th><th><span class="visually-hidden">"Actions"</span></th></tr></thead><tbody>
                    {settings.into_iter().map(|setting| view! { <SettingRow setting=setting /> }).collect_view()}
                </tbody></table></div>
            </section>
        }).collect_view()}
        <details class="raw-config"><summary>"Raw config.toml"</summary><div class="config-paths"><span>{move || state.configuration.get().map(|value| value.config_path).unwrap_or_default()}</span><span>{move || state.configuration.get().map(|value| value.secrets_path).unwrap_or_default()}</span></div><pre>{move || state.configuration.get().map(|value| value.raw_toml).unwrap_or_default()}</pre></details>
    </section> }
}

fn restart_title(count: usize) -> String {
    if count == 1 {
        "1 change takes effect after a restart".to_owned()
    } else {
        format!("{count} changes take effect after a restart")
    }
}

/// Groups settings by the section before the first dot, keeping their order.
fn grouped(values: Vec<Setting>) -> Vec<(String, Vec<Setting>)> {
    let mut groups: Vec<(String, Vec<Setting>)> = Vec::new();
    for setting in values {
        let group =
            setting.key.split_once('.').map_or("general", |(section, _)| section).to_owned();
        match groups.iter_mut().find(|(name, _)| *name == group) {
            Some((_, settings)) => settings.push(setting),
            None => groups.push((group, vec![setting])),
        }
    }
    groups
}

fn group_title(group: &str) -> String {
    group
        .split('_')
        .map(|word| {
            let mut characters = word.chars();
            characters
                .next()
                .map_or_else(String::new, |first| first.to_uppercase().chain(characters).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    let default_source = setting.source == "default";
    let edit_label = format!("Edit {}", setting.key);
    view! { <tr class:editing=move || editing.get()><td><code class="setting-key">{setting.key.clone()}</code></td><td class="setting-value">
        <span hidden=move || editing.get()>{setting.value.clone()}</span>
        <input hidden=move || !editing.get() type=if secret { "password" } else { "text" } aria-label=setting.key.clone() prop:value=move || value.get() on:input=move |event| value.set(event_target_value(&event)) />
    </td><td><span class="source-chip" class:default=default_source class:secret=secret>{setting.source}</span></td><td><span class="restart-badge" class:live=!setting.restart_required>{if setting.restart_required { "Restart" } else { "Live" }}</span></td><td>
        <span class="inline-actions" hidden=move || editing.get()>
            <Show when=move || can_edit><button type="button" class="settings-action edit" aria-label=edit_label.clone() on:click=move |_| editing.set(true)><Icon name="edit" /><span>"Edit"</span></button></Show>
            <Show when=move || can_test><button type="button" class="settings-action" on:click=move |_| update(state, &key.get_value(), "test_value", "", editing)>"Test"</button></Show>
            <Show when=move || can_remove><button type="button" class="settings-action danger" on:click=move |_| update(state, &key.get_value(), "remove_value", "", editing)>"Remove"</button></Show>
        </span>
        <span class="inline-actions" hidden=move || !editing.get()><button type="button" class="settings-action primary" on:click=move |_| update(state, &key.get_value(), "set_value", &value.get_untracked(), editing)>"Save"</button><button type="button" class="settings-action quiet" on:click=move |_| editing.set(false)>"Cancel"</button></span>
    </td></tr> }
}

fn update(
    state: RuntimeState,
    key: &str,
    operation: &'static str,
    value: &str,
    editing: RwSignal<bool>,
) {
    let changed_key = key.to_owned();
    let body = if operation == "set_value" {
        json!({"operation": operation, "key": key, "value": value})
    } else {
        json!({"operation": operation, "key": key})
    };
    spawn_local(async move {
        match api::post::<UpdatedConfiguration, _>(state, "/configuration", &body).await {
            Ok(updated) => {
                state.configuration.set(Some(updated.configuration));
                if updated.restart_required {
                    state.pending_restart.update(|keys| {
                        if !keys.contains(&changed_key) {
                            keys.push(changed_key);
                        }
                    });
                }
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
