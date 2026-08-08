use crate::model::CoreQuery;
use crate::plan::{PlanExpr, PlanVar, RewritePlan};

use super::{and, union};
use crate::rewrite::context::RewriteContext;

/// Returns whether a query is a sum-query according to Definition 4.5.
///
/// Definition 4.2 makes triple patterns, AND, UNION, and SELECT
/// sum-queries.
pub(super) fn is_sum_query(query: &CoreQuery) -> bool {
    matches!(
        query,
        CoreQuery::Triple(_)
            | CoreQuery::And(_, _)
            | CoreQuery::Union(_, _)
            | CoreQuery::Select { .. }
    )
}

/// Builds the pattern `T` of a sum-query.
///
/// `provenance` is the variable through which the pattern exposes the
/// provenance value consumed by the outer `ProvAggSum`.
///
/// Optimized patterns are used whenever a Definition 4.6 rule applies.
/// Otherwise the corresponding Definition 4.2 pattern is constructed
/// while recursive operands still pass through the optimized dispatcher.
pub(super) fn pattern(
    query: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    debug_assert!(is_sum_query(query));

    if and::is_applicable(query) {
        return and::pattern(query, provenance, context);
    }

    if union::is_applicable(query) {
        return union::pattern(query, provenance, context);
    }

    base_pattern(query, provenance, context)
}

/// Builds the Definition 4.2 pattern of a sum-query without its
/// outer provenance aggregation.
fn base_pattern(
    query: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    match query {
        CoreQuery::Triple(pattern) => RewritePlan::ReifiedTriple {
            pattern: pattern.clone(),
            source: provenance.clone(),
        },

        CoreQuery::And(left, right) => {
            let left_provenance = context.fresh_var();

            let left = super::rewrite_with_context(left, &left_provenance, context);

            let right_provenance = context.fresh_var();

            let right = super::rewrite_with_context(right, &right_provenance, context);

            RewritePlan::Extend {
                input: Box::new(RewritePlan::Join(vec![left, right])),
                expression: PlanExpr::Product(vec![
                    PlanExpr::Var(left_provenance),
                    PlanExpr::Var(right_provenance),
                ]),
                variable: provenance.clone(),
            }
        }

        CoreQuery::Union(left, right) => {
            let left = super::rewrite_with_context(left, provenance, context);

            let right = super::rewrite_with_context(right, provenance, context);

            RewritePlan::Union(vec![left, right])
        }

        CoreQuery::Select { input, .. } => super::rewrite_with_context(input, provenance, context),

        CoreQuery::Empty
        | CoreQuery::Diff(_, _)
        | CoreQuery::Filter { .. }
        | CoreQuery::Bind { .. } => {
            unreachable!("only Definition 4.5 sum-queries have patterns")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ObjectPattern, PredicatePattern, QueryExprId, SubjectPattern, TriplePattern,
    };
    use oxrdf::{NamedNode, Variable};

    fn triple() -> CoreQuery {
        CoreQuery::Triple(TriplePattern {
            subject: SubjectPattern::NamedNode(
                NamedNode::new("https://example.com/s").expect("valid IRI"),
            ),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("https://example.com/p").expect("valid IRI"),
            ),
            object: ObjectPattern::NamedNode(
                NamedNode::new("https://example.com/o").expect("valid IRI"),
            ),
        })
    }

    #[test]
    fn identifies_definition_4_5_sum_queries() {
        let triple = triple();

        assert!(is_sum_query(&triple));

        assert!(is_sum_query(&CoreQuery::And(
            Box::new(triple.clone(),),
            Box::new(triple.clone(),),
        )));

        assert!(is_sum_query(&CoreQuery::Union(
            Box::new(triple.clone(),),
            Box::new(triple.clone(),),
        )));

        assert!(is_sum_query(&CoreQuery::Select {
            variables: vec![],
            input: Box::new(triple.clone(),),
        }));

        assert!(!is_sum_query(&CoreQuery::Empty));

        assert!(!is_sum_query(&CoreQuery::Diff(
            Box::new(triple.clone(),),
            Box::new(triple.clone(),),
        )));

        assert!(!is_sum_query(&CoreQuery::Filter {
            input: Box::new(triple.clone(),),
            condition: QueryExprId::new(1),
        }));

        assert!(!is_sum_query(&CoreQuery::Bind {
            input: Box::new(triple),
            expression: QueryExprId::new(2),
            variable: Variable::new("x").expect("valid variable",),
        }));
    }
}
