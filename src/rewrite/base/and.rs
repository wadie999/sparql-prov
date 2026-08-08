use crate::model::CoreQuery;
use crate::plan::{PlanExpr, PlanVar, RewritePlan};
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
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    let left_provenance = context.fresh_var();
    let left = rewrite_with_context(left, &left_provenance, context);

    let right_provenance = context.fresh_var();
    let right = rewrite_with_context(right, &right_provenance, context);

    let product = context.fresh_var();

    let joined = RewritePlan::Join(vec![left, right]);

    let extended = RewritePlan::Extend {
        input: Box::new(joined),
        expression: PlanExpr::Product(vec![
            PlanExpr::Var(left_provenance),
            PlanExpr::Var(right_provenance),
        ]),
        variable: product.clone(),
    };

    aggregate::sum_by(extended, variables, &product, provenance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{InternalVar, SelectItem};

    #[test]
    fn matches_npcs_definition_4_2_rule_3() {
        let left = CoreQuery::Empty;
        let right = CoreQuery::Empty;

        let mut context = RewriteContext::new();
        let provenance = context.fresh_var();

        let plan = rewrite(&left, &right, &[], &provenance, &mut context);

        let left_provenance = PlanVar::Internal(InternalVar::new(1));
        let right_provenance = PlanVar::Internal(InternalVar::new(2));
        let product = PlanVar::Internal(InternalVar::new(3));

        assert_eq!(
            plan,
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
                    expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(product)),),
                    alias: provenance,
                }],
                group_by: vec![],
            }
        );
    }
}
