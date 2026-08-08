//! Base NPCS rewriting.
//!
//! Implements NPCS 2024, Definition 4.2.

mod aggregate;
mod and;
mod empty;
mod triple;
mod union;

use crate::model::CoreQuery;
use crate::plan::RewriteFragment;

use super::context::RewriteContext;
use super::scope::in_scope;

/// Rewrites a query using the base NPCS rewriting algorithm.
pub fn rewrite(query: &CoreQuery) -> RewriteFragment {
    let mut context = RewriteContext::new();

    rewrite_with_context(query, &mut context)
}

pub(super) fn rewrite_with_context(
    query: &CoreQuery,
    context: &mut RewriteContext,
) -> RewriteFragment {
    match query {
        CoreQuery::Empty => empty::rewrite(context),

        CoreQuery::Triple(pattern) => triple::rewrite(pattern, context),

        CoreQuery::And(left, right) => {
            let variables = in_scope(query);

            and::rewrite(left, right, &variables, context)
        }

        CoreQuery::Union(left, right) => {
            let variables = in_scope(query);

            union::rewrite(left, right, &variables, context)
        }

        _ => todo!("remaining NPCS base rewriting rules"),
    }
}
