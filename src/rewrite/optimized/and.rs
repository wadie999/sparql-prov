use crate::model::CoreQuery;
use crate::plan::{PlanExpr, PlanVar, RewritePlan};

use super::{aggregate, sum_query};
use crate::rewrite::context::RewriteContext;
use crate::rewrite::scope::in_scope;

/// Returns whether Definition 4.6 Rule 1 applies.
///
/// AND is treated as variadic by flattening nested AND nodes, following
/// the associativity observation after Definition 4.6.
pub(super) fn is_applicable(query: &CoreQuery) -> bool {
    if !matches!(query, CoreQuery::And(_, _)) {
        return false;
    }

    let mut operands = Vec::new();

    collect_operands(query, &mut operands);

    operands.into_iter().all(sum_query::is_sum_query)
}

/// Implements NPCS 2024, Definition 4.6, Rule 1.
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

/// Builds the optimized pattern `T` for Rule 1.
///
/// Each operand pattern exposes its provenance through a distinct fresh
/// variable. Those provenance values are multiplied into `provenance`.
pub(super) fn pattern(
    query: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    debug_assert!(is_applicable(query));

    let mut operands = Vec::new();

    collect_operands(query, &mut operands);

    let mut patterns = Vec::with_capacity(operands.len());

    let mut provenance_inputs = Vec::with_capacity(operands.len());

    for operand in operands {
        let operand_provenance = context.fresh_var();

        let operand_pattern = sum_query::pattern(operand, &operand_provenance, context);

        patterns.push(operand_pattern);

        provenance_inputs.push(PlanExpr::Var(operand_provenance));
    }

    RewritePlan::Extend {
        input: Box::new(RewritePlan::Join(patterns)),
        expression: PlanExpr::Product(provenance_inputs),
        variable: provenance.clone(),
    }
}

fn collect_operands<'a>(query: &'a CoreQuery, operands: &mut Vec<&'a CoreQuery>) {
    match query {
        CoreQuery::And(left, right) => {
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
    use crate::plan::{InternalVar, SelectItem};
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
    fn matches_npcs_definition_4_6_rule_1() {
        let left = triple("1");
        let right = triple("2");

        let query = CoreQuery::And(Box::new(left.clone()), Box::new(right.clone()));

        let fragment = super::super::rewrite(&query);

        let sum_input = PlanVar::Internal(InternalVar::new(1));

        let left_source = PlanVar::Internal(InternalVar::new(2));

        let right_source = PlanVar::Internal(InternalVar::new(3));

        assert_eq!(
            fragment.plan,
            RewritePlan::Select {
                input: Box::new(RewritePlan::Extend {
                    input: Box::new(RewritePlan::Join(vec![
                        RewritePlan::ReifiedTriple {
                            pattern: match left {
                                CoreQuery::Triple(pattern) => pattern,
                                _ => unreachable!(),
                            },
                            source: left_source.clone(),
                        },
                        RewritePlan::ReifiedTriple {
                            pattern: match right {
                                CoreQuery::Triple(pattern) => pattern,
                                _ => unreachable!(),
                            },
                            source: right_source.clone(),
                        },
                    ],),),
                    expression: PlanExpr::Product(vec![
                        PlanExpr::Var(left_source,),
                        PlanExpr::Var(right_source,),
                    ],),
                    variable: sum_input.clone(),
                },),
                items: vec![SelectItem::Expr {
                    expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(sum_input,),),),
                    alias: fragment.provenance.clone(),
                },],
                group_by: vec![],
            }
        );
    }

    #[test]
    fn treats_nested_and_as_variadic() {
        let query = CoreQuery::And(
            Box::new(CoreQuery::And(Box::new(triple("1")), Box::new(triple("2")))),
            Box::new(triple("3")),
        );

        let fragment = super::super::rewrite(&query);

        let RewritePlan::Select { input, .. } = fragment.plan else {
            panic!("optimized AND must have one outer SELECT");
        };

        let RewritePlan::Extend {
            input, expression, ..
        } = *input
        else {
            panic!("optimized AND must bind one provenance product");
        };

        let RewritePlan::Join(inputs) = *input else {
            panic!("optimized AND must join its patterns");
        };

        assert_eq!(inputs.len(), 3);

        let PlanExpr::Product(provenance_inputs) = expression else {
            panic!("optimized AND must multiply provenance inputs");
        };

        assert_eq!(provenance_inputs.len(), 3);
    }
}
