use crate::plan::{PlanExpr, RewriteFragment, RewritePlan};

use crate::rewrite::context::RewriteContext;

/// NPCS 2024, Definition 4.2, Rule 1.
///
/// Rewrites the empty basic graph pattern by assigning provenance `1`.
pub(super) fn rewrite(context: &mut RewriteContext) -> RewriteFragment {
    let provenance = context.fresh_var();

    RewriteFragment {
        plan: RewritePlan::Extend {
            input: Box::new(RewritePlan::Unit),
            expression: PlanExpr::One,
            variable: provenance.clone(),
        },
        provenance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{InternalVar, PlanVar};

    #[test]
    fn matches_npcs_definition_4_2_rule_1() {
        let mut context = RewriteContext::new();

        let fragment = rewrite(&mut context);

        let provenance = PlanVar::Internal(InternalVar::new(0));

        assert_eq!(fragment.provenance, provenance);

        assert_eq!(
            fragment.plan,
            RewritePlan::Extend {
                input: Box::new(RewritePlan::Unit),
                expression: PlanExpr::One,
                variable: provenance,
            }
        );
    }
}
