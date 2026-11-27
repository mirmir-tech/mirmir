//! A GGUF repository's quantizations in one catalog row: their sizes, which
//! the search estimates until the files are listed, and the one variant the
//! row shows, sizes and downloads.

#[cfg(test)]
mod tests;

use crate::types::catalog::{CatalogVariant, ModelVariants};

/// Where the variants and their sizes come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sizes {
    /// The search's estimates. The next look at the select lists the
    /// files, also after a listing failed.
    Estimated,
    /// The files are being listed.
    Listing,
    /// The server listed the files; each variant's `size_kind` says whether
    /// its size is exact, estimated or unknown.
    Listed,
}

/// What a listing did to the row.
#[derive(Debug, PartialEq, Eq)]
pub enum Listing {
    /// A newer refresh or listing superseded this reply; nothing changed.
    Stale,
    /// Its variants replaced the estimates.
    Taken,
    /// It offered no variants; the estimates stay and the next look
    /// lists again.
    Empty,
    /// It no longer offers the picked variant, so the row chose its
    /// default instead.
    Withdrawn(String),
    /// It no longer offers the variant being downloaded, which the row
    /// keeps showing, and downloading, as not offered.
    Kept(String),
}

/// One option of the select.
#[derive(Clone)]
pub struct Choice {
    pub variant: CatalogVariant,
    /// False for a variant kept while downloading that the listing no
    /// longer offers.
    pub offered: bool,
}

/// The variant the user chose rather than the default.
#[derive(Clone)]
enum Pick {
    /// Picked in the select or downloaded, by the name of one of the
    /// offered variants; it outlasts a listing that still offers it.
    Offered(String),
    /// Being downloaded when the listing withdrew it: shown, and not
    /// offered, only while its download runs or once it is downloaded.
    Kept(CatalogVariant),
}

#[derive(Clone)]
pub struct Quantizations {
    variants: Vec<CatalogVariant>,
    /// The server's choice for whoever picks none; a listing replaces it.
    default: Option<String>,
    pick: Option<Pick>,
    sizes: Sizes,
    /// Incremented on each listing and search refresh so late replies from an
    /// earlier search or listing epoch are dropped.
    generation: u64,
}

impl Quantizations {
    /// The variants a search estimated, with its default.
    pub const fn estimated(variants: Vec<CatalogVariant>, default: Option<String>) -> Self {
        Self {
            variants,
            default,
            pick: None,
            sizes: Sizes::Estimated,
            generation: 0,
        }
    }

    /// The select's options: the offered variants, then a kept one.
    pub fn options(&self) -> Vec<Choice> {
        let offered = self
            .variants
            .iter()
            .map(|variant| Choice { variant: variant.clone(), offered: true });
        let kept = self.kept().map(|variant| Choice { variant: variant.clone(), offered: false });
        offered.chain(kept).collect()
    }

    pub const fn sizes(&self) -> Sizes {
        self.sizes
    }

    /// The chosen variant when the listing no longer offers it.
    pub const fn kept(&self) -> Option<&CatalogVariant> {
        match &self.pick {
            Some(Pick::Kept(variant)) => Some(variant),
            _ => None,
        }
    }

    fn offered(&self, name: &str) -> Option<&CatalogVariant> {
        self.variants.iter().find(|variant| variant.name == name)
    }

    /// The variant the select shows and a download takes: the picked one,
    /// else the default, else the first.
    pub fn chosen(&self) -> Option<&CatalogVariant> {
        let picked = match &self.pick {
            Some(Pick::Kept(variant)) => Some(variant),
            Some(Pick::Offered(name)) => self.offered(name),
            None => None,
        };
        picked
            .or_else(|| self.default.as_deref().and_then(|name| self.offered(name)))
            .or_else(|| self.variants.first())
    }

    /// What a download of the chosen variant requests: `repo:name`, or
    /// the repository when it has no variants.
    pub fn target(&self, repo: &str) -> String {
        self.chosen()
            .map_or_else(|| repo.to_owned(), |variant| format!("{repo}:{}", variant.name))
    }

    /// Picks the offered variant `name` in the select; any other name,
    /// a kept one too, is ignored.
    pub fn pick(&mut self, name: &str) {
        if self.offered(name).is_some() {
            self.pick = Some(Pick::Offered(name.to_owned()));
        }
    }

    /// Pins the chosen variant as picked, as a download of it does, so a
    /// listing's default cannot replace it. A download takes only offered
    /// variants: a kept one is released first.
    pub fn pin_chosen(&mut self) {
        self.release_kept();
        self.pick = self.chosen().map(|variant| Pick::Offered(variant.name.clone()));
    }

    /// Drops a kept variant, whose download ended without it, for the
    /// default.
    pub fn release_kept(&mut self) {
        if self.kept().is_some() {
            self.pick = None;
        }
    }

    /// Starts listing the files unless they are listed or being listed;
    /// returns the listing generation token when a request should be sent.
    pub fn begin_listing(&mut self) -> Option<u64> {
        if self.sizes != Sizes::Estimated {
            return None;
        }
        self.sizes = Sizes::Listing;
        self.generation += 1;
        Some(self.generation)
    }

    /// Takes the listed variants and the default they imply, unless a newer
    /// refresh superseded `generation` or the listing is empty. A pick the
    /// listing no longer offers falls back to that default, unless
    /// `downloading` says the chosen variant is being downloaded: then the row
    /// keeps it.
    pub fn listed(
        &mut self,
        generation: u64,
        listing: ModelVariants,
        downloading: bool,
    ) -> Listing {
        if generation != self.generation {
            return Listing::Stale;
        }
        if listing.variants.is_empty() {
            self.listing_failed(generation);
            return Listing::Empty;
        }
        let chosen = self.chosen().cloned();
        self.variants = listing.variants;
        self.default = listing.default_variant;
        self.sizes = Sizes::Listed;
        let name = match &self.pick {
            Some(Pick::Offered(name)) => name.clone(),
            Some(Pick::Kept(variant)) => variant.name.clone(),
            None => return Listing::Taken,
        };
        if self.offered(&name).is_some() {
            self.pick = Some(Pick::Offered(name));
            return Listing::Taken;
        }
        if let Some(variant) = chosen.filter(|variant| downloading && variant.name == name) {
            self.pick = Some(Pick::Kept(variant));
            Listing::Kept(name)
        } else {
            self.pick = None;
            Listing::Withdrawn(name)
        }
    }

    /// Keeps the estimates after a failed listing; returns whether `generation`
    /// is still current so stale failures go unreported.
    pub fn listing_failed(&mut self, generation: u64) -> bool {
        if generation != self.generation {
            return false;
        }
        if self.sizes == Sizes::Listing {
            self.sizes = Sizes::Estimated;
        }
        true
    }

    /// Replaces the row's search estimates when a new search returns the same
    /// repository with updated variants or default. Keeps a picked variant
    /// that the new search still offers, or one currently downloading as not
    /// offered, and drops any other withdrawn pick so a download never
    /// submits a variant absent from the latest response.
    pub fn refresh(
        &mut self,
        variants: Vec<CatalogVariant>,
        default: Option<String>,
        downloading: bool,
    ) {
        let chosen = self.chosen().cloned();
        self.variants = variants;
        self.default = default;
        self.sizes = Sizes::Estimated;
        self.generation += 1;
        let name = match &self.pick {
            Some(Pick::Offered(name)) => name.clone(),
            Some(Pick::Kept(variant)) => variant.name.clone(),
            None => return,
        };
        if self.offered(&name).is_some() {
            self.pick = Some(Pick::Offered(name));
        } else if let Some(variant) = chosen.filter(|variant| downloading && variant.name == name) {
            self.pick = Some(Pick::Kept(variant));
        } else {
            self.pick = None;
        }
    }
}
