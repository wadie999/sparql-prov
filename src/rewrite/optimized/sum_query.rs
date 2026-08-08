use crate::model::CoreQuery;
use crate::plan::{PlanExpr, PlanVar, RewritePlan, SelectItem};

use super::and;
use crate::rewrite::context::RewriteContext;

/// The pattern `T` of a sum-query rewriting from Definition 4.5.
///
/// `pattern` is the body below the outer provenance aggregation.
/// `provenance` is the variable bound by that pattern and consumed
/// by the removed `ProvAggSum`.
pub(super) struct SumPattern {
    pub(super) pattern: RewritePlan,
    pub(super) provenance: PlanVar,
}

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

/// Returns the pattern of a sum-query.
///
/// When an optimized rule applies, its pattern is constructed directly.
/// Otherwise the query is rewritten through the optimized dispatcher and
/// its outer provenance aggregation is removed.
pub(super) fn pattern(query: &CoreQuery, context: &mut RewriteContext) -> SumPattern {
    debug_assert!(is_sum_query(query));

    if and::is_applicable(query) {
        return and::pattern(query, context);
    }

    rewritten_pattern(query, context)
}

fn rewritten_pattern(query: &CoreQuery, context: &mut RewriteContext) -> SumPattern {
    let output = context.fresh_var();

    let rewritten = super::rewrite_with_context(query, &output, context);

    let RewritePlan::Select { input, items, .. } = rewritten else {
        unreachable!("a Definition 4.5 sum-query must rewrite to SELECT");
    };

    let provenance = items
        .iter()
        .find_map(|item| match item {
            SelectItem::Expr {
                expression: PlanExpr::AggregateSum(expression),
                alias,
            } if alias == &output => match expression.as_ref() {
                PlanExpr::Var(variable) => Some(variable.clone()),
                _ => None,
            },

            _ => None,
        })
        .expect("a Definition 4.5 sum-query must aggregate provenance");

    SumPattern {
        pattern: *input,
        provenance,
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
