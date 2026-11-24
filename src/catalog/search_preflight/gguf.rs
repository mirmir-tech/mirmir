//! GGUF repositories in search results: one row per variant (quantization),
//! each with its own size and memory fit, admitted together from the
//! header of the smallest, since the variants share the architecture.

use libmir::RemoteModelContract;

use super::{CONCURRENCY, SEARCH_BUDGET, run_bounded};
use crate::{
    catalog::{
        Catalog, CatalogModel, MachineMemory, fit,
        hub::{GgufRepository, RepoFile},
        preflight, resumable,
    },
    config::HubReference,
    error::Result,
};

/// The repository's variants, and the smallest one's admission.
type Inspection = (GgufRepository, Result<(libmir::Result<RemoteModelContract>, u64)>);

pub(super) async fn expand(
    catalog: &Catalog,
    rows: Vec<CatalogModel>,
    memory: MachineMemory,
) -> Vec<CatalogModel> {
    let requests = rows
        .iter()
        .enumerate()
        .map(|(index, row)| async move { (index, inspect(catalog, &row.id).await) })
        .collect();
    let mut inspected = run_bounded(requests, CONCURRENCY, SEARCH_BUDGET).await;
    inspected.sort_by_key(|(index, _)| *index);
    let mut inspected = inspected.into_iter().peekable();
    let mut expanded = Vec::with_capacity(rows.len());
    for (index, mut row) in rows.into_iter().enumerate() {
        match inspected.next_if(|(done, _)| *done == index) {
            Some((_, Ok((repository, admission)))) if !repository.variants.is_empty() => {
                expanded.extend(variants(catalog, &row, &repository, &admission, memory));
            },
            Some((_, Ok(_))) => {
                preflight::apply_failure(&mut row, "the repository lists no GGUF files");
                expanded.push(row);
            },
            Some((_, Err(error))) => {
                preflight::apply_failure(&mut row, error.to_string());
                expanded.push(row);
            },
            None => {
                preflight::apply_failure(
                    &mut row,
                    "listing variants exceeded the four-second catalog search budget",
                );
                expanded.push(row);
            },
        }
    }
    expanded
}

async fn inspect(catalog: &Catalog, repo_id: &str) -> Result<Inspection> {
    let listed = resumable::listing(&catalog.store, repo_id, "main").await?;
    let files = listed
        .into_iter()
        .map(|file| RepoFile { path: file.path, size: file.size })
        .collect::<Vec<_>>();
    let repository = GgufRepository::from_files(&files);
    let Some(smallest) = repository.variants.first() else {
        return Ok((repository, Err(crate::error::Error::Config("no GGUF variant".into()))));
    };
    let token = catalog.store.hf_token()?;
    let admission =
        preflight::inspect_gguf(&catalog.client, token.as_deref(), repo_id, "main", smallest).await;
    Ok((repository, admission))
}

fn variants(
    catalog: &Catalog,
    row: &CatalogModel,
    repository: &GgufRepository,
    admission: &Result<(libmir::Result<RemoteModelContract>, u64)>,
    memory: MachineMemory,
) -> Vec<CatalogModel> {
    let vision = repository.vision.as_ref().map_or(0, |tower| tower.size);
    repository
        .variants
        .iter()
        .filter(|variant| format!("{}:{}", row.id, variant.name).parse::<HubReference>().is_ok())
        .map(|variant| {
            let id = format!("{}:{}", row.id, variant.name);
            let mut model = fit::variant(row, id, &variant.name, variant.bytes() + vision, memory);
            model.downloaded = catalog.store.hub_model_downloaded(&model.id).unwrap_or(false);
            model.local_source = if model.downloaded {
                "mirmir"
            } else {
                "remote"
            };
            match admission {
                Ok((Ok(contract), fetched)) => {
                    preflight::apply_gguf(&mut model, contract, *fetched);
                },
                Ok((Err(error), fetched)) => preflight::reject_gguf(&mut model, error, *fetched),
                Err(error) => preflight::apply_failure(&mut model, error.to_string()),
            }
            model
        })
        .collect()
}
