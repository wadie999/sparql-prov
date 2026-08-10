//! Preservation and rendering of Spargebra expression ASTs.
//!
//! FILTER and BIND expressions remain owned by the Spargebra adapter.
//! `CoreQuery` refers to them only through opaque `QueryExprId` values.

use oxrdf::Variable;
use spargebra::algebra::Expression;

use crate::compiler::ExpressionVariableSubstitution;
use crate::model::QueryExprId;

use super::spargebra::SpargebraAdapterError;

/// Expressions preserved while adapting one input SPARQL query.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct SpargebraExpressionStore {
    expressions: Vec<Expression>,
}

impl SpargebraExpressionStore {
    /// Stores an expression and returns its stable query-local identifier.
    pub(super) fn preserve(
        &mut self,
        expression: &Expression,
    ) -> Result<QueryExprId, SpargebraAdapterError> {
        validate_expression(expression)?;

        let index = u32::try_from(self.expressions.len())
            .map_err(|_| SpargebraAdapterError::ExpressionIdentifierSpaceExhausted)?;

        let identifier = QueryExprId::new(index);

        self.expressions.push(expression.clone());

        Ok(identifier)
    }

    /// Renders a preserved expression after applying concrete substitutions.
    pub(super) fn render(
        &self,
        identifier: QueryExprId,
        substitutions: &[ExpressionVariableSubstitution],
    ) -> Option<String> {
        let expression = self.expressions.get(identifier.index() as usize)?;

        Some(substitute_expression(expression, substitutions).to_string())
    }
}

/// Returns variables referenced by an expression.
///
/// The expression is validated at the same time so unsupported nested
/// constructs are rejected during adaptation rather than compilation.
pub(super) fn expression_variables(
    expression: &Expression,
) -> Result<Vec<Variable>, SpargebraAdapterError> {
    validate_expression(expression)?;

    let mut variables = Vec::new();

    collect_variables(expression, &mut variables);

    Ok(variables)
}

fn validate_expression(expression: &Expression) -> Result<(), SpargebraAdapterError> {
    match expression {
        Expression::NamedNode(_)
        | Expression::Literal(_)
        | Expression::Variable(_)
        | Expression::Bound(_) => Ok(()),

        Expression::Or(left, right)
        | Expression::And(left, right)
        | Expression::Equal(left, right)
        | Expression::SameTerm(left, right)
        | Expression::Greater(left, right)
        | Expression::GreaterOrEqual(left, right)
        | Expression::Less(left, right)
        | Expression::LessOrEqual(left, right)
        | Expression::Add(left, right)
        | Expression::Subtract(left, right)
        | Expression::Multiply(left, right)
        | Expression::Divide(left, right) => {
            validate_expression(left)?;
            validate_expression(right)
        }

        Expression::In(left, values) => {
            validate_expression(left)?;

            for value in values {
                validate_expression(value)?;
            }

            Ok(())
        }

        Expression::UnaryPlus(input) | Expression::UnaryMinus(input) | Expression::Not(input) => {
            validate_expression(input)
        }

        Expression::If(condition, then_branch, else_branch) => {
            validate_expression(condition)?;
            validate_expression(then_branch)?;
            validate_expression(else_branch)
        }

        Expression::Coalesce(inputs) | Expression::FunctionCall(_, inputs) => {
            for input in inputs {
                validate_expression(input)?;
            }

            Ok(())
        }

        Expression::Exists(_) => Err(SpargebraAdapterError::UnsupportedExpression("EXISTS")),
    }
}

fn collect_variables(expression: &Expression, variables: &mut Vec<Variable>) {
    match expression {
        Expression::Variable(variable) | Expression::Bound(variable) => {
            variables.push(variable.clone());
        }

        Expression::NamedNode(_) | Expression::Literal(_) => {}

        Expression::Or(left, right)
        | Expression::And(left, right)
        | Expression::Equal(left, right)
        | Expression::SameTerm(left, right)
        | Expression::Greater(left, right)
        | Expression::GreaterOrEqual(left, right)
        | Expression::Less(left, right)
        | Expression::LessOrEqual(left, right)
        | Expression::Add(left, right)
        | Expression::Subtract(left, right)
        | Expression::Multiply(left, right)
        | Expression::Divide(left, right) => {
            collect_variables(left, variables);
            collect_variables(right, variables);
        }

        Expression::In(left, values) => {
            collect_variables(left, variables);

            for value in values {
                collect_variables(value, variables);
            }
        }

        Expression::UnaryPlus(input) | Expression::UnaryMinus(input) | Expression::Not(input) => {
            collect_variables(input, variables);
        }

        Expression::If(condition, then_branch, else_branch) => {
            collect_variables(condition, variables);
            collect_variables(then_branch, variables);
            collect_variables(else_branch, variables);
        }

        Expression::Coalesce(inputs) | Expression::FunctionCall(_, inputs) => {
            for input in inputs {
                collect_variables(input, variables);
            }
        }

        Expression::Exists(_) => {
            unreachable!("EXISTS expressions are rejected during adaptation")
        }
    }
}

fn substitute_expression(
    expression: &Expression,
    substitutions: &[ExpressionVariableSubstitution],
) -> Expression {
    match expression {
        Expression::NamedNode(node) => Expression::NamedNode(node.clone()),

        Expression::Literal(literal) => Expression::Literal(literal.clone()),

        Expression::Variable(variable) => {
            Expression::Variable(substitute_variable(variable, substitutions))
        }

        Expression::Bound(variable) => {
            Expression::Bound(substitute_variable(variable, substitutions))
        }

        Expression::Or(left, right) => Expression::Or(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::And(left, right) => Expression::And(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::Equal(left, right) => Expression::Equal(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::SameTerm(left, right) => Expression::SameTerm(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::Greater(left, right) => Expression::Greater(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::GreaterOrEqual(left, right) => Expression::GreaterOrEqual(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::Less(left, right) => Expression::Less(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::LessOrEqual(left, right) => Expression::LessOrEqual(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::In(left, values) => Expression::In(
            Box::new(substitute_expression(left, substitutions)),
            values
                .iter()
                .map(|value| substitute_expression(value, substitutions))
                .collect(),
        ),

        Expression::Add(left, right) => Expression::Add(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::Subtract(left, right) => Expression::Subtract(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::Multiply(left, right) => Expression::Multiply(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::Divide(left, right) => Expression::Divide(
            Box::new(substitute_expression(left, substitutions)),
            Box::new(substitute_expression(right, substitutions)),
        ),

        Expression::UnaryPlus(input) => {
            Expression::UnaryPlus(Box::new(substitute_expression(input, substitutions)))
        }

        Expression::UnaryMinus(input) => {
            Expression::UnaryMinus(Box::new(substitute_expression(input, substitutions)))
        }

        Expression::Not(input) => {
            Expression::Not(Box::new(substitute_expression(input, substitutions)))
        }

        Expression::If(condition, then_branch, else_branch) => Expression::If(
            Box::new(substitute_expression(condition, substitutions)),
            Box::new(substitute_expression(then_branch, substitutions)),
            Box::new(substitute_expression(else_branch, substitutions)),
        ),

        Expression::Coalesce(inputs) => Expression::Coalesce(
            inputs
                .iter()
                .map(|input| substitute_expression(input, substitutions))
                .collect(),
        ),

        Expression::FunctionCall(function, inputs) => Expression::FunctionCall(
            function.clone(),
            inputs
                .iter()
                .map(|input| substitute_expression(input, substitutions))
                .collect(),
        ),

        Expression::Exists(_) => {
            unreachable!("EXISTS expressions are rejected during adaptation")
        }
    }
}

fn substitute_variable(
    variable: &Variable,
    substitutions: &[ExpressionVariableSubstitution],
) -> Variable {
    substitutions
        .iter()
        .rev()
        .find(|substitution| substitution.from == *variable)
        .map_or_else(|| variable.clone(), |substitution| substitution.to.clone())
}

#[cfg(test)]
mod tests {
    use oxrdf::Variable;
    use spargebra::Query;
    use spargebra::SparqlParser;
    use spargebra::algebra::GraphPattern;

    use super::*;

    fn filter_expression(query: &str) -> Expression {
        let parsed = SparqlParser::new()
            .parse_query(query)
            .expect("valid SPARQL query");

        let Query::Select { pattern, .. } = parsed else {
            panic!("expected SELECT");
        };

        fn find_filter(pattern: &GraphPattern) -> Option<&Expression> {
            match pattern {
                GraphPattern::Filter { expr, .. } => Some(expr),

                GraphPattern::Project { inner, .. } => find_filter(inner),

                _ => None,
            }
        }

        find_filter(&pattern)
            .expect("query should contain FILTER")
            .clone()
    }

    #[test]
    fn substitution_changes_variable_nodes_but_not_literal_text() {
        let expression = filter_expression(
            "SELECT ?y WHERE {
                ?s <http://example.com/p> ?y .
                FILTER(CONCAT(STR(?y), \"?y\") = ?y)
            }",
        );

        let mut store = SpargebraExpressionStore::default();

        let id = store
            .preserve(&expression)
            .expect("expression should be supported");

        let y = Variable::new("y").expect("valid variable");
        let renamed = Variable::new("renamed").expect("valid variable");

        let rendered = store
            .render(
                id,
                &[ExpressionVariableSubstitution {
                    from: y,
                    to: renamed,
                }],
            )
            .expect("expression should exist");

        assert!(rendered.contains("?renamed"));
        assert!(rendered.contains("\"?y\""));
        assert!(!rendered.contains("STR(?y)"));
    }

    #[test]
    fn substitution_renames_bound_variables_structurally() {
        let expression = filter_expression(
            "SELECT ?y WHERE {
                ?s <http://example.com/p> ?y .
                FILTER(BOUND(?y) && ?y != 0)
            }",
        );

        let mut store = SpargebraExpressionStore::default();

        let id = store
            .preserve(&expression)
            .expect("expression should be supported");

        let y = Variable::new("y").expect("valid variable");
        let replacement = Variable::new("replacement").expect("valid variable");

        let rendered = store
            .render(
                id,
                &[ExpressionVariableSubstitution {
                    from: y,
                    to: replacement,
                }],
            )
            .expect("expression should exist");

        assert!(rendered.contains("BOUND(?replacement)"));
    }
}
