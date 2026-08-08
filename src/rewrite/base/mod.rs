//! Base NPCS rewriting.
//!
//! Implements NPCS 2024, Definition 4.2.

mod empty;

use crate::model::CoreQuery;
use crate::plan::RewriteFragment;

use super::context::RewriteContext;

/// Rewrites a query using the base NPCS rewriting algorithm.
pub fn rewrite(query: &CoreQuery) -> RewriteFragment {
    let mut context = RewriteContext::new();

    match query {
        CoreQuery::Empty => empty::rewrite(&mut context),

        _ => todo!("remaining NPCS base rewriting rules"),
    }
}
