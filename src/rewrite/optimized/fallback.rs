use crate::model::CoreQuery;
use crate::plan::{PlanExpr, PlanVar, RewritePlan, SelectItem, VariableSubstitution};

use super::{aggregate, rewrite_with_context};
use crate::rewrite::context::RewriteContext;
use crate::rewrite::scope::{diff_substitution_variables, in_scope};
use crate::rewrite::variables::pattern_variables;

/// Applies one level of the base rewriting from Definition 4.2.
///
/// Recursive operands are always sent back through the optimized
/// dispatcher so that Definition 4.6 rules can still be applied
/// anywhere below the current query node.
pub(super) fn rewrite(
    query: &CoreQuery,
    provenance: &PlanVar,
    context: &mut RewriteContext,
) -> RewritePlan {
    match query {
        CoreQuery::Empty => RewritePlan::Extend {
            input: Box::new(RewritePlan::Unit),
            expression: PlanExpr::One,
            variable: provenance.clone(),
        },

        CoreQuery::Triple(pattern) => {
            let source = context.fresh_var();

            let reified = RewritePlan::ReifiedTriple {
                pattern: pattern.clone(),
                source: source.clone(),
            };

            aggregate::sum_by(reified, &pattern_variables(pattern), &source, provenance)
        }

        CoreQuery::And(left, right) => {
            let variables = in_scope(query);

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

            aggregate::sum_by(extended, &variables, &product, provenance)
        }

        CoreQuery::Union(left, right) => {
            let variables = in_scope(query);
            let sum_input = context.fresh_var();

            let left = rewrite_with_context(left, &sum_input, context);

            let right = rewrite_with_context(right, &sum_input, context);

            let union = RewritePlan::Union(vec![left, right]);

            aggregate::sum_by(union, &variables, &sum_input, provenance)
        }

        CoreQuery::Diff(left, right) => {
            let variables = in_scope(query);

            let substitutions = diff_substitution_variables(left, right)
                .into_iter()
                .map(|from| VariableSubstitution {
                    from,
                    to: context.fresh_var(),
                })
                .collect();

            let left_provenance = context.fresh_var();

            let left = rewrite_with_context(left, &left_provenance, context);

            let right_provenance = context.fresh_var();

            let right = rewrite_with_context(right, &right_provenance, context);

            let optional = RewritePlan::CompatibleOptional {
                left: Box::new(left),
                right: Box::new(right),
                substitutions,
            };

            let mut group_by: Vec<_> = variables.iter().cloned().map(PlanVar::User).collect();

            group_by.push(left_provenance.clone());

            let mut items: Vec<_> = variables
                .iter()
                .cloned()
                .map(PlanVar::User)
                .map(SelectItem::Var)
                .collect();

            items.push(SelectItem::Expr {
                expression: PlanExpr::Difference {
                    left: Box::new(PlanExpr::Var(left_provenance.clone())),
                    right: Box::new(PlanExpr::AggregateSum(Box::new(PlanExpr::Var(
                        right_provenance,
                    )))),
                },
                alias: provenance.clone(),
            });

            RewritePlan::Select {
                input: Box::new(optional),
                items,
                group_by,
            }
        }

        CoreQuery::Select { variables, input } => {
            let input_provenance = context.fresh_var();

            let input = rewrite_with_context(input, &input_provenance, context);

            aggregate::sum_by(input, variables, &input_provenance, provenance)
        }

        CoreQuery::Filter { input, condition } => {
            let input = rewrite_with_context(input, provenance, context);

            RewritePlan::Filter {
                input: Box::new(input),
                condition: *condition,
            }
        }

        CoreQuery::Bind {
            input,
            expression,
            variable,
        } => {
            let input = rewrite_with_context(input, provenance, context);

            RewritePlan::Bind {
                input: Box::new(input),
                expression: *expression,
                variable: PlanVar::User(variable.clone()),
            }
        }
    }
}
