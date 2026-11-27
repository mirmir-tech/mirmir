use super::{Listing, Sizes, estimated, exact, variant};

/// A refreshed search invalidates any in-flight listing so a late reply cannot
/// repopulate withdrawn variants or a repo that became non-GGUF.
#[test]
fn refresh_drops_withdrawn_variants_and_invalidates_stale_listings() {
    let mut quantizations = estimated();
    quantizations.pick("Q8_0");
    let stale = quantizations.begin_listing().expect("listing starts");
    quantizations.refresh(Vec::new(), None, false);
    assert_eq!(quantizations.sizes(), Sizes::Estimated);
    assert_eq!(quantizations.target("demo/model"), "demo/model");
    assert_eq!(quantizations.listed(stale, exact(), false), Listing::Stale);
    assert!(!quantizations.listing_failed(stale));
    assert_eq!(quantizations.target("demo/model"), "demo/model");

    quantizations.refresh(
        vec![variant("Q4_K_M", "estimated", 4, "fits")],
        Some("Q4_K_M".to_owned()),
        false,
    );
    quantizations.pick("Q4_K_M");
    quantizations.refresh(
        vec![variant("Q4_K_M", "estimated", 4, "fits"), variant("Q8_0", "estimated", 7, "fits")],
        Some("Q8_0".to_owned()),
        false,
    );
    assert_eq!(quantizations.target("demo/model"), "demo/model:Q4_K_M");
}
