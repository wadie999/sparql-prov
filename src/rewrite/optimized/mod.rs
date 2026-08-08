//! Optimized NPCS rewriting.
//!
//! Implements the aggregation-reducing rewriting from NPCS 2024,
//! Definition 4.6.

mod aggregate;
mod and;
mod diff;
mod fallback;
mod select;
mod sum_query;
mod union;

use crate::model::CoreQuery;
use crate::plan::{PlanVar, RewriteFragment, RewritePlan};

use super::context::RewriteContext;

/// Rewrites a query using the optimized NPCS strategy.
pub fn rewrite(query: &CoreQuery) -> RewriteFragment {
    let mut context = RewriteContext::new();

    let provenance = context.fresh_var();

    let plan = rewrite_with_context(query, &provenance, &mut context);

    RewriteFragment { plan, provenance }
}

/// Implements the recursive optimized rewriting `β(Q, z)`.
///
/// Definition 4.6 rules are preferred whenever applicable.
/// Otherwise one level of Definition 4.2 is applied, whose
/// operands recurse through this dispatcher again.
pub(super) fn rewrite_with_context(
    query: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    if and::is_applicable(query) {
        return and::rewrite(query, provenance, context);
    }

    if union::is_applicable(query) {
        return union::rewrite(query, provenance, context);
    }

    if diff::is_applicable(query) {
        return diff::rewrite(query, provenance, context);
    }

    if select::is_applicable(query) {
        return select::rewrite(query, provenance, context);
    }

    fallback::rewrite(query, provenance, context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ObjectPattern, PredicatePattern, QueryExprId, SubjectPattern, TriplePattern,
    };
    use crate::plan::RewritePlan;
    use oxrdf::NamedNode;

    fn triple(suffix: &str) -> CoreQuery {
        CoreQuery::Triple(TriplePattern {
            subject: SubjectPattern::NamedNode(
                NamedNode::new(format!("https://example.com/s{suffix}")).expect("valid IRI"),
            ),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("https://example.com/p").expect("valid IRI"),
            ),
            object: ObjectPattern::NamedNode(
                NamedNode::new(format!("https://example.com/o{suffix}")).expect("valid IRI"),
            ),
        })
    }

    #[test]
    fn applies_optimized_rules_below_fallback_nodes() {
        let query = CoreQuery::Filter {
            input: Box::new(CoreQuery::And(Box::new(triple("1")), Box::new(triple("2")))),
            condition: QueryExprId::new(1),
        };

        let fragment = rewrite(&query);

        let RewritePlan::Filter { input, .. } = fragment.plan else {
            panic!("FILTER must be preserved");
        };

        let RewritePlan::Select { input, .. } = *input else {
            panic!("inner optimized AND must have one outer SELECT");
        };

        let RewritePlan::Extend { input, .. } = *input else {
            panic!("optimized AND must bind one provenance product");
        };

        let RewritePlan::Join(inputs) = *input else {
            panic!("optimized AND must join its operand patterns");
        };

        assert_eq!(inputs.len(), 2);

        assert!(
            inputs
                .iter()
                .all(|input| { matches!(input, RewritePlan::ReifiedTriple { .. }) })
        );
    }
}
