//! Compilation of rewrite-time provenance expressions to SPARQL.
//!
//! NPCS defines provenance operations such as `ProvAggSum`, `ProvProd`,
//! and `ProvDiff` in terms of SPARQL string and aggregate expressions.
//!
//! This module lowers `PlanExpr`, which represents computations to be
//! performed by the rewritten query. It does not construct runtime
//! `ProvenanceExpr` values.

use crate::plan::{PlanExpr, PlanVar};

use super::variables::PlanVariableResolver;

/// Compiles a rewrite-time provenance expression to SPARQL.
///
/// Variable identities are resolved through the same resolver used by the
/// surrounding query compilation so that references remain consistent
/// throughout the generated query.
pub(super) fn compile(expression: &PlanExpr, variables: &mut PlanVariableResolver) -> String {
    match expression {
        PlanExpr::One => "1".to_owned(),

        PlanExpr::Var(variable) => compile_variable(variable, variables),

        PlanExpr::AggregateSum(input) => {
            let input = compile(input, variables);

            format!("CONCAT(\"(⊕\", GROUP_CONCAT({input}; SEPARATOR=\"\"), \")\")")
        }

        PlanExpr::Product(factors) => compile_product(factors, variables),

        PlanExpr::Difference { left, right } => {
            let left = compile(left, variables);
            let right = compile(right, variables);

            format!("CONCAT(\"(⊖\", {left}, {right}, \")\")")
        }
    }
}

fn compile_variable(variable: &PlanVar, variables: &mut PlanVariableResolver) -> String {
    variables.resolve(variable).to_string()
}

fn compile_product(factors: &[PlanExpr], variables: &mut PlanVariableResolver) -> String {
    if factors.is_empty() {
        return "1".to_owned();
    }

    let factors = factors
        .iter()
        .map(|factor| compile(factor, variables))
        .collect::<Vec<_>>()
        .join(", ");

    format!("CONCAT(\"(⊗\", {factors}, \")\")")
}

#[cfg(test)]
mod tests {
    use oxrdf::Variable;

    use crate::plan::{InternalVar, PlanExpr, PlanVar};

    use super::*;

    #[test]
    fn compiles_multiplicative_identity() {
        let mut variables = PlanVariableResolver::new(&[]);

        assert_eq!(compile(&PlanExpr::One, &mut variables), "1");
    }

    #[test]
    fn compiles_user_variable_without_renaming() {
        let variable = Variable::new("prov").expect("valid variable");

        let expression = PlanExpr::Var(PlanVar::User(variable.clone()));

        let mut variables = PlanVariableResolver::new(std::slice::from_ref(&variable));

        assert_eq!(compile(&expression, &mut variables), "?prov");
    }

    #[test]
    fn compiles_internal_variable_using_compiler_resolver() {
        let expression = PlanExpr::Var(PlanVar::Internal(InternalVar::new(4)));

        let mut variables = PlanVariableResolver::new(&[]);

        assert_eq!(compile(&expression, &mut variables), "?__sparql_proof_4");
    }

    #[test]
    fn internal_expression_variable_avoids_user_collision() {
        let reserved = Variable::new("__sparql_proof_4").expect("valid variable");

        let expression = PlanExpr::Var(PlanVar::Internal(InternalVar::new(4)));

        let mut variables = PlanVariableResolver::new(&[reserved]);

        assert_eq!(compile(&expression, &mut variables), "?__sparql_proof_4_1");
    }

    #[test]
    fn compiles_npcs_provenance_aggregate_sum() {
        let expression = PlanExpr::AggregateSum(Box::new(PlanExpr::Var(PlanVar::Internal(
            InternalVar::new(1),
        ))));

        let mut variables = PlanVariableResolver::new(&[]);

        assert_eq!(
            compile(&expression, &mut variables),
            "CONCAT(\"(⊕\", \
GROUP_CONCAT(?__sparql_proof_1; SEPARATOR=\"\"), \")\")"
        );
    }

    #[test]
    fn compiles_npcs_provenance_product_in_factor_order() {
        let expression = PlanExpr::Product(vec![
            PlanExpr::Var(PlanVar::Internal(InternalVar::new(1))),
            PlanExpr::Var(PlanVar::Internal(InternalVar::new(2))),
            PlanExpr::Var(PlanVar::Internal(InternalVar::new(3))),
        ]);

        let mut variables = PlanVariableResolver::new(&[]);

        assert_eq!(
            compile(&expression, &mut variables),
            "CONCAT(\"(⊗\", \
?__sparql_proof_1, \
?__sparql_proof_2, \
?__sparql_proof_3, \")\")"
        );
    }

    #[test]
    fn empty_product_compiles_to_multiplicative_identity() {
        let expression = PlanExpr::Product(Vec::new());

        let mut variables = PlanVariableResolver::new(&[]);

        assert_eq!(compile(&expression, &mut variables), "1");
    }

    #[test]
    fn compiles_npcs_provenance_difference() {
        let expression = PlanExpr::Difference {
            left: Box::new(PlanExpr::Var(PlanVar::Internal(InternalVar::new(1)))),
            right: Box::new(PlanExpr::Var(PlanVar::Internal(InternalVar::new(2)))),
        };

        let mut variables = PlanVariableResolver::new(&[]);

        assert_eq!(
            compile(&expression, &mut variables),
            "CONCAT(\"(⊖\", \
?__sparql_proof_1, \
?__sparql_proof_2, \")\")"
        );
    }

    #[test]
    fn compiles_diff_with_nested_aggregate_sum() {
        let expression = PlanExpr::Difference {
            left: Box::new(PlanExpr::Var(PlanVar::Internal(InternalVar::new(1)))),
            right: Box::new(PlanExpr::AggregateSum(Box::new(PlanExpr::Var(
                PlanVar::Internal(InternalVar::new(2)),
            )))),
        };

        let mut variables = PlanVariableResolver::new(&[]);

        assert_eq!(
            compile(&expression, &mut variables),
            "CONCAT(\"(⊖\", \
?__sparql_proof_1, \
CONCAT(\"(⊕\", \
GROUP_CONCAT(?__sparql_proof_2; SEPARATOR=\"\"), \")\"), \
\")\")"
        );
    }

    #[test]
    fn preserves_nested_provenance_computation_structure() {
        let expression = PlanExpr::AggregateSum(Box::new(PlanExpr::Product(vec![
            PlanExpr::Var(PlanVar::Internal(InternalVar::new(1))),
            PlanExpr::Var(PlanVar::Internal(InternalVar::new(2))),
        ])));

        let mut variables = PlanVariableResolver::new(&[]);

        assert_eq!(
            compile(&expression, &mut variables),
            "CONCAT(\"(⊕\", \
GROUP_CONCAT(CONCAT(\"(⊗\", \
?__sparql_proof_1, \
?__sparql_proof_2, \")\"); SEPARATOR=\"\"), \")\")"
        );
    }
}
