//! The select of a GGUF repository's quantizations.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::{
    super::installed,
    pulling,
    quantizations::{Choice, Listing, Quantizations, Sizes},
    size_label,
};
use crate::{api, state::RuntimeState, types::catalog::ModelVariants};

/// The search estimates the variants' sizes from the parameter count; a
/// look at the select lists the files once, with their sizes, and looks
/// again after a listing failed. Each option is selected from the row's
/// resolved choice, so replacing the options cannot show another variant
/// than the one a download takes; an option is keyed by everything its
/// label shows.
#[component]
pub(super) fn VariantSelect(repo: String, quantizations: RwSignal<Quantizations>) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let path = format!("/catalog/variants?repo_id={}", urlencoding::encode(&repo));
    let named = repo.clone();
    let list = move |_| {
        let Some(generation) = quantizations
            .try_maybe_update(|item| {
                let generation = item.begin_listing();
                (generation.is_some(), generation)
            })
            .flatten()
        else {
            return;
        };
        let (path, repo) = (path.clone(), named.clone());
        spawn_local(async move {
            // A row the search has replaced or refreshed takes no stale answer.
            match api::get::<ModelVariants>(&path).await {
                Ok(listing) => {
                    let listed = quantizations.try_update(|item| {
                        let downloading = pulling(state, &item.target(&repo));
                        (item.listed(generation, listing, downloading), item.target(&repo))
                    });
                    if let Some((listed, target)) = listed {
                        report(state, &repo, listed, &target);
                    }
                },
                Err(error) => {
                    if quantizations
                        .try_update(|item| item.listing_failed(generation))
                        .unwrap_or(false)
                    {
                        state.notify(error, true);
                    }
                },
            }
        });
    };
    let chosen =
        move || quantizations.with(|item| item.chosen().map(|variant| variant.name.clone()));
    let listing = move || quantizations.with(|item| item.sizes() == Sizes::Listing);
    let options = move || quantizations.with(Quantizations::options);
    view! {
        <select class="variant-select" aria-label="Quantization" aria-busy=move || listing().to_string() data-tooltip="Quantization to download" on:focus=list on:change=move |event| quantizations.update(|item| item.pick(&event_target_value(&event)))>
            <For each=options key=|option| (option.variant.name.clone(), option.variant.size_kind.clone(), option.variant.weight_bytes, option.offered) children=move |option| {
                let Choice { variant, offered } = option;
                let target = format!("{repo}:{}", variant.name);
                let withdrawn = if offered { "" } else { " · not offered" };
                let label = format!("{} · {}{withdrawn}", variant.name, size_label(&variant));
                let mark = move || if state.models.with(|models| installed(models, &target)) { " ✓" } else { "" };
                let name = variant.name.clone();
                let selected = move || chosen().as_deref() == Some(name.as_str());
                view! { <option value=variant.name disabled=!offered prop:selected=selected>{label}{mark}</option> }
            } />
        </select>
    }
}

/// Tells the user what a listing changed: an empty one is a failure, and a
/// withdrawn pick is replaced, or kept while it downloads.
fn report(state: RuntimeState, repo: &str, listed: Listing, target: &str) {
    match listed {
        Listing::Stale | Listing::Taken => {},
        Listing::Empty => {
            state.notify(format!("{repo} listed no quantizations; sizes stay estimated"), true);
        },
        Listing::Withdrawn(name) => {
            state.notify(format!("{repo}:{name} is no longer offered; chose {target}"), false);
        },
        Listing::Kept(name) => state
            .notify(format!("{repo}:{name} is no longer offered; its download continues"), false),
    }
}
