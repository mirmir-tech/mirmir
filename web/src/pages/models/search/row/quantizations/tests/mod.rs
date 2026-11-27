mod refresh;

use serde_json::json;

use super::{super::pull_body, Listing, Quantizations, Sizes};
use crate::types::catalog::{CatalogVariant, ModelVariants};

const GIB: u64 = 1 << 30;

fn variant(name: &str, size_kind: &str, gib: u64, memory_fit: &str) -> CatalogVariant {
    CatalogVariant {
        name: name.to_owned(),
        size_kind: size_kind.to_owned(),
        weight_bytes: Some(gib * GIB),
        memory_fit: memory_fit.to_owned(),
    }
}

/// The search estimated `Q8_0` as the default.
fn estimated() -> Quantizations {
    Quantizations::estimated(
        vec![variant("Q4_K_M", "estimated", 4, "fits"), variant("Q8_0", "estimated", 7, "fits")],
        Some("Q8_0".to_owned()),
    )
}

/// The exact listing finds `Q8_0` too large and defaults to `Q4_K_M`.
fn exact() -> ModelVariants {
    ModelVariants {
        variants: vec![
            variant("Q4_K_M", "exact", 5, "fits"),
            variant("Q8_0", "exact", 9, "does_not_fit"),
        ],
        default_variant: Some("Q4_K_M".to_owned()),
    }
}

fn chosen(quantizations: &Quantizations) -> Option<&str> {
    quantizations.chosen().map(|variant| variant.name.as_str())
}

/// Without a pick, the listing's default replaces the estimate's, and the
/// download requests the variant the select shows.
#[test]
fn the_exact_default_replaces_the_estimated_one() {
    let mut quantizations = estimated();
    assert_eq!(chosen(&quantizations), Some("Q8_0"));
    let generation = quantizations.begin_listing().expect("listing starts");
    assert_eq!(quantizations.listed(generation, exact(), false), Listing::Taken);

    assert_eq!(chosen(&quantizations), Some("Q4_K_M"));
    assert_eq!(quantizations.chosen().map(|variant| variant.size_kind.as_str()), Some("exact"));
    assert_eq!(
        pull_body(&quantizations.target("demo/model")),
        json!({"repo_id": "demo/model:Q4_K_M", "revision": null})
    );
}

/// A pick outlasts the listing, before or after it answers.
#[test]
fn a_pick_outlasts_the_listing() {
    let mut quantizations = estimated();
    quantizations.pick("Q8_0");
    let generation = quantizations.begin_listing().expect("listing starts");
    assert_eq!(quantizations.listed(generation, exact(), false), Listing::Taken);
    assert_eq!(quantizations.target("demo/model"), "demo/model:Q8_0");

    quantizations.pick("Q4_K_M");
    assert_eq!(quantizations.target("demo/model"), "demo/model:Q4_K_M");
    quantizations.pick("IQ1_S");
    assert_eq!(chosen(&quantizations), Some("Q4_K_M"), "an unlisted name is not a pick");
}

/// A pick the listing no longer has falls back to the listing's default.
#[test]
fn a_pick_the_listing_drops_falls_back_to_its_default() {
    let mut quantizations = Quantizations::estimated(
        vec![variant("Q2_K", "estimated", 2, "fits"), variant("Q4_K_M", "estimated", 4, "fits")],
        Some("Q4_K_M".to_owned()),
    );
    quantizations.pick("Q2_K");
    let generation = quantizations.begin_listing().expect("listing starts");
    assert_eq!(
        quantizations.listed(generation, exact(), false),
        Listing::Withdrawn("Q2_K".to_owned())
    );
    assert_eq!(chosen(&quantizations), Some("Q4_K_M"));
}

/// A download pins the variant it takes: the listing's default does not
/// replace it, and only an offered pick goes unreported.
#[test]
fn a_download_pins_its_variant() {
    let mut quantizations = estimated();
    let generation = quantizations.begin_listing().expect("listing starts");
    quantizations.pin_chosen();
    assert_eq!(quantizations.listed(generation, exact(), false), Listing::Taken);
    assert_eq!(quantizations.target("demo/model"), "demo/model:Q8_0");

    let mut unpicked = estimated();
    let generation = unpicked.begin_listing().expect("listing starts");
    assert_eq!(unpicked.listed(generation, exact(), false), Listing::Taken);
}

/// The select and the download agree even without a default.
#[test]
fn without_a_default_the_first_variant_is_chosen() {
    let quantizations =
        Quantizations::estimated(vec![variant("Q4_K_M", "estimated", 4, "fits")], None);
    assert_eq!(quantizations.target("demo/model"), "demo/model:Q4_K_M");
    assert_eq!(Quantizations::estimated(Vec::new(), None).target("demo/model"), "demo/model");
}

/// One listing at a time; a failed one is retried by the next look, a
/// successful one is not repeated.
#[test]
fn a_failed_listing_is_retried() {
    let mut quantizations = estimated();
    let first = quantizations.begin_listing().expect("listing starts");
    assert!(quantizations.begin_listing().is_none(), "one listing in flight");
    assert!(quantizations.listing_failed(first));
    assert_eq!(quantizations.sizes(), Sizes::Estimated);
    assert_eq!(chosen(&quantizations), Some("Q8_0"));

    let second = quantizations.begin_listing().expect("the next look retries");
    assert_eq!(quantizations.listed(second, exact(), false), Listing::Taken);
    assert_eq!(quantizations.sizes(), Sizes::Listed);
    assert!(quantizations.begin_listing().is_none());
    assert!(quantizations.listing_failed(second));
    assert_eq!(quantizations.sizes(), Sizes::Listed, "a listed row stays listed");
}

/// A variant being downloaded that the listing withdraws stays chosen and
/// is shown as not offered, until its download ends; one no longer
/// downloading falls back at once.
#[test]
fn a_withdrawn_download_is_kept() {
    let only_q4 = || ModelVariants {
        variants: vec![variant("Q4_K_M", "exact", 5, "fits")],
        default_variant: Some("Q4_K_M".to_owned()),
    };
    let mut quantizations = estimated();
    let generation = quantizations.begin_listing().expect("listing starts");
    quantizations.pin_chosen();
    assert_eq!(
        quantizations.listed(generation, only_q4(), true),
        Listing::Kept("Q8_0".to_owned())
    );
    assert_eq!(quantizations.target("demo/model"), "demo/model:Q8_0");
    let options = quantizations.options();
    let offered = options.iter().map(|option| (option.variant.name.as_str(), option.offered));
    assert_eq!(offered.collect::<Vec<_>>(), [("Q4_K_M", true), ("Q8_0", false)]);
    quantizations.pick("Q8_0");
    assert_eq!(chosen(&quantizations), Some("Q8_0"));

    let mut ended = quantizations.clone();
    ended.release_kept();
    assert_eq!(ended.target("demo/model"), "demo/model:Q4_K_M");
    assert_eq!(ended.options().len(), 1);
    ended.pick("Q8_0");
    assert_eq!(chosen(&ended), Some("Q4_K_M"), "a withdrawn name is not a pick");

    quantizations.pin_chosen();
    assert_eq!(quantizations.target("demo/model"), "demo/model:Q4_K_M");

    let mut picked = estimated();
    let generation = picked.begin_listing().expect("listing starts");
    picked.pin_chosen();
    picked.listed(generation, only_q4(), true);
    picked.pick("Q4_K_M");
    assert!(picked.kept().is_none() && picked.options().len() == 1);

    let mut idle = estimated();
    let generation = idle.begin_listing().expect("listing starts");
    idle.pin_chosen();
    assert_eq!(idle.listed(generation, only_q4(), false), Listing::Withdrawn("Q8_0".to_owned()));
    assert_eq!(idle.options().len(), 1);
}

/// An empty listing keeps the estimates and the choice, like a failed one.
#[test]
fn an_empty_listing_keeps_the_estimates() {
    let mut quantizations = estimated();
    let generation = quantizations.begin_listing().expect("listing starts");
    let empty = ModelVariants {
        variants: Vec::new(),
        default_variant: None,
    };
    assert_eq!(quantizations.listed(generation, empty, false), Listing::Empty);
    assert_eq!(quantizations.sizes(), Sizes::Estimated);
    assert_eq!(quantizations.target("demo/model"), "demo/model:Q8_0");
    assert!(quantizations.begin_listing().is_some(), "the next look lists again");
}

fn kept_q8() -> Quantizations {
    let mut quantizations = estimated();
    let generation = quantizations.begin_listing().expect("listing starts");
    quantizations.pin_chosen();
    let only_q4 = ModelVariants {
        variants: vec![variant("Q4_K_M", "exact", 5, "fits")],
        default_variant: Some("Q4_K_M".to_owned()),
    };
    assert_eq!(
        quantizations.listed(generation, only_q4, true),
        Listing::Kept("Q8_0".to_owned())
    );
    quantizations
}

/// Releasing a kept variant falls back to the listing's default and drops
/// its option; releasing without one changes nothing.
#[test]
fn release_kept_falls_back_to_the_default() {
    let mut quantizations = kept_q8();
    quantizations.release_kept();
    assert!(quantizations.kept().is_none());
    assert_eq!(quantizations.target("demo/model"), "demo/model:Q4_K_M");
    assert_eq!(quantizations.options().len(), 1);

    quantizations.pick("Q4_K_M");
    quantizations.release_kept();
    assert_eq!(quantizations.target("demo/model"), "demo/model:Q4_K_M");
}

/// A download pins only offered variants: it releases a kept one first.
#[test]
fn pin_chosen_releases_a_kept_variant() {
    let mut quantizations = kept_q8();
    quantizations.pin_chosen();
    assert!(quantizations.kept().is_none());
    assert_eq!(quantizations.target("demo/model"), "demo/model:Q4_K_M");
}
