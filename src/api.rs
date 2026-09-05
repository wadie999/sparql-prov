//! High-level SPARQLProof rewriting API.
//!
//! This module composes parsing, NPCS rewriting, and SPARQL compilation into
//! the public library operation used by external applications.

use std::error::Error;
use std::fmt;

use oxrdf::Variable;

use crate::adapter::{SpargebraAdapterError, parse_sparql};
use crate::compiler::{CompileError, ReificationScheme, SparqlCompiler};
use crate::rewrite::{base, optimized};

/// NPCS rewriting algorithm used by the high-level API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteStrategy {
    /// NPCS 2024 base rewriting.
    Base,

    /// NPCS 2024 aggregation-reducing rewriting.
    Optimized,
}

/// Result of rewriting an input SPARQL query.
///
/// The concrete provenance variable is returned explicitly so callers never
/// need to depend on SPARQLProof's internal generated-variable naming policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewrittenQuery {
    sparql: String,
    provenance_variable: Variable,
    strategy: RewriteStrategy,
}

impl RewrittenQuery {
    /// Returns the generated provenance-aware SPARQL query.
    pub fn sparql(&self) -> &str {
        &self.sparql
    }

    /// Returns the result variable containing the final provenance value.
    pub fn provenance_variable(&self) -> &Variable {
        &self.provenance_variable
    }

    /// Returns the NPCS rewriting strategy used to produce this query.
    pub fn strategy(&self) -> RewriteStrategy {
        self.strategy
    }
}

/// Error produced by the complete SPARQL rewriting pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteError {
    /// Input SPARQL could not be adapted to the supported core query model.
    Adapter(SpargebraAdapterError),

    /// The rewritten plan could not be compiled to SPARQL.
    Compiler(CompileError),
}

impl fmt::Display for RewriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter(error) => {
                write!(formatter, "SPARQL adaptation failed: {error}")
            }

            Self::Compiler(error) => {
                write!(formatter, "SPARQL compilation failed: {error}")
            }
        }
    }
}

impl Error for RewriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Adapter(error) => Some(error),
            Self::Compiler(error) => Some(error),
        }
    }
}

impl From<SpargebraAdapterError> for RewriteError {
    fn from(error: SpargebraAdapterError) -> Self {
        Self::Adapter(error)
    }
}

impl From<CompileError> for RewriteError {
    fn from(error: CompileError) -> Self {
        Self::Compiler(error)
    }
}

/// Rewrites a SPARQL query using NPCS provenance computation.
///
/// This is the main high-level SPARQLProof entry point:
///
/// SPARQL text
/// → parser adapter
/// → core query model
/// → NPCS rewriting
/// → rewrite-plan compiler
/// → executable provenance-aware SPARQL.
///
/// The selected reification strategy remains supplied by the caller so the
/// rewriting pipeline stays independent of any particular RDF encoding or
/// endpoint.
pub fn rewrite_sparql<R>(
    query: &str,
    strategy: RewriteStrategy,
    reification: R,
) -> Result<RewrittenQuery, RewriteError>
where
    R: ReificationScheme,
{
    // Keep the active dataset outside the algebra rewrite. Nested SELECTs inherit
    // it, so provenance cannot accidentally use occurrences from another case.
    let mut input = spargebra::SparqlParser::new().parse_query(query)
        .map_err(|error| SpargebraAdapterError::Parse(error.to_string()))?;
    let dataset = match &mut input {
        spargebra::Query::Select { dataset, .. } => dataset.take(),
        _ => None,
    };
    let parsed = parse_sparql(&input.to_string())?;

    let fragment = match strategy {
        RewriteStrategy::Base => base::rewrite(parsed.query()),

        RewriteStrategy::Optimized => optimized::rewrite(parsed.query()),
    };

    let compiled = SparqlCompiler::new(reification).compile_fragment_with_expressions(
        &fragment,
        parsed.variables(),
        &parsed,
    )?;

    let sparql = if let Some(dataset) = dataset {
        let mut clauses = String::new();
        for graph in dataset.default {
            clauses.push_str(&format!("FROM {graph}\n"));
        }
        if let Some(named) = dataset.named {
            for graph in named {
                clauses.push_str(&format!("FROM NAMED {graph}\n"));
            }
        }
        format!("SELECT *\n{clauses}WHERE {{ {{ {} }} }}", compiled.sparql())
    } else {
        compiled.sparql().to_owned()
    };
    Ok(RewrittenQuery {
        sparql,
        provenance_variable: compiled.provenance_variable().clone(),
        strategy,
    })
}

#[cfg(test)]
mod tests {
    use oxrdf::NamedNode;

    use crate::compiler::RdfStarReification;

    use super::*;

    fn reification() -> RdfStarReification {
        RdfStarReification::new(
            NamedNode::new("http://example.com/wasDerivedFrom")
                .expect("valid provenance predicate"),
        )
    }

    #[test]
    fn rewrites_sparql_through_complete_public_pipeline() {
        let rewritten = rewrite_sparql(
            r#"
            SELECT ?person
            WHERE {
                ?person <http://example.com/likes> ?food .
            }
            "#,
            RewriteStrategy::Base,
            reification(),
        )
        .expect("supported query should rewrite");

        assert!(
            rewritten
                .sparql()
                .contains("<< ?person <http://example.com/likes> ?food >>")
        );

        assert!(
            rewritten
                .sparql()
                .contains("<http://example.com/wasDerivedFrom>")
        );

        assert!(
            rewritten
                .sparql()
                .contains(&rewritten.provenance_variable().to_string()),
            "returned provenance variable must occur in generated SPARQL"
        );

        assert_eq!(rewritten.strategy(), RewriteStrategy::Base);
    }

    #[test]
    fn exposes_optimized_rewriting_strategy() {
        let rewritten = rewrite_sparql(
            r#"
            SELECT ?person ?food ?drink
            WHERE {
                ?person <http://example.com/likes> ?food .
                ?person <http://example.com/drinks> ?drink .
            }
            "#,
            RewriteStrategy::Optimized,
            reification(),
        )
        .expect("optimized rewriting should succeed");

        assert_eq!(rewritten.strategy(), RewriteStrategy::Optimized);

        assert!(
            !rewritten.sparql().is_empty(),
            "optimized rewriting must produce executable SPARQL"
        );
    }

    #[test]
    fn restores_preserved_filter_expression_automatically() {
        let rewritten = rewrite_sparql(
            r#"
            SELECT ?person
            WHERE {
                ?person <http://example.com/age> ?age .
                FILTER(?age >= 18)
            }
            "#,
            RewriteStrategy::Optimized,
            reification(),
        )
        .expect("FILTER query should rewrite");

        assert!(
            rewritten.sparql().contains("FILTER"),
            "preserved FILTER must survive the complete pipeline"
        );

        assert!(
            rewritten.sparql().contains("?age"),
            "FILTER variable must remain available"
        );

        assert!(
            rewritten.sparql().contains(">="),
            "FILTER comparison must remain available"
        );
    }

    #[test]
    fn preserves_default_dataset_clauses() {
        let rewritten = rewrite_sparql(
            r#"
            SELECT ?person
            FROM <urn:case:source-a>
            FROM <urn:case:source-b>
            WHERE {
                ?person <http://example.com/likes> ?food .
            }
            "#,
            RewriteStrategy::Optimized,
            reification(),
        )
        .expect("dataset-scoped query should rewrite");

        assert!(rewritten.sparql().contains("FROM <urn:case:source-a>"));
        assert!(rewritten.sparql().contains("FROM <urn:case:source-b>"));
    }
}
