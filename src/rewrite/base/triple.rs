use crate::model::TriplePattern;
use crate::plan::{PlanVar, RewritePlan};

use super::aggregate;
use crate::rewrite::context::RewriteContext;
use crate::rewrite::variables::pattern_variables;

/// NPCS 2024, Definition 4.2, Rule 2.
///
/// Rewrites a triple pattern by reifying its matched statement and
/// aggregating all provenance sources for each solution mapping.
pub(super) fn rewrite(
    pattern: &TriplePattern,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    let source = context.fresh_var();

    let reified = RewritePlan::ReifiedTriple {
        pattern: pattern.clone(),
        source: source.clone(),
    };

    aggregate::sum_by(reified, &pattern_variables(pattern), &source, provenance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern};
    use crate::plan::{InternalVar, PlanExpr, SelectItem};
    use oxrdf::Variable;

    #[test]
    fn matches_npcs_definition_4_2_rule_2() {
        let subject = Variable::new("s").expect("valid variable");
        let predicate = Variable::new("p").expect("valid variable");
        let object = Variable::new("o").expect("valid variable");

        let pattern = TriplePattern {
            subject: SubjectPattern::Variable(subject.clone()),
            predicate: PredicatePattern::Variable(predicate.clone()),
            object: ObjectPattern::Variable(object.clone()),
        };

        let mut context = RewriteContext::new();
        let provenance = context.fresh_var();

        let plan = rewrite(&pattern, &provenance, &mut context);

        let source = PlanVar::Internal(InternalVar::new(1));

        let subject = PlanVar::User(subject);
        let predicate = PlanVar::User(predicate);
        let object = PlanVar::User(object);

        assert_eq!(
            plan,
            RewritePlan::Select {
                input: Box::new(RewritePlan::ReifiedTriple {
                    pattern,
                    source: source.clone(),
                },),
                items: vec![
                    SelectItem::Var(subject.clone()),
                    SelectItem::Var(predicate.clone()),
                    SelectItem::Var(object.clone()),
                    SelectItem::Expr {
                        expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(source),),),
                        alias: provenance,
                    },
                ],
                group_by: vec![subject, predicate, object,],
            }
        );
    }
}
