use crate::model::CoreQuery;
use crate::plan::{PlanVar, RewritePlan};

use super::{aggregate, sum_query};
use crate::rewrite::context::RewriteContext;

/// Returns whether Definition 4.6 Rule 4 applies.
///
/// The optimized SELECT rule applies when its input is a
/// Definition 4.5 sum-query.
pub(super) fn is_applicable(query: &CoreQuery) -> bool {
    match query {
        CoreQuery::Select { input, .. } => sum_query::is_sum_query(input),

        _ => false,
    }
}

/// Implements NPCS 2024, Definition 4.6, Rule 4.
pub(super) fn rewrite(
    query: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    let CoreQuery::Select { variables, input } = query else {
        unreachable!("optimized SELECT requires a SELECT query");
    };

    debug_assert!(sum_query::is_sum_query(input));

    let sum_input = context.fresh_var();

    let pattern = sum_query::pattern(input, &sum_input, context);

    aggregate::sum_by(pattern, variables, &sum_input, provenance)
}

/// Builds the pattern of an optimized SELECT.
///
/// Under Definition 4.6 Rule 4, the pattern of the SELECT rewriting is
/// exactly the pattern of its sum-query input.
pub(super) fn pattern(
    query: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    let CoreQuery::Select { input, .. } = query else {
        unreachable!("optimized SELECT pattern requires a SELECT query");
    };

    debug_assert!(sum_query::is_sum_query(input));

    sum_query::pattern(input, provenance, context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};
    use crate::plan::{InternalVar, PlanExpr, SelectItem};
    use oxrdf::{NamedNode, Variable};

    fn triple(variable: Variable) -> CoreQuery {
        CoreQuery::Triple(TriplePattern {
            subject: SubjectPattern::Variable(variable),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("https://example.com/p").expect("valid IRI"),
            ),
            object: ObjectPattern::NamedNode(
                NamedNode::new("https://example.com/o").expect("valid IRI"),
            ),
        })
    }

    #[test]
    fn matches_npcs_definition_4_6_rule_4() {
        let x = Variable::new("x").expect("valid variable");

        let input = triple(x.clone());

        let query = CoreQuery::Select {
            variables: vec![x.clone()],
            input: Box::new(input.clone()),
        };

        let fragment = super::super::rewrite(&query);

        let sum_input = PlanVar::Internal(InternalVar::new(1));

        assert_eq!(
            fragment.plan,
            RewritePlan::Select {
                input: Box::new(RewritePlan::ReifiedTriple {
                    pattern: match input {
                        CoreQuery::Triple(pattern) => pattern,
                        _ => unreachable!(),
                    },
                    source: sum_input.clone(),
                },),
                items: vec![
                    SelectItem::Var(PlanVar::User(x.clone(),),),
                    SelectItem::Expr {
                        expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(sum_input,),),),
                        alias: fragment.provenance.clone(),
                    },
                ],
                group_by: vec![PlanVar::User(x),],
            }
        );
    }

    #[test]
    fn exposes_select_pattern_without_intermediate_aggregation() {
        let x = Variable::new("x").expect("valid variable");

        let selected = CoreQuery::Select {
            variables: vec![x.clone()],
            input: Box::new(triple(x.clone())),
        };

        let query = CoreQuery::And(Box::new(selected), Box::new(triple(x)));

        let fragment = super::super::rewrite(&query);

        let RewritePlan::Select { input, .. } = fragment.plan else {
            panic!("optimized AND must produce an outer SELECT");
        };

        let RewritePlan::Extend { input, .. } = *input else {
            panic!("optimized AND must bind a provenance product");
        };

        let RewritePlan::Join(inputs) = *input else {
            panic!("optimized AND must join operand patterns");
        };

        assert_eq!(inputs.len(), 2);

        assert!(
            inputs
                .iter()
                .all(|input| { matches!(input, RewritePlan::ReifiedTriple { .. }) })
        );
    }
}
