//! Core query model used by provenance rewriting.
//!
//! The model captures the SPARQL constructs required by the rewriting
//! engine without depending on a particular SPARQL parser.

use oxrdf::{BlankNode, Literal, NamedNode, Variable};

/// Opaque reference to an expression originating from the input query.
///
/// NPCS preserves FILTER and BIND expressions without interpreting their
/// internal SPARQL expression structure. Adapters are responsible for
/// associating these identifiers with the original expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueryExprId(u32);

impl QueryExprId {
    /// Creates an expression identifier.
    pub fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the numeric identity of the referenced expression.
    pub fn index(self) -> u32 {
        self.0
    }
}

/// Subject position of a triple pattern.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubjectPattern {
    NamedNode(NamedNode),
    BlankNode(BlankNode),
    Variable(Variable),
    Triple(Box<TriplePattern>),
}

/// Predicate position of a triple pattern.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PredicatePattern {
    NamedNode(NamedNode),
    Variable(Variable),
}

/// Object position of a triple pattern.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ObjectPattern {
    NamedNode(NamedNode),
    BlankNode(BlankNode),
    Literal(Literal),
    Variable(Variable),
    Triple(Box<TriplePattern>),
}

/// A SPARQL triple pattern.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TriplePattern {
    pub subject: SubjectPattern,
    pub predicate: PredicatePattern,
    pub object: ObjectPattern,
}

/// A query expression admitted by the provenance rewriting engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreQuery {
    /// The empty basic graph pattern.
    Empty,

    /// A single triple pattern.
    Triple(TriplePattern),

    /// Conjunction of two query expressions.
    And(Box<CoreQuery>, Box<CoreQuery>),

    /// Union of two query expressions.
    Union(Box<CoreQuery>, Box<CoreQuery>),

    /// Algebraic difference between two query expressions.
    Diff(Box<CoreQuery>, Box<CoreQuery>),

    /// Projection over a set of variables.
    Select {
        variables: Vec<Variable>,
        input: Box<CoreQuery>,
    },

    /// Filter a query using an expression from the input query.
    Filter {
        input: Box<CoreQuery>,
        condition: QueryExprId,
    },

    /// Extend a query with a value computed by an input-query expression.
    Bind {
        input: Box<CoreQuery>,
        expression: QueryExprId,
        variable: Variable,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_expression_identifier_preserves_identity() {
        let expression = QueryExprId::new(7);

        assert_eq!(expression.index(), 7);
    }

    #[test]
    fn represents_filter_without_interpreting_expression() {
        let query = CoreQuery::Filter {
            input: Box::new(CoreQuery::Empty),
            condition: QueryExprId::new(3),
        };

        assert!(matches!(
            query,
            CoreQuery::Filter {
                condition,
                ..
            } if condition == QueryExprId::new(3)
        ));
    }

    #[test]
    fn represents_bind_without_interpreting_expression() {
        let variable = Variable::new("result").expect("valid SPARQL variable");

        let query = CoreQuery::Bind {
            input: Box::new(CoreQuery::Empty),
            expression: QueryExprId::new(5),
            variable: variable.clone(),
        };

        assert!(matches!(
            query,
            CoreQuery::Bind {
                expression,
                variable: bound_variable,
                ..
            } if expression == QueryExprId::new(5) && bound_variable == variable
        ));
    }
}
