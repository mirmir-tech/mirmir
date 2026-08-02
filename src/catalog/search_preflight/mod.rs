use std::{collections::HashSet, future::Future, time::Duration};

use futures_util::{StreamExt, stream};

use super::{Catalog, CatalogModel, preflight};

const CONCURRENCY: usize = 4;
const SEARCH_BUDGET: Duration = Duration::from_secs(4);

pub(super) async fn enrich(catalog: &Catalog, models: &mut [CatalogModel]) {
    let candidates = models
        .iter()
        .enumerate()
        .filter(|(_, model)| {
            model.local_source == "remote"
                && model.compatibility == "unknown"
                && model.container == "SafeTensors"
        })
        .map(|(index, model)| (index, model.id.clone()))
        .collect::<Vec<_>>();
    let mut remaining = candidates.iter().map(|(index, _)| *index).collect::<HashSet<_>>();
    let requests =
        candidates
            .into_iter()
            .map(|(index, repo_id)| async move {
                (index, catalog.preflight_headers(&repo_id, None).await)
            })
            .collect();

    for (index, result) in run_bounded(requests, CONCURRENCY, SEARCH_BUDGET).await {
        let _removed = remaining.remove(&index);
        match result {
            Ok(result) => preflight::apply_to_model(&mut models[index], &result),
            Err(error) => preflight::apply_failure(&mut models[index], error.to_string()),
        }
    }
    for index in remaining {
        preflight::apply_failure(
            &mut models[index],
            "inspection exceeded the four-second catalog search budget",
        );
    }
}

async fn run_bounded<F, T>(requests: Vec<F>, concurrency: usize, budget: Duration) -> Vec<T>
where
    F: Future<Output = T>,
{
    let pending = stream::iter(requests).buffer_unordered(concurrency);
    futures_util::pin_mut!(pending);
    let deadline = tokio::time::sleep(budget);
    tokio::pin!(deadline);
    let mut completed = Vec::new();
    loop {
        tokio::select! {
            Some(result) = pending.next() => completed.push(result),
            () = &mut deadline => break,
            else => break,
        }
    }
    completed
}

#[cfg(test)]
mod tests;
