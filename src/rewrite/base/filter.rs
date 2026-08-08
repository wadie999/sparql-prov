use crate::model::{CoreQuery, QueryExprId};
use crate::plan::{RewriteFragment, RewritePlan};

use super::rewrite_with_context;
use crate::rewrite::context::RewriteContext;

/// NPCS 2024, Definition 4.2, Rule 7.
///
/// Rewrites FILTER by recursively rewriting its input and preserving the
/// original filter expression unchanged.
pub(super) fn rewrite(
    input: &CoreQuery,
    condition: QueryExprId,
    context: &mut RewriteContext,
) -> RewriteFragment {
    let input = rewrite_with_context(input, context);

    RewriteFragment {
        plan: RewritePlan::Filter {
            input: Box::new(input.plan),
            condition,
        },
        provenance: input.provenance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{InternalVar, PlanExpr, PlanVar};

    #[test]
    fn matches_npcs_definition_4_2_rule_7() {
        let input = CoreQuery::Empty;
        let condition = QueryExprId::new(7);

        let mut context = RewriteContext::new();

        let fragment = rewrite(&input, condition, &mut context);

        let provenance = PlanVar::Internal(InternalVar::new(0));

        assert_eq!(fragment.provenance, provenance);

        assert_eq!(
            fragment.plan,
            RewritePlan::Filter {
                input: Box::new(RewritePlan::Extend {
                    input: Box::new(RewritePlan::Unit),
                    expression: PlanExpr::One,
                    variable: provenance,
                }),
                condition,
            }
        );
    }
}
