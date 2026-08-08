use crate::model::CoreQuery;
use crate::plan::{PlanExpr, PlanVar, RewritePlan, SelectItem, VariableSubstitution};

use super::{rewrite_with_context, sum_query};
use crate::rewrite::context::RewriteContext;
use crate::rewrite::scope::{diff_substitution_variables, in_scope};

/// Returns whether Definition 4.6 Rule 3 applies.
///
/// The optimized DIFF rule applies when its right operand is a
/// Definition 4.5 sum-query.
pub(super) fn is_applicable(query: &CoreQuery) -> bool {
    match query {
        CoreQuery::Diff(_, right) => sum_query::is_sum_query(right),
        _ => false,
    }
}

/// Implements NPCS 2024, Definition 4.6, Rule 3.
pub(super) fn rewrite(
    query: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    let CoreQuery::Diff(left, right) = query else {
        unreachable!("optimized DIFF requires a DIFF query");
    };

    debug_assert!(sum_query::is_sum_query(right));

    let variables = in_scope(query);

    let substitutions = diff_substitution_variables(left, right)
        .into_iter()
        .map(|from| VariableSubstitution {
            from,
            to: context.fresh_var(),
        })
        .collect();

    let left_provenance = context.fresh_var();

    let left = rewrite_with_context(left, &left_provenance, context);

    let right_pattern_provenance = context.fresh_var();

    let right_pattern = sum_query::pattern(right, &right_pattern_provenance, context);

    let optional = RewritePlan::CompatibleOptional {
        left: Box::new(left),
        right: Box::new(right_pattern),
        substitutions,
    };

    let mut group_by: Vec<_> = variables.iter().cloned().map(PlanVar::User).collect();

    group_by.push(left_provenance.clone());

    let mut items: Vec<_> = variables
        .iter()
        .cloned()
        .map(PlanVar::User)
        .map(SelectItem::Var)
        .collect();

    items.push(SelectItem::Expr {
        expression: PlanExpr::Difference {
            left: Box::new(PlanExpr::Var(left_provenance.clone())),
            right: Box::new(PlanExpr::AggregateSum(Box::new(PlanExpr::Var(
                right_pattern_provenance,
            )))),
        },
        alias: provenance.clone(),
    });

    RewritePlan::Select {
        input: Box::new(optional),
        items,
        group_by,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};
    use crate::plan::InternalVar;
    use oxrdf::{NamedNode, Variable};

    fn constant_triple() -> CoreQuery {
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

    fn variable_triple(subject: Variable, object: Variable) -> CoreQuery {
        CoreQuery::Triple(TriplePattern {
            subject: SubjectPattern::Variable(subject),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("https://example.com/p").expect("valid IRI"),
            ),
            object: ObjectPattern::Variable(object),
        })
    }

    #[test]
    fn matches_npcs_definition_4_6_rule_3() {
        let query = CoreQuery::Diff(Box::new(CoreQuery::Empty), Box::new(constant_triple()));

        let fragment = super::super::rewrite(&query);

        let left_provenance = PlanVar::Internal(InternalVar::new(1));

        let right_pattern_provenance = PlanVar::Internal(InternalVar::new(2));

        assert_eq!(
            fragment.plan,
            RewritePlan::Select {
                input: Box::new(RewritePlan::CompatibleOptional {
                    left: Box::new(RewritePlan::Extend {
                        input: Box::new(RewritePlan::Unit,),
                        expression: PlanExpr::One,
                        variable: left_provenance.clone(),
                    },),
                    right: Box::new(RewritePlan::ReifiedTriple {
                        pattern: match constant_triple() {
                            CoreQuery::Triple(pattern) => pattern,
                            _ => unreachable!(),
                        },
                        source: right_pattern_provenance.clone(),
                    },),
                    substitutions: vec![],
                },),
                items: vec![SelectItem::Expr {
                    expression: PlanExpr::Difference {
                        left: Box::new(PlanExpr::Var(left_provenance.clone(),),),
                        right: Box::new(PlanExpr::AggregateSum(Box::new(PlanExpr::Var(
                            right_pattern_provenance,
                        ),),),),
                    },
                    alias: fragment.provenance.clone(),
                },],
                group_by: vec![left_provenance,],
            }
        );
    }

    #[test]
    fn preserves_diff_variable_substitution() {
        let x = Variable::new("x").expect("valid variable");

        let y = Variable::new("y").expect("valid variable");

        let left = CoreQuery::Union(
            Box::new(variable_triple(x.clone(), y.clone())),
            Box::new(variable_triple(x.clone(), x.clone())),
        );

        let right = variable_triple(x, y.clone());

        let query = CoreQuery::Diff(Box::new(left), Box::new(right));

        let fragment = super::super::rewrite(&query);

        let RewritePlan::Select { input, .. } = fragment.plan else {
            panic!("optimized DIFF must produce SELECT");
        };

        let RewritePlan::CompatibleOptional { substitutions, .. } = *input else {
            panic!("optimized DIFF must use CompatibleOptional");
        };

        assert_eq!(substitutions.len(), 1);

        assert_eq!(substitutions[0].from, y);

        assert!(matches!(substitutions[0].to, PlanVar::Internal(_)));
    }
}
