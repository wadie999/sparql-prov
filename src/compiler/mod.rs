//! Lowering of provenance rewrite plans to executable SPARQL.
//!
//! The compiler is intentionally separate from query rewriting:
//!
//! - rewriting determines what the transformed query must compute;
//! - compilation determines how that logical plan is represented in SPARQL.

mod error;
mod rdf_star;
mod reification;
mod syntax;
mod variables;

use oxrdf::Variable;

use crate::plan::RewritePlan;

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

    /// Compiles the currently supported rewrite-plan fragment to SPARQL.
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

            RewritePlan::Join(_) => Err(CompileError::UnsupportedPlan("Join")),

            RewritePlan::Union(_) => Err(CompileError::UnsupportedPlan("Union")),

            RewritePlan::CompatibleOptional { .. } => {
                Err(CompileError::UnsupportedPlan("CompatibleOptional"))
            }

            RewritePlan::Select { .. } => Err(CompileError::UnsupportedPlan("Select")),

            RewritePlan::Filter { .. } => Err(CompileError::UnsupportedPlan("Filter")),

            RewritePlan::Bind { .. } => Err(CompileError::UnsupportedPlan("Bind")),

            RewritePlan::Extend { .. } => Err(CompileError::UnsupportedPlan("Extend")),
        }
    }
}

#[cfg(test)]
mod tests {
    use oxrdf::{NamedNode, Variable};

    use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};
    use crate::plan::{InternalVar, PlanVar};

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
    fn unsupported_plan_nodes_fail_explicitly() {
        let plan = RewritePlan::Join(vec![RewritePlan::Unit, RewritePlan::Unit]);

        let error = compiler()
            .compile(&plan, &[])
            .expect_err("join compilation is not implemented yet");

        assert_eq!(error, CompileError::UnsupportedPlan("Join"));
    }
}
