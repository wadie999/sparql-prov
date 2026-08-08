use crate::model::CoreQuery;
use crate::plan::{PlanExpr, PlanVar, RewritePlan, SelectItem, VariableSubstitution};
use oxrdf::Variable;

use super::rewrite_with_context;
use crate::rewrite::context::RewriteContext;
use crate::rewrite::scope::diff_substitution_variables;

/// NPCS 2024, Definition 4.2, Rule 5.
///
/// Rewrites difference using the compatibility-preserving OPTIONAL
/// construction and provenance difference.
pub(super) fn rewrite(
    left: &CoreQuery,
    right: &CoreQuery,
    variables: &[Variable],
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    let substitutions = diff_substitution_variables(left, right)
        .into_iter()
        .map(|from| VariableSubstitution {
            from,
            to: context.fresh_var(),
        })
        .collect();

    let left_provenance = context.fresh_var();

    let left = rewrite_with_context(left, &left_provenance, context);

    let right_provenance = context.fresh_var();

    let right = rewrite_with_context(right, &right_provenance, context);

    let optional = RewritePlan::CompatibleOptional {
        left: Box::new(left),
        right: Box::new(right),
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
                right_provenance,
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
    use crate::rewrite::scope::in_scope;
    use oxrdf::{NamedNode, Variable};

    fn variable_pattern(subject: Variable, object: Variable) -> CoreQuery {
        CoreQuery::Triple(TriplePattern {
            subject: SubjectPattern::Variable(subject),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("https://example.com/p").expect("valid IRI"),
            ),
            object: ObjectPattern::Variable(object),
        })
    }

    #[test]
    fn matches_npcs_definition_4_2_rule_5() {
        let left = CoreQuery::Empty;
        let right = CoreQuery::Empty;

        let mut context = RewriteContext::new();
        let provenance = context.fresh_var();

        let plan = rewrite(&left, &right, &[], &provenance, &mut context);

        let left_provenance = PlanVar::Internal(InternalVar::new(1));

        let right_provenance = PlanVar::Internal(InternalVar::new(2));

        assert_eq!(
            plan,
            RewritePlan::Select {
                input: Box::new(RewritePlan::CompatibleOptional {
                    left: Box::new(RewritePlan::Extend {
                        input: Box::new(RewritePlan::Unit,),
                        expression: PlanExpr::One,
                        variable: left_provenance.clone(),
                    },),
                    right: Box::new(RewritePlan::Extend {
                        input: Box::new(RewritePlan::Unit,),
                        expression: PlanExpr::One,
                        variable: right_provenance.clone(),
                    },),
                    substitutions: vec![],
                },),
                items: vec![SelectItem::Expr {
                    expression: PlanExpr::Difference {
                        left: Box::new(PlanExpr::Var(left_provenance.clone(),),),
                        right: Box::new(PlanExpr::AggregateSum(Box::new(PlanExpr::Var(
                            right_provenance,
                        ),),),),
                    },
                    alias: provenance,
                }],
                group_by: vec![left_provenance],
            }
        );
    }

    #[test]
    fn renames_shared_variable_not_strongly_bound_on_left() {
        let x = Variable::new("x").expect("valid variable");

        let y = Variable::new("y").expect("valid variable");

        let left = CoreQuery::Union(
            Box::new(variable_pattern(x.clone(), y.clone())),
            Box::new(variable_pattern(x.clone(), x.clone())),
        );

        let right = variable_pattern(x, y.clone());

        let variables = in_scope(&left);

        let mut context = RewriteContext::new();
        let provenance = context.fresh_var();

        let plan = rewrite(&left, &right, &variables, &provenance, &mut context);

        let RewritePlan::Select { input, .. } = plan else {
            panic!("DIFF must produce an outer SELECT");
        };

        let RewritePlan::CompatibleOptional { substitutions, .. } = *input else {
            panic!("DIFF must use compatibility-preserving OPTIONAL");
        };

        assert_eq!(substitutions.len(), 1);
        assert_eq!(substitutions[0].from, y);

        assert!(matches!(substitutions[0].to, PlanVar::Internal(_)));
    }
}
