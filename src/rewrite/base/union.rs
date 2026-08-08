use crate::model::CoreQuery;
use crate::plan::{PlanVar, RewritePlan};
use oxrdf::Variable;

use super::{aggregate, rewrite_with_context};
use crate::rewrite::context::RewriteContext;

/// NPCS 2024, Definition 4.2, Rule 4.
///
/// Rewrites both UNION branches into the same intermediate provenance
/// variable and sums their alternative derivations.
pub(super) fn rewrite(
    left: &CoreQuery,
    right: &CoreQuery,
    variables: &[Variable],
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    let sum_input = context.fresh_var();

    let left = rewrite_with_context(left, &sum_input, context);

    let right = rewrite_with_context(right, &sum_input, context);

    let union = RewritePlan::Union(vec![left, right]);

    aggregate::sum_by(union, variables, &sum_input, provenance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{InternalVar, PlanExpr, SelectItem};

    #[test]
    fn matches_npcs_definition_4_2_rule_4() {
        let left = CoreQuery::Empty;
        let right = CoreQuery::Empty;

        let mut context = RewriteContext::new();
        let provenance = context.fresh_var();

        let plan = rewrite(&left, &right, &[], &provenance, &mut context);

        let sum_input = PlanVar::Internal(InternalVar::new(1));

        assert_eq!(
            plan,
            RewritePlan::Select {
                input: Box::new(RewritePlan::Union(vec![
                    RewritePlan::Extend {
                        input: Box::new(RewritePlan::Unit,),
                        expression: PlanExpr::One,
                        variable: sum_input.clone(),
                    },
                    RewritePlan::Extend {
                        input: Box::new(RewritePlan::Unit,),
                        expression: PlanExpr::One,
                        variable: sum_input.clone(),
                    },
                ]),),
                items: vec![SelectItem::Expr {
                    expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(sum_input,)),),
                    alias: provenance,
                }],
                group_by: vec![],
            }
        );
    }
}
