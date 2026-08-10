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

use crate::compiler::{ExpressionVariableSubstitution, InputExpressionRenderer};
use crate::model::{
    CoreQuery, ObjectPattern, PredicatePattern, QueryExprId, SubjectPattern, TriplePattern,
};

use super::spargebra_expression::{SpargebraExpressionStore, expression_variables};

/// Result of adapting an input SPARQL query.
///
/// Besides the parser-independent query model, the adapter retains the user
/// variables and preserved input expressions required by later compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSparqlQuery {
    query: CoreQuery,
    variables: Vec<Variable>,
    expressions: SpargebraExpressionStore,
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

impl InputExpressionRenderer for ParsedSparqlQuery {
    fn render(
        &self,
        expression: QueryExprId,
        substitutions: &[ExpressionVariableSubstitution],
    ) -> Option<String> {
        self.expressions.render(expression, substitutions)
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
    /// currently supported adapter fragment.
    UnsupportedGraphPattern(&'static str),

    /// A preserved SPARQL expression contains a construct that cannot yet be
    /// safely retained and structurally substituted.
    UnsupportedExpression(&'static str),

    /// More preserved expressions were encountered than `QueryExprId` can
    /// represent.
    ExpressionIdentifierSpaceExhausted,
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

            Self::UnsupportedExpression(kind) => {
                write!(formatter, "unsupported SPARQL expression: {kind}")
            }

            Self::ExpressionIdentifierSpaceExhausted => {
                formatter.write_str("input expression identifier space exhausted")
            }
        }
    }
}

impl Error for SpargebraAdapterError {}

#[derive(Debug, Default)]
struct AdapterContext {
    expressions: SpargebraExpressionStore,
    variables: Vec<Variable>,
    seen_variables: HashSet<Variable>,
}

impl AdapterContext {
    fn push_variable(&mut self, variable: &Variable) {
        if self.seen_variables.insert(variable.clone()) {
            self.variables.push(variable.clone());
        }
    }

    fn preserve_expression(
        &mut self,
        expression: &spargebra::algebra::Expression,
    ) -> Result<QueryExprId, SpargebraAdapterError> {
        for variable in expression_variables(expression)? {
            self.push_variable(&variable);
        }

        self.expressions.preserve(expression)
    }
}

/// Parses a SPARQL SELECT query into the SPARQLProof core model.
pub fn parse_sparql(input: &str) -> Result<ParsedSparqlQuery, SpargebraAdapterError> {
    let parsed = SparqlParser::new()
        .parse_query(input)
        .map_err(|error| SpargebraAdapterError::Parse(error.to_string()))?;

    let mut context = AdapterContext::default();

    let query = match parsed {
        Query::Select {
            dataset, pattern, ..
        } => {
            if dataset.is_some() {
                return Err(SpargebraAdapterError::UnsupportedGraphPattern(
                    "dataset specification",
                ));
            }

            adapt_graph_pattern(&pattern, &mut context)?
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

    Ok(ParsedSparqlQuery {
        query,
        variables: context.variables,
        expressions: context.expressions,
    })
}

fn adapt_graph_pattern(
    pattern: &GraphPattern,
    context: &mut AdapterContext,
) -> Result<CoreQuery, SpargebraAdapterError> {
    match pattern {
        GraphPattern::Bgp { patterns } => adapt_bgp(patterns, context),

        GraphPattern::Join { left, right } => Ok(CoreQuery::And(
            Box::new(adapt_graph_pattern(left, context)?),
            Box::new(adapt_graph_pattern(right, context)?),
        )),

        GraphPattern::Union { left, right } => Ok(CoreQuery::Union(
            Box::new(adapt_graph_pattern(left, context)?),
            Box::new(adapt_graph_pattern(right, context)?),
        )),

        GraphPattern::Project { inner, variables } => {
            let input = adapt_graph_pattern(inner, context)?;

            for variable in variables {
                context.push_variable(variable);
            }

            Ok(CoreQuery::Select {
                variables: variables.clone(),
                input: Box::new(input),
            })
        }

        GraphPattern::Filter { expr, inner } => {
            let input = adapt_graph_pattern(inner, context)?;

            let condition = context.preserve_expression(expr)?;

            Ok(CoreQuery::Filter {
                input: Box::new(input),
                condition,
            })
        }

        GraphPattern::Extend {
            inner,
            variable,
            expression,
        } => {
            let input = adapt_graph_pattern(inner, context)?;

            let expression = context.preserve_expression(expression)?;

            context.push_variable(variable);

            Ok(CoreQuery::Bind {
                input: Box::new(input),
                expression,
                variable: variable.clone(),
            })
        }

        GraphPattern::Path { .. } => Err(SpargebraAdapterError::UnsupportedGraphPattern(
            "property path",
        )),

        GraphPattern::LeftJoin { .. } => {
            Err(SpargebraAdapterError::UnsupportedGraphPattern("OPTIONAL"))
        }

        GraphPattern::Graph { .. } => Err(SpargebraAdapterError::UnsupportedGraphPattern("GRAPH")),

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

fn adapt_bgp(
    patterns: &[SpargebraTriplePattern],
    context: &mut AdapterContext,
) -> Result<CoreQuery, SpargebraAdapterError> {
    let mut patterns = patterns.iter();

    let Some(first) = patterns.next() else {
        return Ok(CoreQuery::Empty);
    };

    let mut query = CoreQuery::Triple(adapt_triple_pattern(first, context));

    for pattern in patterns {
        query = CoreQuery::And(
            Box::new(query),
            Box::new(CoreQuery::Triple(adapt_triple_pattern(pattern, context))),
        );
    }

    Ok(query)
}

fn adapt_triple_pattern(
    pattern: &SpargebraTriplePattern,
    context: &mut AdapterContext,
) -> TriplePattern {
    TriplePattern {
        subject: adapt_subject(&pattern.subject, context),
        predicate: adapt_predicate(&pattern.predicate, context),
        object: adapt_object(&pattern.object, context),
    }
}

fn adapt_subject(pattern: &TermPattern, context: &mut AdapterContext) -> SubjectPattern {
    match pattern {
        TermPattern::NamedNode(node) => SubjectPattern::NamedNode(node.clone()),

        TermPattern::BlankNode(node) => SubjectPattern::BlankNode(node.clone()),

        TermPattern::Variable(variable) => {
            context.push_variable(variable);

            SubjectPattern::Variable(variable.clone())
        }

        TermPattern::Literal(_) => {
            unreachable!("SPARQL parser cannot place a literal in subject position")
        }
    }
}

fn adapt_predicate(pattern: &NamedNodePattern, context: &mut AdapterContext) -> PredicatePattern {
    match pattern {
        NamedNodePattern::NamedNode(node) => PredicatePattern::NamedNode(node.clone()),

        NamedNodePattern::Variable(variable) => {
            context.push_variable(variable);

            PredicatePattern::Variable(variable.clone())
        }
    }
}

fn adapt_object(pattern: &TermPattern, context: &mut AdapterContext) -> ObjectPattern {
    match pattern {
        TermPattern::NamedNode(node) => ObjectPattern::NamedNode(node.clone()),

        TermPattern::BlankNode(node) => ObjectPattern::BlankNode(node.clone()),

        TermPattern::Literal(literal) => ObjectPattern::Literal(literal.clone()),

        TermPattern::Variable(variable) => {
            context.push_variable(variable);

            ObjectPattern::Variable(variable.clone())
        }
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
    fn parses_filter_and_preserves_expression() {
        let parsed = parse_sparql(
            "SELECT ?person WHERE {
                ?person <http://example.com/age> ?age .
                FILTER(?age >= 18)
            }",
        )
        .expect("FILTER query should parse");

        let CoreQuery::Select { input, .. } = parsed.query() else {
            panic!("expected SELECT");
        };

        let CoreQuery::Filter { condition, .. } = input.as_ref() else {
            panic!("expected FILTER");
        };

        let rendered = parsed
            .render(*condition, &[])
            .expect("preserved expression should render");

        assert!(rendered.contains("?age"));
        assert!(rendered.contains(">="));
    }

    #[test]
    fn parses_bind_and_preserves_expression() {
        let parsed = parse_sparql(
            "SELECT ?label WHERE {
                ?person <http://example.com/name> ?name .
                BIND(STR(?name) AS ?label)
            }",
        )
        .expect("BIND query should parse");

        let CoreQuery::Select { input, .. } = parsed.query() else {
            panic!("expected SELECT");
        };

        let CoreQuery::Bind {
            expression,
            variable,
            ..
        } = input.as_ref()
        else {
            panic!("expected BIND");
        };

        assert_eq!(variable, &Variable::new("label").expect("valid variable"));

        let rendered = parsed
            .render(*expression, &[])
            .expect("preserved expression should render");

        assert!(rendered.contains("STR(?name)"));
    }

    #[test]
    fn preserved_expression_can_be_structurally_renamed() {
        let parsed = parse_sparql(
            "SELECT ?value WHERE {
                ?s <http://example.com/p> ?value .
                FILTER(BOUND(?value) && ?value != 0)
            }",
        )
        .expect("FILTER query should parse");

        let CoreQuery::Select { input, .. } = parsed.query() else {
            panic!("expected SELECT");
        };

        let CoreQuery::Filter { condition, .. } = input.as_ref() else {
            panic!("expected FILTER");
        };

        let value = Variable::new("value").expect("valid variable");

        let renamed = Variable::new("renamed").expect("valid variable");

        let rendered = parsed
            .render(
                *condition,
                &[ExpressionVariableSubstitution {
                    from: value,
                    to: renamed,
                }],
            )
            .expect("preserved expression should render");

        assert!(rendered.contains("BOUND(?renamed)"));
        assert!(rendered.contains("?renamed"));
    }

    #[test]
    fn rejects_exists_expression_explicitly() {
        let error = parse_sparql(
            "SELECT ?s WHERE {
                ?s <http://example.com/p> ?o .
                FILTER EXISTS {
                    ?s <http://example.com/q> ?x
                }
            }",
        )
        .expect_err("EXISTS should be rejected");

        assert_eq!(
            error,
            SpargebraAdapterError::UnsupportedExpression("EXISTS")
        );
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
