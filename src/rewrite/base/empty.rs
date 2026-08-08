use crate::plan::{PlanExpr, PlanVar, RewritePlan};

/// NPCS 2024, Definition 4.2, Rule 1.
///
/// Rewrites the empty basic graph pattern by assigning provenance `1`.
pub(super) fn rewrite(provenance: &PlanVar) -> RewritePlan {
    RewritePlan::Extend {
        input: Box::new(RewritePlan::Unit),
        expression: PlanExpr::One,
        variable: provenance.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{InternalVar, PlanVar};
    use crate::rewrite::context::RewriteContext;

    #[test]
    fn matches_npcs_definition_4_2_rule_1() {
        let mut context = RewriteContext::new();
        let provenance = context.fresh_var();

        let plan = rewrite(&provenance);

        assert_eq!(provenance, PlanVar::Internal(InternalVar::new(0)));

        assert_eq!(
            plan,
            RewritePlan::Extend {
                input: Box::new(RewritePlan::Unit),
                expression: PlanExpr::One,
                variable: provenance,
            }
        );
    }
}
