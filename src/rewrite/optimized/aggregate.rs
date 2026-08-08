use crate::plan::{PlanExpr, PlanVar, RewritePlan, SelectItem};
use oxrdf::Variable;

pub(super) fn sum_by(
    input: RewritePlan,
    variables: &[Variable],
    source: &PlanVar,
    output: &PlanVar,
) -> RewritePlan {
    let group_by: Vec<_> = variables.iter().cloned().map(PlanVar::User).collect();

    let mut items: Vec<_> = group_by.iter().cloned().map(SelectItem::Var).collect();

    items.push(SelectItem::Expr {
        expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(source.clone()))),
        alias: output.clone(),
    });

    RewritePlan::Select {
        input: Box::new(input),
        items,
        group_by,
    }
}
