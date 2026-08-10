//! Lowering of provenance rewrite plans to executable SPARQL.
//!
//! The compiler is intentionally separate from query rewriting:
//!
//! - rewriting determines what the transformed query must compute;
//! - compilation determines how that logical plan is represented in SPARQL.

mod error;
mod expression;
mod rdf_star;
mod reification;
mod syntax;
mod variables;

use oxrdf::Variable;

use crate::plan::{RewritePlan, SelectItem};

pub use error::CompileError;
pub use rdf_star::RdfStarReification;
pub use reification::ReificationScheme;

use variables::PlanVariableResolver;

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

    /// Compiles a rewrite plan to SPARQL.
    ///
    /// `reserved_variables` must contain the variables originating from the
    /// input query. They are used to guarantee that compiler-generated
    /// variables cannot collide with user variables.
    pub fn compile(
        &self,
        plan: &RewritePlan,
        reserved_variables: &[Variable],
    ) -> Result<String, CompileError> {
        let mut variables = PlanVariableResolver::new(reserved_variables);

        let pattern = self.compile_pattern(plan, &mut variables)?;

        Ok(format!("SELECT * WHERE {{ {pattern} }}"))
    }

    fn compile_pattern(
        &self,
        plan: &RewritePlan,
        variables: &mut PlanVariableResolver,
    ) -> Result<String, CompileError> {
        match plan {
            RewritePlan::Unit => Ok(String::new()),

            RewritePlan::ReifiedTriple { pattern, source } => {
                let source = variables.resolve(source);

                Ok(self.reification.reify(pattern, &source))
            }

            RewritePlan::Join(inputs) => self.compile_join(inputs, variables),

            RewritePlan::Union(inputs) => self.compile_union(inputs, variables),

            RewritePlan::Select {
                input,
                items,
                group_by,
            } => self.compile_select(input, items, group_by, variables),

            RewritePlan::Extend {
                input,
                expression,
                variable,
            } => {
                let input = self.compile_pattern(input, variables)?;

                let expression = expression::compile(expression, variables);

                let variable = variables.resolve(variable);

                let bind = format!("BIND({expression} AS {variable})");

                append_pattern(input, bind)
            }

            RewritePlan::CompatibleOptional { .. } => {
                Err(CompileError::UnsupportedPlan("CompatibleOptional"))
            }

            RewritePlan::Filter { .. } => Err(CompileError::UnsupportedPlan("Filter")),

            RewritePlan::Bind { .. } => Err(CompileError::UnsupportedPlan("Bind")),
        }
    }

    fn compile_join(
        &self,
        inputs: &[RewritePlan],
        variables: &mut PlanVariableResolver,
    ) -> Result<String, CompileError> {
        let mut compiled = Vec::with_capacity(inputs.len());

        for input in inputs {
            let pattern = self.compile_pattern(input, variables)?;

            if !pattern.is_empty() {
                compiled.push(pattern);
            }
        }

        Ok(compiled.join(" "))
    }

    fn compile_union(
        &self,
        inputs: &[RewritePlan],
        variables: &mut PlanVariableResolver,
    ) -> Result<String, CompileError> {
        if inputs.is_empty() {
            return Err(CompileError::InvalidPlan(
                "union requires at least one input",
            ));
        }

        let mut compiled = Vec::with_capacity(inputs.len());

        for input in inputs {
            let pattern = self.compile_pattern(input, variables)?;
            compiled.push(format!("{{ {pattern} }}"));
        }

        Ok(compiled.join(" UNION "))
    }

    fn compile_select(
        &self,
        input: &RewritePlan,
        items: &[SelectItem],
        group_by: &[crate::plan::PlanVar],
        variables: &mut PlanVariableResolver,
    ) -> Result<String, CompileError> {
        if items.is_empty() {
            return Err(CompileError::InvalidPlan(
                "select requires at least one projected item",
            ));
        }

        let input = self.compile_pattern(input, variables)?;

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
                let expression = expression::compile(plan_expression, variables);

                let alias = variables.resolve(alias);

                format!("({expression} AS {alias})")
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
    use oxrdf::{NamedNode, Variable};

    use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};
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

        let plan = RewritePlan::Join(vec![first, second]);

        let query = compiler().compile(&plan, &[]).expect("join should compile");

        assert_eq!(
            query,
            "SELECT * WHERE { \
BIND(1 AS ?__sparql_proof_0) \
BIND(1 AS ?__sparql_proof_1) }"
        );
    }

    #[test]
    fn empty_join_behaves_as_unit_pattern() {
        let plan = RewritePlan::Join(Vec::new());

        let query = compiler()
            .compile(&plan, &[])
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

        let plan = RewritePlan::Union(vec![first, second]);

        let query = compiler()
            .compile(&plan, &[])
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
        let plan = RewritePlan::Union(Vec::new());

        let error = compiler()
            .compile(&plan, &[])
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
    fn unsupported_plan_nodes_fail_explicitly() {
        let condition = crate::model::QueryExprId::new(0);

        let plan = RewritePlan::Filter {
            input: Box::new(RewritePlan::Unit),
            condition,
        };

        let error = compiler()
            .compile(&plan, &[])
            .expect_err("filter compilation is not implemented yet");

        assert_eq!(error, CompileError::UnsupportedPlan("Filter"));
    }
}
