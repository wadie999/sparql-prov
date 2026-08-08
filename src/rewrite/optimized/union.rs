use crate::model::CoreQuery;
use crate::plan::{PlanVar, RewritePlan};

use super::{aggregate, sum_query};
use crate::rewrite::context::RewriteContext;
use crate::rewrite::scope::in_scope;

/// Returns whether Definition 4.6 Rule 2 applies.
///
/// UNION is treated as variadic by flattening nested UNION nodes,
/// following the associativity observation after Definition 4.6.
pub(super) fn is_applicable(query: &CoreQuery) -> bool {
    if !matches!(query, CoreQuery::Union(_, _)) {
        return false;
    }

    let mut operands = Vec::new();

    collect_operands(query, &mut operands);

    operands.into_iter().all(sum_query::is_sum_query)
}

/// Implements NPCS 2024, Definition 4.6, Rule 2.
pub(super) fn rewrite(
    query: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    let variables = in_scope(query);

    let sum_input = context.fresh_var();

    let pattern = pattern(query, &sum_input, context);

    aggregate::sum_by(pattern, &variables, &sum_input, provenance)
}

/// Builds the optimized UNION pattern.
///
/// Every branch exposes its provenance through the same variable,
/// corresponding to `?z⊕` in Definition 4.6 Rule 2.
pub(super) fn pattern(
    query: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    debug_assert!(is_applicable(query));

    let mut operands = Vec::new();

    collect_operands(query, &mut operands);

    let patterns = operands
        .into_iter()
        .map(|operand| sum_query::pattern(operand, provenance, context))
        .collect();

    RewritePlan::Union(patterns)
}

fn collect_operands<'a>(query: &'a CoreQuery, operands: &mut Vec<&'a CoreQuery>) {
    match query {
        CoreQuery::Union(left, right) => {
            collect_operands(left, operands);

            collect_operands(right, operands);
        }

        _ => operands.push(query),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};
    use crate::plan::{InternalVar, PlanExpr, SelectItem};
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
    fn matches_npcs_definition_4_6_rule_2() {
        let left = triple("1");

        let right = triple("2");

        let query = CoreQuery::Union(Box::new(left.clone()), Box::new(right.clone()));

        let fragment = super::super::rewrite(&query);

        let sum_input = PlanVar::Internal(InternalVar::new(1));

        assert_eq!(
            fragment.plan,
            RewritePlan::Select {
                input: Box::new(RewritePlan::Union(vec![
                    RewritePlan::ReifiedTriple {
                        pattern: match left {
                            CoreQuery::Triple(pattern) => pattern,
                            _ => unreachable!(),
                        },
                        source: sum_input.clone(),
                    },
                    RewritePlan::ReifiedTriple {
                        pattern: match right {
                            CoreQuery::Triple(pattern) => pattern,
                            _ => unreachable!(),
                        },
                        source: sum_input.clone(),
                    },
                ],),),
                items: vec![SelectItem::Expr {
                    expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(sum_input,),),),
                    alias: fragment.provenance.clone(),
                },],
                group_by: vec![],
            }
        );
    }

    #[test]
    fn treats_nested_union_as_variadic() {
        let query = CoreQuery::Union(
            Box::new(CoreQuery::Union(
                Box::new(triple("1")),
                Box::new(triple("2")),
            )),
            Box::new(triple("3")),
        );

        let fragment = super::super::rewrite(&query);

        let RewritePlan::Select { input, .. } = fragment.plan else {
            panic!("optimized UNION must have one outer SELECT");
        };

        let RewritePlan::Union(branches) = *input else {
            panic!("optimized UNION must combine branch patterns directly");
        };

        assert_eq!(branches.len(), 3);

        let sources: Vec<_> = branches
            .iter()
            .map(|branch| match branch {
                RewritePlan::ReifiedTriple { source, .. } => source,
                _ => panic!("triple branch must remain a reified pattern"),
            })
            .collect();

        assert!(sources.windows(2).all(|pair| { pair[0] == pair[1] }));
    }
}
