//! A bounded generation parameter: a number entry paired with a slider.

pub mod range;
#[cfg(test)]
mod tests;

use leptos::prelude::*;

pub use self::range::Range;

/// `value` is `None` when the parameter is left to the model's default; an
/// emptied number entry reports `None`.
#[component]
pub fn RangeParameter<F>(
    label: &'static str,
    #[prop(into)] range: Signal<Range>,
    #[prop(into)] value: Signal<Option<f64>>,
    on_change: F,
) -> impl IntoView
where
    F: Fn(Option<f64>) + Copy + Send + Sync + 'static,
{
    let shown = move || {
        let range = range.get();
        value.get().map(|value| range.format(range.clamp(value))).unwrap_or_default()
    };
    let position = move || {
        let range = range.get();
        range.format(value.get().map_or_else(|| range.middle(), |value| range.clamp(value)))
    };
    view! { <div class="parameter-control">
        <label>{label}<input type="number" placeholder="Default" min=move || range.get().min max=move || range.get().max step=move || range.get().step prop:value=shown on:change=move |event| {
            let input = event_target::<web_sys::HtmlInputElement>(&event);
            on_change(range.get_untracked().parse(&input.value()));
            input.set_value(&shown());
        } /></label>
        <input class="parameter-slider" class:unset=move || value.get().is_none() type="range" min=move || range.get().min max=move || range.get().max step=move || range.get().step prop:value=position on:input=move |event| on_change(range.get_untracked().parse(&event_target_value(&event))) />
    </div> }
}
