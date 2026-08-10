//! Lowering of provenance rewrite plans to executable SPARQL.
//!
//! The compiler is intentionally separate from query rewriting:
//!
//! - rewriting determines what the transformed query must compute;
//! - compilation determines how that logical plan is represented in SPARQL.

mod error;
mod expression;
mod fragment;
mod input_expression;
mod rdf_star;
mod reification;
mod substitution;
mod syntax;
mod variables;

use oxrdf::Variable;

use crate::plan::{PlanVar, RewritePlan, SelectItem, VariableSubstitution};

pub use error::CompileError;
pub use fragment::CompiledRewrite;
pub use input_expression::{ExpressionVariableSubstitution, InputExpressionRenderer};
pub use rdf_star::RdfStarReification;
pub use reification::ReificationScheme;

use variables::PlanVariableResolver;

struct NoInputExpressions;

impl InputExpressionRenderer for NoInputExpressions {
    fn render(
        &self,
        _expression: crate::model::QueryExprId,
        _substitutions: &[ExpressionVariableSubstitution],
    ) -> Option<String> {
        None
    }
}

/// Compiles provenance rewrite plans using a selected RDF reification
/// strategy.
#[derive(Debug, Clone)]
pub struct SparqlCompiler<R> {
    reification: R,
}

impl<R> SparqlCompiler<R>
where
    R: ReificationScheme,
{
    /// Creates a compiler using the given reification strategy.
    pub fn new(reification: R) -> Self {
        Self { reification }
    }

    /// Compiles a rewrite plan that does not require preserved input
    /// expressions.
    pub fn compile(
        &self,
        plan: &RewritePlan,
        reserved_variables: &[Variable],
    ) -> Result<String, CompileError> {
        self.compile_with_expressions(plan, reserved_variables, &NoInputExpressions)
    }

    /// Compiles a rewrite plan with access to expressions preserved from the
    /// input query.
    pub fn compile_with_expressions<E>(
        &self,
        plan: &RewritePlan,
        reserved_variables: &[Variable],
        expressions: &E,
    ) -> Result<String, CompileError>
    where
        E: InputExpressionRenderer,
    {
        let mut variables = PlanVariableResolver::new(reserved_variables);

        let pattern = self.compile_pattern(plan, &mut variables, expressions, &[])?;

        Ok(format!("SELECT * WHERE {{ {pattern} }}"))
    }

    fn compile_pattern<E>(
        &self,
        plan: &RewritePlan,
        variables: &mut PlanVariableResolver,
        expressions: &E,
        substitutions: &[VariableSubstitution],
    ) -> Result<String, CompileError>
    where
        E: InputExpressionRenderer,
    {
        match plan {
            RewritePlan::Unit => Ok(String::new()),

            RewritePlan::ReifiedTriple { pattern, source } => {
                let pattern = substitution::triple_pattern(pattern, substitutions, variables);

                let source = substitution::resolve_plan_variable(source, substitutions, variables);

                Ok(self.reification.reify(&pattern, &source))
            }

            RewritePlan::Join(inputs) => {
                self.compile_join(inputs, variables, expressions, substitutions)
            }

            RewritePlan::Union(inputs) => {
                self.compile_union(inputs, variables, expressions, substitutions)
            }

            RewritePlan::Select {
                input,
                items,
                group_by,
            } => self.compile_select(
                input,
                items,
                group_by,
                variables,
                expressions,
                substitutions,
            ),

            RewritePlan::Filter { input, condition } => {
                let input = self.compile_pattern(input, variables, expressions, substitutions)?;

                let expression_substitutions =
                    substitution::input_expression_substitutions(substitutions, variables);

                let condition = expressions
                    .render(*condition, &expression_substitutions)
                    .ok_or(CompileError::MissingInputExpression(*condition))?;

                append_pattern(input, format!("FILTER({condition})"))
            }

            RewritePlan::Bind {
                input,
                expression: input_expression,
                variable,
            } => {
                let input = self.compile_pattern(input, variables, expressions, substitutions)?;

                let expression_substitutions =
                    substitution::input_expression_substitutions(substitutions, variables);

                let input_expression = expressions
                    .render(*input_expression, &expression_substitutions)
                    .ok_or(CompileError::MissingInputExpression(*input_expression))?;

                let variable =
                    substitution::resolve_plan_variable(variable, substitutions, variables);

                append_pattern(input, format!("BIND({input_expression} AS {variable})"))
            }

            RewritePlan::Extend {
                input,
                expression: plan_expression,
                variable,
            } => {
                let input = self.compile_pattern(input, variables, expressions, substitutions)?;

                let plan_expression =
                    expression::compile(plan_expression, variables, substitutions);

                let variable =
                    substitution::resolve_plan_variable(variable, substitutions, variables);

                append_pattern(input, format!("BIND({plan_expression} AS {variable})"))
            }

            RewritePlan::CompatibleOptional {
                left,
                right,
                substitutions: local_substitutions,
            } => self.compile_compatible_optional(
                left,
                right,
                local_substitutions,
                variables,
                expressions,
                substitutions,
            ),
        }
    }

    fn compile_join<E>(
        &self,
        inputs: &[RewritePlan],
        variables: &mut PlanVariableResolver,
        expressions: &E,
        substitutions: &[VariableSubstitution],
    ) -> Result<String, CompileError>
    where
        E: InputExpressionRenderer,
    {
        let mut compiled = Vec::with_capacity(inputs.len());

        for input in inputs {
            let pattern = self.compile_pattern(input, variables, expressions, substitutions)?;

            if !pattern.is_empty() {
                compiled.push(pattern);
            }
        }

        Ok(compiled.join(" "))
    }

    fn compile_union<E>(
        &self,
        inputs: &[RewritePlan],
        variables: &mut PlanVariableResolver,
        expressions: &E,
        substitutions: &[VariableSubstitution],
    ) -> Result<String, CompileError>
    where
        E: InputExpressionRenderer,
    {
        if inputs.is_empty() {
            return Err(CompileError::InvalidPlan(
                "union requires at least one input",
            ));
        }

        let mut compiled = Vec::with_capacity(inputs.len());

        for input in inputs {
            let pattern = self.compile_pattern(input, variables, expressions, substitutions)?;

            compiled.push(format!("{{ {pattern} }}"));
        }

        Ok(compiled.join(" UNION "))
    }

    fn compile_select<E>(
        &self,
        input: &RewritePlan,
        items: &[SelectItem],
        group_by: &[PlanVar],
        variables: &mut PlanVariableResolver,
        expressions: &E,
        substitutions: &[VariableSubstitution],
    ) -> Result<String, CompileError>
    where
        E: InputExpressionRenderer,
    {
        if items.is_empty() {
            return Err(CompileError::InvalidPlan(
                "select requires at least one projected item",
            ));
        }

        let input = self.compile_pattern(input, variables, expressions, substitutions)?;

        let items = items
            .iter()
            .map(|item| self.compile_select_item(item, variables, substitutions))
            .collect::<Vec<_>>()
            .join(" ");

        let group_by = group_by
            .iter()
            .map(|variable| {
                substitution::resolve_plan_variable(variable, substitutions, variables).to_string()
            })
            .collect::<Vec<_>>();

        let grouping = if group_by.is_empty() {
            String::new()
        } else {
            format!(" GROUP BY {}", group_by.join(" "))
        };

        Ok(format!(
            "{{ SELECT {items} WHERE {{ {input} }}{grouping} }}"
        ))
    }

    fn compile_select_item(
        &self,
        item: &SelectItem,
        variables: &mut PlanVariableResolver,
        substitutions: &[VariableSubstitution],
    ) -> String {
        match item {
            SelectItem::Var(variable) => {
                substitution::resolve_plan_variable(variable, substitutions, variables).to_string()
            }

            SelectItem::Expr {
                expression: plan_expression,
                alias,
            } => {
                let plan_expression =
                    expression::compile(plan_expression, variables, substitutions);

                let alias = substitution::resolve_plan_variable(alias, substitutions, variables);

                format!("({plan_expression} AS {alias})")
            }
        }
    }

    fn compile_compatible_optional<E>(
        &self,
        left: &RewritePlan,
        right: &RewritePlan,
        local_substitutions: &[VariableSubstitution],
        variables: &mut PlanVariableResolver,
        expressions: &E,
        outer_substitutions: &[VariableSubstitution],
    ) -> Result<String, CompileError>
    where
        E: InputExpressionRenderer,
    {
        for substitution in local_substitutions {
            if !matches!(substitution.to, PlanVar::Internal(_)) {
                return Err(CompileError::InvalidPlan(
                    "compatible optional substitution target must be internal",
                ));
            }
        }

        let left = self.compile_pattern(left, variables, expressions, outer_substitutions)?;

        let right_substitutions = substitution::extend(outer_substitutions, local_substitutions);

        let right = self.compile_pattern(right, variables, expressions, &right_substitutions)?;

        let optional = format!("{left} OPTIONAL {{ {right} }}");

        if local_substitutions.is_empty() {
            return Ok(format!("{{ {optional} }}"));
        }

        let conditions = local_substitutions
            .iter()
            .map(|local| {
                let left_variable = substitution::resolve_user_variable(
                    &local.from,
                    outer_substitutions,
                    variables,
                );

                let right_variable = variables.resolve(&local.to);

                format!(
                    "(!BOUND({left_variable}) || \
!BOUND({right_variable}) || \
{left_variable} = {right_variable})"
                )
            })
            .collect::<Vec<_>>()
            .join(" && ");

        Ok(format!("{{ {optional} FILTER({conditions}) }}"))
    }
}

fn append_pattern(left: String, right: String) -> Result<String, CompileError> {
    if left.is_empty() {
        Ok(right)
    } else if right.is_empty() {
        Ok(left)
    } else {
        Ok(format!("{left} {right}"))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use oxrdf::{NamedNode, Variable};

    use crate::model::{
        ObjectPattern, PredicatePattern, QueryExprId, SubjectPattern, TriplePattern,
    };
    use crate::plan::{
        InternalVar, PlanExpr, PlanVar, RewritePlan, SelectItem, VariableSubstitution,
    };

    use super::*;

    fn compiler() -> SparqlCompiler<RdfStarReification> {
        SparqlCompiler::new(RdfStarReification::new(
            NamedNode::new("http://example.com/wasDerivedFrom").expect("valid IRI"),
        ))
    }

    fn triple_pattern() -> TriplePattern {
        TriplePattern {
            subject: SubjectPattern::Variable(Variable::new("person").expect("valid variable")),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("http://example.com/likes").expect("valid IRI"),
            ),
            object: ObjectPattern::Variable(Variable::new("food").expect("valid variable")),
        }
    }

    #[derive(Default)]
    struct TestExpressions {
        expressions: HashMap<QueryExprId, String>,
    }

    impl TestExpressions {
        fn with(mut self, id: QueryExprId, expression: &str) -> Self {
            self.expressions.insert(id, expression.to_owned());

            self
        }
    }

    impl InputExpressionRenderer for TestExpressions {
        fn render(
            &self,
            expression: QueryExprId,
            substitutions: &[ExpressionVariableSubstitution],
        ) -> Option<String> {
            let mut expression = self.expressions.get(&expression)?.clone();

            for substitution in substitutions {
                expression = expression
                    .replace(&substitution.from.to_string(), &substitution.to.to_string());
            }

            Some(expression)
        }
    }

    #[test]
    fn compiles_unit_as_empty_group_graph_pattern() {
        let query = compiler()
            .compile(&RewritePlan::Unit, &[])
            .expect("unit plan should compile");

        assert_eq!(query, "SELECT * WHERE {  }");
    }

    #[test]
    fn compiles_reified_triple_with_generated_source_variable() {
        let person = Variable::new("person").expect("valid variable");

        let food = Variable::new("food").expect("valid variable");

        let plan = RewritePlan::ReifiedTriple {
            pattern: triple_pattern(),
            source: PlanVar::Internal(InternalVar::new(0)),
        };

        let query = compiler()
            .compile(&plan, &[person, food])
            .expect("reified triple should compile");

        assert_eq!(
            query,
            "SELECT * WHERE { \
<< ?person <http://example.com/likes> ?food >> \
<http://example.com/wasDerivedFrom> ?__sparql_proof_0 . }"
        );
    }

    #[test]
    fn compiles_extend_over_unit_as_bind() {
        let plan = RewritePlan::Extend {
            input: Box::new(RewritePlan::Unit),
            expression: PlanExpr::One,
            variable: PlanVar::Internal(InternalVar::new(0)),
        };

        let query = compiler()
            .compile(&plan, &[])
            .expect("extend should compile");

        assert_eq!(
            query,
            "SELECT * WHERE { \
BIND(1 AS ?__sparql_proof_0) }"
        );
    }

    #[test]
    fn compiles_join_as_conjunction_of_patterns() {
        let first = RewritePlan::Extend {
            input: Box::new(RewritePlan::Unit),
            expression: PlanExpr::One,
            variable: PlanVar::Internal(InternalVar::new(0)),
        };

        let second = RewritePlan::Extend {
            input: Box::new(RewritePlan::Unit),
            expression: PlanExpr::One,
            variable: PlanVar::Internal(InternalVar::new(1)),
        };

        let query = compiler()
            .compile(&RewritePlan::Join(vec![first, second]), &[])
            .expect("join should compile");

        assert_eq!(
            query,
            "SELECT * WHERE { \
BIND(1 AS ?__sparql_proof_0) \
BIND(1 AS ?__sparql_proof_1) }"
        );
    }

    #[test]
    fn compiles_union_as_grouped_alternatives() {
        let first = RewritePlan::Extend {
            input: Box::new(RewritePlan::Unit),
            expression: PlanExpr::One,
            variable: PlanVar::Internal(InternalVar::new(0)),
        };

        let second = RewritePlan::Extend {
            input: Box::new(RewritePlan::Unit),
            expression: PlanExpr::One,
            variable: PlanVar::Internal(InternalVar::new(1)),
        };

        let query = compiler()
            .compile(&RewritePlan::Union(vec![first, second]), &[])
            .expect("union should compile");

        assert_eq!(
            query,
            "SELECT * WHERE { \
{ BIND(1 AS ?__sparql_proof_0) } UNION \
{ BIND(1 AS ?__sparql_proof_1) } }"
        );
    }

    #[test]
    fn compiles_preserved_filter_expression() {
        let condition = QueryExprId::new(7);

        let expressions =
            TestExpressions::default().with(condition, "?person != <http://example.com/Alice>");

        let plan = RewritePlan::Filter {
            input: Box::new(RewritePlan::ReifiedTriple {
                pattern: triple_pattern(),
                source: PlanVar::Internal(InternalVar::new(0)),
            }),
            condition,
        };

        let query = compiler()
            .compile_with_expressions(&plan, &[], &expressions)
            .expect("filter should compile");

        assert_eq!(
            query,
            "SELECT * WHERE { \
<< ?person <http://example.com/likes> ?food >> \
<http://example.com/wasDerivedFrom> ?__sparql_proof_0 . \
FILTER(?person != <http://example.com/Alice>) }"
        );
    }

    #[test]
    fn compatible_optional_applies_nu_to_right_pattern() {
        let x = Variable::new("x").expect("valid variable");
        let y = Variable::new("y").expect("valid variable");

        let left = RewritePlan::ReifiedTriple {
            pattern: TriplePattern {
                subject: SubjectPattern::Variable(x.clone()),
                predicate: PredicatePattern::NamedNode(
                    NamedNode::new("http://example.com/p").expect("valid IRI"),
                ),
                object: ObjectPattern::Variable(y.clone()),
            },
            source: PlanVar::Internal(InternalVar::new(0)),
        };

        let right = RewritePlan::ReifiedTriple {
            pattern: TriplePattern {
                subject: SubjectPattern::Variable(x.clone()),
                predicate: PredicatePattern::NamedNode(
                    NamedNode::new("http://example.com/q").expect("valid IRI"),
                ),
                object: ObjectPattern::Variable(y.clone()),
            },
            source: PlanVar::Internal(InternalVar::new(1)),
        };

        let plan = RewritePlan::CompatibleOptional {
            left: Box::new(left),
            right: Box::new(right),
            substitutions: vec![VariableSubstitution {
                from: y.clone(),
                to: PlanVar::Internal(InternalVar::new(7)),
            }],
        };

        let query = compiler()
            .compile(&plan, &[x, y])
            .expect("compatible optional should compile");

        assert!(query.contains("<< ?x <http://example.com/p> ?y >>"));

        assert!(query.contains("<< ?x <http://example.com/q> ?__sparql_proof_7 >>"));

        assert!(query.contains(
            "FILTER((!BOUND(?y) || \
!BOUND(?__sparql_proof_7) || \
?y = ?__sparql_proof_7))"
        ));
    }

    #[test]
    fn compatible_optional_renames_preserved_right_expression() {
        let y = Variable::new("y").expect("valid variable");

        let condition = QueryExprId::new(11);

        let expressions = TestExpressions::default().with(condition, "?y != 0");

        let right = RewritePlan::Filter {
            input: Box::new(RewritePlan::Unit),
            condition,
        };

        let plan = RewritePlan::CompatibleOptional {
            left: Box::new(RewritePlan::Unit),
            right: Box::new(right),
            substitutions: vec![VariableSubstitution {
                from: y.clone(),
                to: PlanVar::Internal(InternalVar::new(3)),
            }],
        };

        let query = compiler()
            .compile_with_expressions(&plan, std::slice::from_ref(&y), &expressions)
            .expect("compatible optional should compile");

        assert!(query.contains("FILTER(?__sparql_proof_3 != 0)"));
    }

    #[test]
    fn compatible_optional_renames_projection_and_grouping() {
        let y = Variable::new("y").expect("valid variable");

        let right = RewritePlan::Select {
            input: Box::new(RewritePlan::Unit),
            items: vec![SelectItem::Var(PlanVar::User(y.clone()))],
            group_by: vec![PlanVar::User(y.clone())],
        };

        let plan = RewritePlan::CompatibleOptional {
            left: Box::new(RewritePlan::Unit),
            right: Box::new(right),
            substitutions: vec![VariableSubstitution {
                from: y.clone(),
                to: PlanVar::Internal(InternalVar::new(5)),
            }],
        };

        let query = compiler()
            .compile(&plan, std::slice::from_ref(&y))
            .expect("compatible optional should compile");

        assert!(query.contains("SELECT ?__sparql_proof_5"));

        assert!(query.contains("GROUP BY ?__sparql_proof_5"));
    }

    #[test]
    fn compatible_optional_without_nu_omits_compatibility_filter() {
        let plan = RewritePlan::CompatibleOptional {
            left: Box::new(RewritePlan::Unit),
            right: Box::new(RewritePlan::Unit),
            substitutions: Vec::new(),
        };

        let query = compiler()
            .compile(&plan, &[])
            .expect("compatible optional should compile");

        assert_eq!(query, "SELECT * WHERE { {  OPTIONAL {  } } }");

        assert!(!query.contains("FILTER"));
    }

    #[test]
    fn rejects_non_internal_nu_target() {
        let y = Variable::new("y").expect("valid variable");
        let replacement = Variable::new("replacement").expect("valid variable");

        let plan = RewritePlan::CompatibleOptional {
            left: Box::new(RewritePlan::Unit),
            right: Box::new(RewritePlan::Unit),
            substitutions: vec![VariableSubstitution {
                from: y,
                to: PlanVar::User(replacement),
            }],
        };

        let error = compiler()
            .compile(&plan, &[])
            .expect_err("invalid nu target must be rejected");

        assert_eq!(
            error,
            CompileError::InvalidPlan("compatible optional substitution target must be internal")
        );
    }
}
