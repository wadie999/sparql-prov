//! Base NPCS rewriting.
//!
//! Implements NPCS 2024, Definition 4.2.

mod aggregate;
mod empty;
mod triple;

use crate::model::CoreQuery;
use crate::plan::RewriteFragment;

use super::context::RewriteContext;

/// Rewrites a query using the base NPCS rewriting algorithm.
pub fn rewrite(query: &CoreQuery) -> RewriteFragment {
    let mut context = RewriteContext::new();

    match query {
        CoreQuery::Empty => empty::rewrite(&mut context),

        CoreQuery::Triple(pattern) => triple::rewrite(pattern, &mut context),

        _ => todo!("remaining NPCS base rewriting rules"),
    }
}
