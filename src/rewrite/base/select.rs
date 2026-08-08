use crate::model::CoreQuery;
use crate::plan::{PlanVar, RewritePlan};
use oxrdf::Variable;

use super::{aggregate, rewrite_with_context};
use crate::rewrite::context::RewriteContext;

/// NPCS 2024, Definition 4.2, Rule 6.
///
/// Rewrites projection by recursively rewriting the input and summing
/// provenance for derivations that become identical after projection.
pub(super) fn rewrite(
    variables: &[Variable],
    input: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    let input_provenance = context.fresh_var();

    let input = rewrite_with_context(input, &input_provenance, context);

    aggregate::sum_by(input, variables, &input_provenance, provenance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};
    use crate::plan::{InternalVar, PlanExpr, SelectItem};
    use oxrdf::{NamedNode, Variable};

    #[test]
    fn matches_npcs_definition_4_2_rule_6() {
        let x = Variable::new("x").expect("valid variable");
        let y = Variable::new("y").expect("valid variable");

        let pattern = TriplePattern {
            subject: SubjectPattern::Variable(x.clone()),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("https://example.com/p").expect("valid IRI"),
            ),
            object: ObjectPattern::Variable(y.clone()),
        };

        let input = CoreQuery::Triple(pattern.clone());
        let variables = vec![x.clone()];

        let mut context = RewriteContext::new();
        let provenance = context.fresh_var();

        let plan = rewrite(&variables, &input, &provenance, &mut context);

        let input_provenance = PlanVar::Internal(InternalVar::new(1));
        let source = PlanVar::Internal(InternalVar::new(2));

        let x = PlanVar::User(x);
        let y = PlanVar::User(y);

        assert_eq!(
            plan,
            RewritePlan::Select {
                input: Box::new(RewritePlan::Select {
                    input: Box::new(RewritePlan::ReifiedTriple {
                        pattern,
                        source: source.clone(),
                    },),
                    items: vec![
                        SelectItem::Var(x.clone(),),
                        SelectItem::Var(y.clone(),),
                        SelectItem::Expr {
                            expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(source,),),),
                            alias: input_provenance.clone(),
                        },
                    ],
                    group_by: vec![x.clone(), y,],
                },),
                items: vec![
                    SelectItem::Var(x.clone()),
                    SelectItem::Expr {
                        expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(
                            input_provenance,
                        ),),),
                        alias: provenance,
                    },
                ],
                group_by: vec![x],
            }
        );
    }
}
