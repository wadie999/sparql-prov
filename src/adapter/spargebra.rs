//! Spargebra adapter for the SPARQLProof core query model.
//!
//! Spargebra is used only to parse SPARQL and expose its algebra tree.
//! The tree is translated immediately into the parser-independent
//! `CoreQuery` representation used by provenance rewriting.

use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use oxrdf::Variable;
use spargebra::algebra::GraphPattern;
use spargebra::term::{NamedNodePattern, TermPattern, TriplePattern as SpargebraTriplePattern};
use spargebra::{Query, SparqlParser};

use crate::model::{CoreQuery, ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};

/// Result of adapting an input SPARQL query.
///
/// Besides the parser-independent query model, the adapter retains the user
/// variables appearing in the original query. The compiler uses this set to
/// avoid collisions with generated internal variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSparqlQuery {
    query: CoreQuery,
    variables: Vec<Variable>,
}

impl ParsedSparqlQuery {
    /// Returns the parser-independent query representation.
    pub fn query(&self) -> &CoreQuery {
        &self.query
    }

    /// Returns variables originating from the input SPARQL query.
    pub fn variables(&self) -> &[Variable] {
        &self.variables
    }

    /// Consumes the parsed query and returns its core representation.
    pub fn into_query(self) -> CoreQuery {
        self.query
    }
}

/// Error raised while parsing or adapting a SPARQL query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpargebraAdapterError {
    /// Spargebra rejected the input as invalid SPARQL.
    Parse(String),

    /// SPARQLProof currently accepts only SELECT queries.
    UnsupportedQueryForm(&'static str),

    /// The parsed algebra contains a graph-pattern operation outside the
    /// currently supported NPCS adapter fragment.
    UnsupportedGraphPattern(&'static str),
}

impl fmt::Display for SpargebraAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => {
                write!(formatter, "SPARQL parse error: {message}")
            }

            Self::UnsupportedQueryForm(kind) => {
                write!(formatter, "unsupported SPARQL query form: {kind}")
            }

            Self::UnsupportedGraphPattern(kind) => {
                write!(formatter, "unsupported SPARQL graph pattern: {kind}")
            }
        }
    }
}

impl Error for SpargebraAdapterError {}

/// Parses a SPARQL SELECT query into the SPARQLProof core model.
pub fn parse_sparql(input: &str) -> Result<ParsedSparqlQuery, SpargebraAdapterError> {
    let parsed = SparqlParser::new()
        .parse_query(input)
        .map_err(|error| SpargebraAdapterError::Parse(error.to_string()))?;

    let query = match parsed {
        Query::Select {
            dataset, pattern, ..
        } => {
            if dataset.is_some() {
                return Err(SpargebraAdapterError::UnsupportedGraphPattern(
                    "dataset specification",
                ));
            }

            adapt_graph_pattern(&pattern)?
        }

        Query::Construct { .. } => {
            return Err(SpargebraAdapterError::UnsupportedQueryForm("CONSTRUCT"));
        }

        Query::Describe { .. } => {
            return Err(SpargebraAdapterError::UnsupportedQueryForm("DESCRIBE"));
        }

        Query::Ask { .. } => {
            return Err(SpargebraAdapterError::UnsupportedQueryForm("ASK"));
        }
    };

    let variables = collect_query_variables(&query);

    Ok(ParsedSparqlQuery { query, variables })
}

fn adapt_graph_pattern(pattern: &GraphPattern) -> Result<CoreQuery, SpargebraAdapterError> {
    match pattern {
        GraphPattern::Bgp { patterns } => adapt_bgp(patterns),

        GraphPattern::Join { left, right } => Ok(CoreQuery::And(
            Box::new(adapt_graph_pattern(left)?),
            Box::new(adapt_graph_pattern(right)?),
        )),

        GraphPattern::Union { left, right } => Ok(CoreQuery::Union(
            Box::new(adapt_graph_pattern(left)?),
            Box::new(adapt_graph_pattern(right)?),
        )),

        GraphPattern::Project { inner, variables } => Ok(CoreQuery::Select {
            variables: variables.clone(),
            input: Box::new(adapt_graph_pattern(inner)?),
        }),

        GraphPattern::Path { .. } => Err(SpargebraAdapterError::UnsupportedGraphPattern(
            "property path",
        )),

        GraphPattern::LeftJoin { .. } => {
            Err(SpargebraAdapterError::UnsupportedGraphPattern("OPTIONAL"))
        }

        GraphPattern::Filter { .. } => {
            Err(SpargebraAdapterError::UnsupportedGraphPattern("FILTER"))
        }

        GraphPattern::Graph { .. } => Err(SpargebraAdapterError::UnsupportedGraphPattern("GRAPH")),

        GraphPattern::Extend { .. } => Err(SpargebraAdapterError::UnsupportedGraphPattern("BIND")),

        GraphPattern::Minus { .. } => Err(SpargebraAdapterError::UnsupportedGraphPattern("MINUS")),

        GraphPattern::Values { .. } => {
            Err(SpargebraAdapterError::UnsupportedGraphPattern("VALUES"))
        }

        GraphPattern::OrderBy { .. } => {
            Err(SpargebraAdapterError::UnsupportedGraphPattern("ORDER BY"))
        }

        GraphPattern::Distinct { .. } => {
            Err(SpargebraAdapterError::UnsupportedGraphPattern("DISTINCT"))
        }

        GraphPattern::Reduced { .. } => {
            Err(SpargebraAdapterError::UnsupportedGraphPattern("REDUCED"))
        }

        GraphPattern::Slice { .. } => Err(SpargebraAdapterError::UnsupportedGraphPattern(
            "LIMIT/OFFSET",
        )),

        GraphPattern::Group { .. } => {
            Err(SpargebraAdapterError::UnsupportedGraphPattern("GROUP BY"))
        }

        GraphPattern::Service { .. } => {
            Err(SpargebraAdapterError::UnsupportedGraphPattern("SERVICE"))
        }
    }
}

fn adapt_bgp(patterns: &[SpargebraTriplePattern]) -> Result<CoreQuery, SpargebraAdapterError> {
    let mut patterns = patterns.iter();

    let Some(first) = patterns.next() else {
        return Ok(CoreQuery::Empty);
    };

    let mut query = CoreQuery::Triple(adapt_triple_pattern(first));

    for pattern in patterns {
        query = CoreQuery::And(
            Box::new(query),
            Box::new(CoreQuery::Triple(adapt_triple_pattern(pattern))),
        );
    }

    Ok(query)
}

fn adapt_triple_pattern(pattern: &SpargebraTriplePattern) -> TriplePattern {
    TriplePattern {
        subject: adapt_subject(&pattern.subject),
        predicate: adapt_predicate(&pattern.predicate),
        object: adapt_object(&pattern.object),
    }
}

fn adapt_subject(pattern: &TermPattern) -> SubjectPattern {
    match pattern {
        TermPattern::NamedNode(node) => SubjectPattern::NamedNode(node.clone()),

        TermPattern::BlankNode(node) => SubjectPattern::BlankNode(node.clone()),

        TermPattern::Variable(variable) => SubjectPattern::Variable(variable.clone()),

        TermPattern::Literal(_) => {
            unreachable!("SPARQL parser cannot place a literal in subject position")
        }
    }
}

fn adapt_predicate(pattern: &NamedNodePattern) -> PredicatePattern {
    match pattern {
        NamedNodePattern::NamedNode(node) => PredicatePattern::NamedNode(node.clone()),

        NamedNodePattern::Variable(variable) => PredicatePattern::Variable(variable.clone()),
    }
}

fn adapt_object(pattern: &TermPattern) -> ObjectPattern {
    match pattern {
        TermPattern::NamedNode(node) => ObjectPattern::NamedNode(node.clone()),

        TermPattern::BlankNode(node) => ObjectPattern::BlankNode(node.clone()),

        TermPattern::Literal(literal) => ObjectPattern::Literal(literal.clone()),

        TermPattern::Variable(variable) => ObjectPattern::Variable(variable.clone()),
    }
}

fn collect_query_variables(query: &CoreQuery) -> Vec<Variable> {
    let mut variables = Vec::new();
    let mut seen = HashSet::new();

    collect_core_query_variables(query, &mut variables, &mut seen);

    variables
}

fn collect_core_query_variables(
    query: &CoreQuery,
    variables: &mut Vec<Variable>,
    seen: &mut HashSet<Variable>,
) {
    match query {
        CoreQuery::Empty => {}

        CoreQuery::Triple(pattern) => {
            collect_triple_variables(pattern, variables, seen);
        }

        CoreQuery::And(left, right)
        | CoreQuery::Union(left, right)
        | CoreQuery::Diff(left, right) => {
            collect_core_query_variables(left, variables, seen);

            collect_core_query_variables(right, variables, seen);
        }

        CoreQuery::Select {
            variables: projected,
            input,
        } => {
            collect_core_query_variables(input, variables, seen);

            for variable in projected {
                push_variable(variable, variables, seen);
            }
        }

        CoreQuery::Filter { input, .. } => {
            collect_core_query_variables(input, variables, seen);
        }

        CoreQuery::Bind {
            input, variable, ..
        } => {
            collect_core_query_variables(input, variables, seen);

            push_variable(variable, variables, seen);
        }
    }
}

fn collect_triple_variables(
    pattern: &TriplePattern,
    variables: &mut Vec<Variable>,
    seen: &mut HashSet<Variable>,
) {
    if let SubjectPattern::Variable(variable) = &pattern.subject {
        push_variable(variable, variables, seen);
    }

    if let PredicatePattern::Variable(variable) = &pattern.predicate {
        push_variable(variable, variables, seen);
    }

    if let ObjectPattern::Variable(variable) = &pattern.object {
        push_variable(variable, variables, seen);
    }
}

fn push_variable(variable: &Variable, variables: &mut Vec<Variable>, seen: &mut HashSet<Variable>) {
    if seen.insert(variable.clone()) {
        variables.push(variable.clone());
    }
}

#[cfg(test)]
mod tests {
    use oxrdf::{NamedNode, Variable};

    use super::*;

    #[test]
    fn parses_single_triple_select() {
        let parsed = parse_sparql(
            "SELECT ?s ?o WHERE {
                ?s <http://example.com/p> ?o .
            }",
        )
        .expect("query should parse");

        let s = Variable::new("s").expect("valid variable");
        let o = Variable::new("o").expect("valid variable");

        assert_eq!(
            parsed.query(),
            &CoreQuery::Select {
                variables: vec![s.clone(), o.clone()],
                input: Box::new(CoreQuery::Triple(TriplePattern {
                    subject: SubjectPattern::Variable(s.clone(),),
                    predicate: PredicatePattern::NamedNode(
                        NamedNode::new("http://example.com/p",).expect("valid IRI"),
                    ),
                    object: ObjectPattern::Variable(o.clone(),),
                },)),
            }
        );

        assert_eq!(parsed.variables(), &[s, o]);
    }

    #[test]
    fn parses_multi_triple_bgp_as_conjunction() {
        let parsed = parse_sparql(
            "SELECT ?x WHERE {
                ?x <http://example.com/p> ?y .
                ?y <http://example.com/q> ?z .
            }",
        )
        .expect("query should parse");

        assert!(matches!(
            parsed.query(),
            CoreQuery::Select {
                input,
                ..
            } if matches!(input.as_ref(), CoreQuery::And(_, _))
        ));
    }

    #[test]
    fn parses_union_structurally() {
        let parsed = parse_sparql(
            "SELECT ?x WHERE {
                { ?x <http://example.com/p> ?y }
                UNION
                { ?x <http://example.com/q> ?y }
            }",
        )
        .expect("query should parse");

        assert!(matches!(
            parsed.query(),
            CoreQuery::Select {
                input,
                ..
            } if matches!(input.as_ref(), CoreQuery::Union(_, _))
        ));
    }

    #[test]
    fn parses_empty_bgp_under_select_projection() {
        let parsed = parse_sparql("SELECT * WHERE {}").expect("query should parse");

        assert_eq!(
            parsed.query(),
            &CoreQuery::Select {
                variables: Vec::new(),
                input: Box::new(CoreQuery::Empty),
            }
        );

        assert!(parsed.variables().is_empty());
    }

    #[test]
    fn rejects_non_select_query_form() {
        let error = parse_sparql(
            "ASK WHERE {
                ?s <http://example.com/p> ?o
            }",
        )
        .expect_err("ASK should be rejected");

        assert_eq!(error, SpargebraAdapterError::UnsupportedQueryForm("ASK"));
    }

    #[test]
    fn rejects_property_paths_explicitly() {
        let error = parse_sparql(
            "SELECT ?x WHERE {
                ?x <http://example.com/p>+ ?y
            }",
        )
        .expect_err("property path should be rejected");

        assert_eq!(
            error,
            SpargebraAdapterError::UnsupportedGraphPattern("property path")
        );
    }
}
