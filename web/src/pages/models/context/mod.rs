//! The load dialog's context control: automatic, or a chosen K/V capacity.

pub mod scale;
#[cfg(test)]
mod tests;

use leptos::prelude::*;

use self::scale::{STEPS, Scale, effective, grouped};
use super::{ModelUi, inspect::choose_context};
use crate::types::catalog::{ContextChoice, ContextInfo};

/// The inspected context limits; `None` hides the control.
pub fn info(ui: ModelUi) -> Option<ContextInfo> {
    ui.inspection.with(|item| item.as_ref().and_then(|item| item.context.clone()))
}

/// The context the dialog will load with, in tokens.
pub fn chosen_tokens(ui: ModelUi) -> Option<u64> {
    info(ui).map(|info| effective(ui.context.get(), &info))
}

#[component]
pub fn ContextControl() -> impl IntoView {
    let ui = expect_context::<ModelUi>();
    let automatic = move || ui.context.get() == ContextChoice::Automatic;
    let readout = move || chosen_tokens(ui).map(grouped).unwrap_or_default();
    let position = move || {
        info(ui)
            .map_or(0, |info| Scale::from(&info).position_of(effective(ui.context.get(), &info)))
    };
    let slide = move |text: String| {
        if let (Some(info), Ok(position)) = (info(ui), text.parse::<u32>()) {
            choose_context(ui, ContextChoice::Tokens(Scale::from(&info).tokens_at(position)));
        }
    };
    let toggle = move |checked: bool| {
        let choice = if checked {
            ContextChoice::Automatic
        } else {
            let tokens = info(ui).map_or(0, |info| Scale::from(&info).clamp(info.automatic_tokens));
            ContextChoice::Tokens(tokens)
        };
        choose_context(ui, choice);
    };
    let detail = move || {
        info(ui).map(|info| {
            let resident = info
                .resident_tokens
                .map(|tokens| format!(" · loaded with {}", grouped(tokens)))
                .unwrap_or_default();
            format!("Automatic · {} tokens now{resident}", grouped(info.automatic_tokens))
        })
    };
    view! { <Show when=move || info(ui).is_some()>
        <div class="parameter-control context-control">
            <label><span>"Context"</span><output class="context-readout">{readout}" tokens"</output></label>
            <input class="parameter-slider" type="range" min=0 max=STEPS step=1 aria-label="Context tokens" prop:value=position disabled=automatic on:input=move |event| slide(event_target_value(&event)) />
            <div class="context-meta">
                <label class="check"><input type="checkbox" prop:checked=automatic on:change=move |event| toggle(event_target_checked(&event)) />"Automatic"</label>
                <span>{detail}</span>
            </div>
        </div>
    </Show> }
}
