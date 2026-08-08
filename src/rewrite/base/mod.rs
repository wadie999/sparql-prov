//! Base NPCS rewriting.
//!
//! Implements NPCS 2024, Definition 4.2.

mod aggregate;
mod and;
mod bind;
mod diff;
mod empty;
mod filter;
mod select;
mod triple;
mod union;

use crate::model::CoreQuery;
use crate::plan::{PlanVar, RewriteFragment, RewritePlan};

use super::context::RewriteContext;
use super::scope::in_scope;

/// Rewrites a query using the base NPCS rewriting algorithm.
pub fn rewrite(query: &CoreQuery) -> RewriteFragment {
    let mut context = RewriteContext::new();
    let provenance = context.fresh_var();

    let plan = rewrite_with_context(query, &provenance, &mut context);

    RewriteFragment { plan, provenance }
}

/// Implements the recursive NPCS rewriting function `β(Q, z)`.
pub(super) fn rewrite_with_context(
    query: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    match query {
        CoreQuery::Empty => empty::rewrite(provenance),

        CoreQuery::Triple(pattern) => triple::rewrite(pattern, provenance, context),

        CoreQuery::And(left, right) => {
            let variables = in_scope(query);

            and::rewrite(left, right, &variables, provenance, context)
        }

        CoreQuery::Union(left, right) => {
            let variables = in_scope(query);

            union::rewrite(left, right, &variables, provenance, context)
        }

        CoreQuery::Diff(left, right) => {
            let variables = in_scope(query);

            diff::rewrite(left, right, &variables, provenance, context)
        }

        CoreQuery::Select { variables, input } => {
            select::rewrite(variables, input, provenance, context)
        }

        CoreQuery::Filter { input, condition } => {
            filter::rewrite(input, *condition, provenance, context)
        }

        CoreQuery::Bind {
            input,
            expression,
            variable,
        } => bind::rewrite(input, *expression, variable, provenance, context),
    }
}
