//! Lowering of provenance rewrite plans to executable SPARQL.
//!
//! The compiler is intentionally separate from query rewriting:
//!
//! - rewriting determines what the transformed query must compute;
//! - compilation determines how that logical plan is represented in SPARQL.

mod error;
mod expression;
mod input_expression;
mod rdf_star;
mod reification;
mod syntax;
mod variables;

use oxrdf::Variable;

use crate::plan::{PlanVar, RewritePlan, SelectItem};

pub use error::CompileError;
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
    ///
    /// Plans containing FILTER or input BIND nodes must instead use
    /// `compile_with_expressions`.
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

        let pattern = self.compile_pattern(plan, &mut variables, expressions)?;

        Ok(format!("SELECT * WHERE {{ {pattern} }}"))
    }

    fn compile_pattern<E>(
        &self,
        plan: &RewritePlan,
        variables: &mut PlanVariableResolver,
        expressions: &E,
    ) -> Result<String, CompileError>
    where
        E: InputExpressionRenderer,
    {
        match plan {
            RewritePlan::Unit => Ok(String::new()),

            RewritePlan::ReifiedTriple { pattern, source } => {
                let source = variables.resolve(source);

                Ok(self.reification.reify(pattern, &source))
            }

            RewritePlan::Join(inputs) => self.compile_join(inputs, variables, expressions),

            RewritePlan::Union(inputs) => self.compile_union(inputs, variables, expressions),

            RewritePlan::Select {
                input,
                items,
                group_by,
            } => self.compile_select(input, items, group_by, variables, expressions),

            RewritePlan::Filter { input, condition } => {
                let input = self.compile_pattern(input, variables, expressions)?;

                let condition = expressions
                    .render(*condition, &[])
                    .ok_or(CompileError::MissingInputExpression(*condition))?;

                append_pattern(input, format!("FILTER({condition})"))
            }

            RewritePlan::Bind {
                input,
                expression: input_expression,
                variable,
            } => {
                let input = self.compile_pattern(input, variables, expressions)?;

                let input_expression = expressions
                    .render(*input_expression, &[])
                    .ok_or(CompileError::MissingInputExpression(*input_expression))?;

                let variable = variables.resolve(variable);

                append_pattern(input, format!("BIND({input_expression} AS {variable})"))
            }

            RewritePlan::Extend {
                input,
                expression: plan_expression,
                variable,
            } => {
                let input = self.compile_pattern(input, variables, expressions)?;

                let plan_expression = expression::compile(plan_expression, variables);

                let variable = variables.resolve(variable);

                append_pattern(input, format!("BIND({plan_expression} AS {variable})"))
            }

            RewritePlan::CompatibleOptional { .. } => {
                Err(CompileError::UnsupportedPlan("CompatibleOptional"))
            }
        }
    }

    fn compile_join<E>(
        &self,
        inputs: &[RewritePlan],
        variables: &mut PlanVariableResolver,
        expressions: &E,
    ) -> Result<String, CompileError>
    where
        E: InputExpressionRenderer,
    {
        let mut compiled = Vec::with_capacity(inputs.len());

        for input in inputs {
            let pattern = self.compile_pattern(input, variables, expressions)?;

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
            let pattern = self.compile_pattern(input, variables, expressions)?;

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
    ) -> Result<String, CompileError>
    where
        E: InputExpressionRenderer,
    {
        if items.is_empty() {
            return Err(CompileError::InvalidPlan(
                "select requires at least one projected item",
            ));
        }

        let input = self.compile_pattern(input, variables, expressions)?;

        let items = items
            .iter()
            .map(|item| self.compile_select_item(item, variables))
            .collect::<Vec<_>>()
            .join(" ");

        let group_by = group_by
            .iter()
            .map(|variable| variables.resolve(variable).to_string())
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
    ) -> String {
        match item {
            SelectItem::Var(variable) => variables.resolve(variable).to_string(),

            SelectItem::Expr {
                expression: plan_expression,
                alias,
            } => {
                let plan_expression = expression::compile(plan_expression, variables);

                let alias = variables.resolve(alias);

                format!("({plan_expression} AS {alias})")
            }
        }
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
    use crate::plan::{InternalVar, PlanExpr, PlanVar, RewritePlan, SelectItem};

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
            assert!(
                substitutions.is_empty(),
                "ordinary FILTER/BIND compilation must not rename variables"
            );

            self.expressions.get(&expression).cloned()
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
    fn generated_source_variable_avoids_user_collision() {
        let reserved = Variable::new("__sparql_proof_0").expect("valid variable");

        let plan = RewritePlan::ReifiedTriple {
            pattern: triple_pattern(),
            source: PlanVar::Internal(InternalVar::new(0)),
        };

        let query = compiler()
            .compile(&plan, &[reserved])
            .expect("reified triple should compile");

        assert!(query.contains("?__sparql_proof_0_1"));

        assert!(!query.contains(
            "<http://example.com/wasDerivedFrom> \
?__sparql_proof_0 ."
        ));
    }

    #[test]
    fn user_plan_variable_is_preserved() {
        let provenance = Variable::new("prov").expect("valid variable");

        let plan = RewritePlan::ReifiedTriple {
            pattern: triple_pattern(),
            source: PlanVar::User(provenance.clone()),
        };

        let query = compiler()
            .compile(&plan, std::slice::from_ref(&provenance))
            .expect("reified triple should compile");

        assert!(query.contains("<http://example.com/wasDerivedFrom> ?prov ."));
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
    fn compiles_extend_after_existing_graph_pattern() {
        let source = PlanVar::Internal(InternalVar::new(0));

        let provenance = PlanVar::Internal(InternalVar::new(1));

        let plan = RewritePlan::Extend {
            input: Box::new(RewritePlan::ReifiedTriple {
                pattern: triple_pattern(),
                source: source.clone(),
            }),
            expression: PlanExpr::Var(source),
            variable: provenance,
        };

        let query = compiler()
            .compile(&plan, &[])
            .expect("extend should compile");

        assert_eq!(
            query,
            "SELECT * WHERE { \
<< ?person <http://example.com/likes> ?food >> \
<http://example.com/wasDerivedFrom> ?__sparql_proof_0 . \
BIND(?__sparql_proof_0 AS ?__sparql_proof_1) }"
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
    fn empty_join_behaves_as_unit_pattern() {
        let query = compiler()
            .compile(&RewritePlan::Join(Vec::new()), &[])
            .expect("empty join should compile");

        assert_eq!(query, "SELECT * WHERE {  }");
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
    fn rejects_empty_union() {
        let error = compiler()
            .compile(&RewritePlan::Union(Vec::new()), &[])
            .expect_err("empty union must be rejected");

        assert_eq!(
            error,
            CompileError::InvalidPlan("union requires at least one input")
        );
    }

    #[test]
    fn compiles_select_projection_and_grouping() {
        let person = Variable::new("person").expect("valid variable");

        let source = PlanVar::Internal(InternalVar::new(0));

        let provenance = PlanVar::Internal(InternalVar::new(1));

        let plan = RewritePlan::Select {
            input: Box::new(RewritePlan::ReifiedTriple {
                pattern: triple_pattern(),
                source: source.clone(),
            }),
            items: vec![
                SelectItem::Var(PlanVar::User(person.clone())),
                SelectItem::Expr {
                    expression: PlanExpr::AggregateSum(Box::new(PlanExpr::Var(source))),
                    alias: provenance,
                },
            ],
            group_by: vec![PlanVar::User(person.clone())],
        };

        let query = compiler()
            .compile(&plan, &[person])
            .expect("select should compile");

        assert_eq!(
            query,
            "SELECT * WHERE { \
{ SELECT ?person \
(CONCAT(\"(⊕\", \
GROUP_CONCAT(?__sparql_proof_0; SEPARATOR=\"\"), \")\") \
AS ?__sparql_proof_1) \
WHERE { \
<< ?person <http://example.com/likes> ?food >> \
<http://example.com/wasDerivedFrom> ?__sparql_proof_0 . \
} GROUP BY ?person } }"
        );
    }

    #[test]
    fn compiles_select_without_grouping() {
        let person = Variable::new("person").expect("valid variable");

        let plan = RewritePlan::Select {
            input: Box::new(RewritePlan::Unit),
            items: vec![SelectItem::Var(PlanVar::User(person.clone()))],
            group_by: Vec::new(),
        };

        let query = compiler()
            .compile(&plan, std::slice::from_ref(&person))
            .expect("select should compile");

        assert_eq!(
            query,
            "SELECT * WHERE { \
{ SELECT ?person WHERE {  } } }"
        );
    }

    #[test]
    fn rejects_select_without_projection_items() {
        let plan = RewritePlan::Select {
            input: Box::new(RewritePlan::Unit),
            items: Vec::new(),
            group_by: Vec::new(),
        };

        let error = compiler()
            .compile(&plan, &[])
            .expect_err("empty projection must be rejected");

        assert_eq!(
            error,
            CompileError::InvalidPlan("select requires at least one projected item")
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
    fn compiles_preserved_bind_expression() {
        let expression_id = QueryExprId::new(8);

        let expressions = TestExpressions::default().with(expression_id, "STR(?person)");

        let result = Variable::new("label").expect("valid variable");

        let plan = RewritePlan::Bind {
            input: Box::new(RewritePlan::Unit),
            expression: expression_id,
            variable: PlanVar::User(result.clone()),
        };

        let query = compiler()
            .compile_with_expressions(&plan, std::slice::from_ref(&result), &expressions)
            .expect("bind should compile");

        assert_eq!(
            query,
            "SELECT * WHERE { \
BIND(STR(?person) AS ?label) }"
        );
    }

    #[test]
    fn reports_missing_preserved_expression() {
        let condition = QueryExprId::new(9);

        let plan = RewritePlan::Filter {
            input: Box::new(RewritePlan::Unit),
            condition,
        };

        let error = compiler()
            .compile(&plan, &[])
            .expect_err("missing expression must fail");

        assert_eq!(error, CompileError::MissingInputExpression(condition));
    }

    #[test]
    fn compatible_optional_remains_explicitly_unsupported() {
        let plan = RewritePlan::CompatibleOptional {
            left: Box::new(RewritePlan::Unit),
            right: Box::new(RewritePlan::Unit),
            substitutions: Vec::new(),
        };

        let error = compiler()
            .compile(&plan, &[])
            .expect_err("compatible optional is not implemented yet");

        assert_eq!(error, CompileError::UnsupportedPlan("CompatibleOptional"));
    }
}
