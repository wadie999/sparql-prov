//! Compilation of complete provenance rewrite fragments.
//!
//! A `RewriteFragment` contains both the rewritten query plan and the logical
//! variable that carries the final provenance value. Compiling the fragment
//! therefore preserves this correspondence and exposes the concrete SPARQL
//! variable to callers.

use oxrdf::Variable;

use crate::plan::RewriteFragment;

use super::variables::PlanVariableResolver;
use super::{CompileError, InputExpressionRenderer, ReificationScheme, SparqlCompiler};

/// Executable SPARQL produced from a complete provenance rewrite fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRewrite {
    sparql: String,
    provenance_variable: Variable,
}

impl CompiledRewrite {
    /// Returns the generated SPARQL query.
    pub fn sparql(&self) -> &str {
        &self.sparql
    }

    /// Returns the concrete SPARQL variable containing final provenance.
    pub fn provenance_variable(&self) -> &Variable {
        &self.provenance_variable
    }
}

impl<R> SparqlCompiler<R>
where
    R: ReificationScheme,
{
    /// Compiles a complete rewrite fragment while preserving the concrete
    /// identity of its final provenance variable.
    pub fn compile_fragment_with_expressions<E>(
        &self,
        fragment: &RewriteFragment,
        reserved_variables: &[Variable],
        expressions: &E,
    ) -> Result<CompiledRewrite, CompileError>
    where
        E: InputExpressionRenderer,
    {
        let mut variables = PlanVariableResolver::new(reserved_variables);

        let pattern = self.compile_pattern(&fragment.plan, &mut variables, expressions, &[])?;

        let provenance_variable = variables.resolve(&fragment.provenance);

        Ok(CompiledRewrite {
            sparql: format!("SELECT * WHERE {{ {pattern} }}"),
            provenance_variable,
        })
    }
}
