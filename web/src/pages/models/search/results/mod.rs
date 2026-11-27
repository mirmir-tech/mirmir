//! The catalog search table: the results of the last search that answered,
//! the pages appended to them, and the one request that may still change
//! them. Each search or page supersedes every request before it, so a late
//! answer to an older one changes nothing.

#[cfg(test)]
mod tests;

use crate::types::catalog::{CatalogModel, CatalogResults};

/// A request the table issued; only the latest one may settle it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket(u64);

/// The next page of the shown results: the query they answer and its cursor.
#[derive(Debug, PartialEq, Eq)]
pub struct PageRequest {
    pub ticket: Ticket,
    pub query: String,
    pub cursor: String,
}

/// What an answer did to the table.
#[derive(Debug, PartialEq, Eq)]
pub enum Settled {
    /// A later search or page superseded the request; nothing changed.
    Stale,
    Applied,
    /// The latest request failed; the table keeps what it showed.
    Failed(String),
}

/// The latest request the table issued, the only one it waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Latest {
    Waiting(Ticket),
    Answered(Ticket),
}

impl Latest {
    const fn ticket(self) -> Ticket {
        match self {
            Self::Waiting(ticket) | Self::Answered(ticket) => ticket,
        }
    }
}

impl Default for Latest {
    fn default() -> Self {
        Self::Answered(Ticket(0))
    }
}

#[derive(Default)]
pub struct Results {
    /// The query the models answer, trimmed; empty before any search.
    query: String,
    models: Vec<CatalogModel>,
    next_cursor: Option<String>,
    latest: Latest,
}

impl Results {
    pub fn models(&self) -> &[CatalogModel] {
        &self.models
    }

    /// Whether a search or page is in flight.
    pub const fn loading(&self) -> bool {
        matches!(self.latest, Latest::Waiting(_))
    }

    /// Whether the server reported another page after the shown results.
    pub const fn has_more(&self) -> bool {
        self.next_cursor.is_some()
    }

    pub const fn current(&self, ticket: Ticket) -> bool {
        ticket.0 == self.latest.ticket().0
    }

    /// Starts a search for `query`, superseding any request in flight. An
    /// empty query clears the table at once and needs no request.
    pub fn search(&mut self, query: &str) -> Option<Ticket> {
        let ticket = self.issue();
        if query.trim().is_empty() {
            *self = Self {
                latest: Latest::Answered(ticket),
                ..Self::default()
            };
            return None;
        }
        Some(ticket)
    }

    /// Settles the search for `query` that `ticket` started.
    pub fn found(
        &mut self,
        ticket: Ticket,
        query: &str,
        answer: Result<CatalogResults, String>,
    ) -> Settled {
        self.settle(ticket, answer, |results, found| {
            query.trim().clone_into(&mut results.query);
            results.models = found.models;
            results.next_cursor = found.next_cursor;
        })
    }

    /// Requests the page after the shown results, unless the table is
    /// waiting already or has no more.
    pub fn page(&mut self) -> Option<PageRequest> {
        if self.loading() {
            return None;
        }
        let cursor = self.next_cursor.clone()?;
        let ticket = self.issue();
        Some(PageRequest {
            ticket,
            query: self.query.clone(),
            cursor,
        })
    }

    /// Appends the page that `ticket` requested to the results it continues,
    /// skipping models already shown so shifting Hub pages never duplicate
    /// rows.
    pub fn paged(&mut self, ticket: Ticket, answer: Result<CatalogResults, String>) -> Settled {
        self.settle(ticket, answer, |results, page| {
            for model in page.models {
                if !results.models.iter().any(|existing| existing.id == model.id) {
                    results.models.push(model);
                }
            }
            results.next_cursor = page.next_cursor;
        })
    }

    /// Issues the next ticket and waits for it, superseding the latest.
    const fn issue(&mut self) -> Ticket {
        let ticket = Ticket(self.latest.ticket().0 + 1);
        self.latest = Latest::Waiting(ticket);
        ticket
    }

    fn settle(
        &mut self,
        ticket: Ticket,
        answer: Result<CatalogResults, String>,
        apply: impl FnOnce(&mut Self, CatalogResults),
    ) -> Settled {
        if !self.current(ticket) {
            return Settled::Stale;
        }
        self.latest = Latest::Answered(ticket);
        match answer {
            Ok(found) => {
                apply(self, found);
                Settled::Applied
            },
            Err(error) => Settled::Failed(error),
        }
    }
}
