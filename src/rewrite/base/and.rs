use crate::model::CoreQuery;
use crate::plan::{PlanExpr, RewriteFragment, RewritePlan};
use oxrdf::Variable;

use super::{aggregate, rewrite_with_context};
use crate::rewrite::context::RewriteContext;

/// NPCS 2024, Definition 4.2, Rule 3.
///
/// Rewrites conjunction by recursively rewriting both operands,
/// multiplying their provenance, and summing alternative derivations.
pub(super) fn rewrite(
    left: &CoreQuery,
    right: &CoreQuery,
    variables: &[Variable],
    context: &mut RewriteContext,
) -> RewriteFragment {
    let left = rewrite_with_context(left, context);
    let right = rewrite_with_context(right, context);

    let product = context.fresh_var();
    let provenance = context.fresh_var();

    let joined = RewritePlan::Join(vec![left.plan, right.plan]);

    let extended = RewritePlan::Extend {
        input: Box::new(joined),
        expression: PlanExpr::Product(vec![
            PlanExpr::Var(left.provenance),
            PlanExpr::Var(right.provenance),
        ]),
        variable: product.clone(),
    };

    RewriteFragment {
        plan: aggregate::sum_by(extended, variables, &product, &provenance),
        provenance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{InternalVar, PlanVar, SelectItem};

    #[test]
    fn matches_npcs_definition_4_2_rule_3() {
        let query = CoreQuery::And(Box::new(CoreQuery::Empty), Box::new(CoreQuery::Empty));

        let mut context = RewriteContext::new();

        let fragment = match &query {
            CoreQuery::And(left, right) => rewrite(left, right, &[], &mut context),
            _ => unreachable!(),
        };

        let left_provenance = PlanVar::Internal(InternalVar::new(0));
        let right_provenance = PlanVar::Internal(InternalVar::new(1));
        let product = PlanVar::Internal(InternalVar::new(2));
        let provenance = PlanVar::Internal(InternalVar::new(3));

        assert_eq!(fragment.provenance, provenance);

        assert_eq!(
            fragment.plan,
            RewritePlan::Select {
                input: Box::new(RewritePlan::Extend {
                    input: Box::new(RewritePlan::Join(vec![
                        RewritePlan::Extend {
                            input: Box::new(RewritePlan::Unit),
                            expression: PlanExpr::One,
                            variable: left_provenance.clone(),
                        },
                        RewritePlan::Extend {
                            input: Box::new(RewritePlan::Unit),
                            expression: PlanExpr::One,
                            variable: right_provenance.clone(),
                        },
                    ])),
                    expression: PlanExpr::Product(vec![
                        PlanExpr::Var(left_provenance),
                        PlanExpr::Var(right_provenance),
                    ]),
                    variable: product.clone(),
                }),
                items: vec![SelectItem::Expr {
                    expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(product),)),
                    alias: provenance,
                }],
                group_by: vec![],
            }
        );
    }
}
