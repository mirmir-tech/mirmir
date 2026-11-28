//! One catalog result: a GGUF repository shows its quantizations in a
//! select, sized and fitted one by one, and downloads the chosen one as
//! `id:name`; other repositories download whole.

mod pull;
mod quantizations;
mod reason;
mod select;
#[cfg(test)]
mod tests;

use leptos::prelude::*;

#[cfg(test)]
use self::pull::pull_body;
use self::{
    pull::{last_pull, pull_activity, pull_ended_without_install, pulling, start_download},
    quantizations::Quantizations,
    reason::{badge_state, separate_fit_label, subtitle, tooltip},
    select::VariantSelect,
};
use super::{super::row::cancel, installed};
use crate::{
    components::{FeaturePills, Icon, StatePill, TypePill},
    state::{RuntimeState, bytes},
    types::catalog::{CatalogModel, CatalogVariant, SizeKind},
};

#[component]
pub(super) fn CatalogRow(#[prop(into)] model: Signal<CatalogModel>) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let initial = model.get_untracked();
    let quantizations = RwSignal::new(Quantizations::estimated(
        initial.variants.clone(),
        initial.default_variant.clone(),
    ));
    let id = initial.id.clone();
    let id_for_select = initial.id;
    #[cfg(test)]
    super::tests::probe::watch(model);
    sync_quantizations(state, model, quantizations, initial.variants, initial.default_variant);
    let gguf = move || model.with(|model| !model.variants.is_empty());
    let format = move || {
        model.with(|model| {
            if model.variants.is_empty() {
                model.encoding.clone().unwrap_or_else(|| model.ecosystem.clone())
            } else {
                "GGUF".to_owned()
            }
        })
    };
    let format_detail =
        move || model.with(|model| format!("{} · {}", model.ecosystem, model.container));
    // The select shows, and a download takes, this one resolved choice.
    let variant = move || quantizations.with(|item| item.chosen().cloned());
    let target = move || model.with(|model| quantizations.with(|item| item.target(&model.id)));
    // A download pins its variant, so a listing that answers later keeps
    // showing the variant being downloaded.
    let pull = move || {
        let latest = model.get_untracked();
        let target = quantizations.try_update(|item| {
            item.pin_chosen();
            item.target(&latest.id)
        });
        if let Some(target) = target {
            start_download(state, &latest, target);
        }
    };
    // From the server's live list, which a pull or removal of any variant
    // changes, not from the search result.
    let downloaded = move || state.models.with(|models| installed(models, &target()));
    let operation = move || pull_activity(state, &target());
    let busy = move || state.busy.get().contains_key(&target());
    // A variant the listing withdrew stays while its download runs and
    // once it completes, even before the models list shows it downloaded;
    // after a failed or cancelled pull, or none, the row takes the default.
    Effect::new(move || {
        let kept = quantizations.with(|item| item.kept().is_some());
        if kept
            && !busy()
            && operation().is_none()
            && !downloaded()
            && pull_ended_without_install(last_pull(state, &target()))
        {
            quantizations.update(Quantizations::release_kept);
        }
    });
    let display_state = move || {
        if operation().is_some() || busy() {
            "downloading".to_owned()
        } else if downloaded() {
            "downloaded".to_owned()
        } else {
            model.with(|model| badge_state(model, variant().as_ref()))
        }
    };
    let row_subtitle = move || model.with(|model| subtitle(model, variant().as_ref()));
    let detail = move || {
        operation().map_or_else(
            || {
                if downloaded() {
                    "Downloaded".to_owned()
                } else {
                    model.with(|model| tooltip(model, variant().as_ref()))
                }
            },
            |item| item.detail,
        )
    };
    let variant_fit = move || model.with(|model| separate_fit_label(model, variant().as_ref()));
    let progress = move || {
        operation().and_then(|item| {
            item.current
                .zip(item.total)
                .map(|(current, total)| 100.0 * current as f64 / total.max(1) as f64)
        })
    };
    let size = move || {
        variant().map_or_else(
            || {
                model
                    .with(|model| model.estimated_weight_bytes)
                    .map_or_else(|| "—".to_owned(), bytes)
            },
            |variant| size_label(&variant),
        )
    };
    view! { <tr><td><span class="model-name"><strong>{id}</strong><small>{row_subtitle}</small></span></td><td>{move || if gguf() { view! { <VariantSelect repo=id_for_select.clone() quantizations=quantizations /> }.into_any() } else { view! { <TypePill value=format() /><small>{format_detail()}</small> }.into_any() }}</td><td class="model-size">{size}{move || variant_fit().map(|fit| view! { <small class="variant-fit">{fit}</small> })}</td><td>{move || model.with(|model| view! { <FeaturePills tool_use=model.tool_use thinking=model.thinking vision=model.vision /> })}</td><td><StatePill state=display_state detail=detail progress=progress /></td><td class="model-actions">
        <Show when=move || operation().is_some_and(|item| item.cancellable) fallback=move || view! {
            <button class="icon-action" class:downloaded=downloaded disabled=move || downloaded() || busy() || quantizations.with(|item| item.kept().is_some()) data-tooltip=move || if downloaded() { "Downloaded".to_owned() } else { format!("Download {}", target()) } aria-label=move || if downloaded() { "Downloaded" } else { "Download model" } on:click=move |_| pull()>{move || view! { <Icon name=if downloaded() { "downloaded" } else { "download" } /> }}</button>
        }>
            <button class="icon-action" data-tooltip="Cancel download" aria-label="Cancel download" on:click=move |_| if let Some(item) = operation() { cancel(state, item.operation_id); }><Icon name="cancel" /></button>
        </Show>
    </td></tr> }
}

/// Refreshes the row's quantizations when a new search updates its variants
/// or default, without clobbering exact sizes when another page appends rows.
fn sync_quantizations(
    state: RuntimeState,
    model: Signal<CatalogModel>,
    quantizations: RwSignal<Quantizations>,
    initial_variants: Vec<CatalogVariant>,
    initial_default: Option<String>,
) {
    let seen = StoredValue::new((initial_variants, initial_default));
    Effect::new(move || {
        let (next_variants, next_default, next_id) = model.with(|model| {
            (model.variants.clone(), model.default_variant.clone(), model.id.clone())
        });
        let changed = seen.with_value(|(prev_variants, prev_default)| {
            prev_variants != &next_variants || prev_default != &next_default
        });
        if changed {
            seen.set_value((next_variants.clone(), next_default.clone()));
            quantizations.update(|item| {
                let downloading = pulling(state, &item.target(&next_id));
                item.refresh(next_variants, next_default, downloading);
            });
        }
    });
}

/// The variant's size; `~` marks one estimated before its files are listed.
pub(super) fn size_label(variant: &CatalogVariant) -> String {
    let estimated = if variant.size_kind == SizeKind::Estimated {
        "~"
    } else {
        ""
    };
    variant
        .weight_bytes
        .map_or_else(|| "—".to_owned(), |weight| format!("{estimated}{}", bytes(weight)))
}
