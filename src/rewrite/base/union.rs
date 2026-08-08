use crate::model::CoreQuery;
use crate::plan::{PlanExpr, RewriteFragment, RewritePlan};
use oxrdf::Variable;

use super::{aggregate, rewrite_with_context};
use crate::rewrite::context::RewriteContext;

/// NPCS 2024, Definition 4.2, Rule 4.
///
/// Rewrites union by recursively rewriting both branches into a common
/// provenance slot and summing provenance for each solution mapping.
pub(super) fn rewrite(
    left: &CoreQuery,
    right: &CoreQuery,
    variables: &[Variable],
    context: &mut RewriteContext,
) -> RewriteFragment {
    let left = rewrite_with_context(left, context);
    let right = rewrite_with_context(right, context);

    let sum_input = context.fresh_var();
    let provenance = context.fresh_var();

    let left = RewritePlan::Extend {
        input: Box::new(left.plan),
        expression: PlanExpr::Var(left.provenance),
        variable: sum_input.clone(),
    };

    let right = RewritePlan::Extend {
        input: Box::new(right.plan),
        expression: PlanExpr::Var(right.provenance),
        variable: sum_input.clone(),
    };

    let union = RewritePlan::Union(vec![left, right]);

    RewriteFragment {
        plan: aggregate::sum_by(union, variables, &sum_input, &provenance),
        provenance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{InternalVar, PlanVar, SelectItem};

    #[test]
    fn matches_npcs_definition_4_2_rule_4() {
        let left = CoreQuery::Empty;
        let right = CoreQuery::Empty;

        let mut context = RewriteContext::new();

        let fragment = rewrite(&left, &right, &[], &mut context);

        let left_provenance = PlanVar::Internal(InternalVar::new(0));
        let right_provenance = PlanVar::Internal(InternalVar::new(1));
        let sum_input = PlanVar::Internal(InternalVar::new(2));
        let provenance = PlanVar::Internal(InternalVar::new(3));

        assert_eq!(fragment.provenance, provenance);

        assert_eq!(
            fragment.plan,
            RewritePlan::Select {
                input: Box::new(RewritePlan::Union(vec![
                    RewritePlan::Extend {
                        input: Box::new(RewritePlan::Extend {
                            input: Box::new(RewritePlan::Unit),
                            expression: PlanExpr::One,
                            variable: left_provenance.clone(),
                        }),
                        expression: PlanExpr::Var(left_provenance),
                        variable: sum_input.clone(),
                    },
                    RewritePlan::Extend {
                        input: Box::new(RewritePlan::Extend {
                            input: Box::new(RewritePlan::Unit),
                            expression: PlanExpr::One,
                            variable: right_provenance.clone(),
                        }),
                        expression: PlanExpr::Var(right_provenance),
                        variable: sum_input.clone(),
                    },
                ])),
                items: vec![SelectItem::Expr {
                    expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(sum_input),)),
                    alias: provenance,
                }],
                group_by: vec![],
            }
        );
    }
}
