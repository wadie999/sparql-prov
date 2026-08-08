//! Core query model used by provenance rewriting.
//!
//! The model captures the SPARQL constructs required by the rewriting
//! engine without depending on a particular SPARQL parser.

use oxrdf::{BlankNode, Literal, NamedNode, Variable};

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
}
