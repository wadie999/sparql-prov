use crate::model::{CoreQuery, QueryExprId};
use crate::plan::{PlanVar, RewriteFragment, RewritePlan};
use oxrdf::Variable;

use super::rewrite_with_context;
use crate::rewrite::context::RewriteContext;

/// NPCS 2024, Definition 4.2, Rule 8.
///
/// Rewrites BIND by recursively rewriting its input and preserving the
/// original expression and target variable unchanged.
pub(super) fn rewrite(
    input: &CoreQuery,
    expression: QueryExprId,
    variable: &Variable,
    context: &mut RewriteContext,
) -> RewriteFragment {
    let input = rewrite_with_context(input, context);

    RewriteFragment {
        plan: RewritePlan::Bind {
            input: Box::new(input.plan),
            expression,
            variable: PlanVar::User(variable.clone()),
        },
        provenance: input.provenance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{InternalVar, PlanExpr};

    #[test]
    fn matches_npcs_definition_4_2_rule_8() {
        let input = CoreQuery::Empty;
        let expression = QueryExprId::new(8);
        let variable = Variable::new("x").expect("valid variable");

        let mut context = RewriteContext::new();

        let fragment = rewrite(&input, expression, &variable, &mut context);

        let provenance = PlanVar::Internal(InternalVar::new(0));

        assert_eq!(fragment.provenance, provenance);

        assert_eq!(
            fragment.plan,
            RewritePlan::Bind {
                input: Box::new(RewritePlan::Extend {
                    input: Box::new(RewritePlan::Unit),
                    expression: PlanExpr::One,
                    variable: provenance,
                }),
                expression,
                variable: PlanVar::User(variable),
            }
        );
    }
}
