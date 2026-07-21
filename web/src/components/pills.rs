use leptos::prelude::*;

use super::Icon;

#[component]
pub fn TypePill(value: String) -> impl IntoView {
    let class = value.to_lowercase().replace(['_', ' '], "-");
    view! { <span class=format!("type-pill {class}")>{if value.is_empty() { "Unknown".to_owned() } else { value }}</span> }
}

#[component]
pub fn StatePill(
    #[prop(into)] state: Signal<String>,
    #[prop(into)] detail: Signal<String>,
    #[prop(into)] progress: Signal<Option<f64>>,
) -> impl IntoView {
    view! {
        <span class=move || format!("state-pill {}", state.get().replace([' ', '_'], "-")) title=move || detail.get()>
            <Show when=move || matches!(state.get().as_str(), "loading" | "unloading" | "downloading")>
                <span class="progress-ring" style=move || progress.get().map(|value| format!("--progress: {value}%"))></span>
            </Show>
            {move || state.get().replace(['_', '-'], " ")}
        </span>
    }
}

#[component]
pub fn FeaturePills(tool_use: bool, thinking: bool, vision: bool) -> impl IntoView {
    view! {
        <span class="feature-list">
            {tool_use.then(|| view! { <span class="feature-pill tools" title="Tool use"><Icon name="tools" /></span> })}
            {thinking.then(|| view! { <span class="feature-pill thinking" title="Thinking"><Icon name="thinking" /></span> })}
            {vision.then(|| view! { <span class="feature-pill vision" title="Vision"><Icon name="vision" /></span> })}
            {(!tool_use && !thinking && !vision).then(|| view! { <span class="feature-empty">"—"</span> })}
        </span>
    }
}
