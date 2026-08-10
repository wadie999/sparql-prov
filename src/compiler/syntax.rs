//! Serialization helpers for query-model terms used by the compiler.

use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};

/// Serializes a triple pattern without a trailing dot.
pub(super) fn triple_pattern(pattern: &TriplePattern) -> String {
    format!(
        "{} {} {}",
        subject_pattern(&pattern.subject),
        predicate_pattern(&pattern.predicate),
        object_pattern(&pattern.object),
    )
}

/// Serializes a triple pattern as an NPCS-style RDF-star embedded triple.
pub(super) fn embedded_triple_pattern(pattern: &TriplePattern) -> String {
    format!("<< {} >>", triple_pattern(pattern))
}

fn subject_pattern(pattern: &SubjectPattern) -> String {
    match pattern {
        SubjectPattern::NamedNode(node) => node.to_string(),
        SubjectPattern::BlankNode(node) => node.to_string(),
        SubjectPattern::Variable(variable) => variable.to_string(),
        SubjectPattern::Triple(triple) => embedded_triple_pattern(triple),
    }
}

fn predicate_pattern(pattern: &PredicatePattern) -> String {
    match pattern {
        PredicatePattern::NamedNode(node) => node.to_string(),
        PredicatePattern::Variable(variable) => variable.to_string(),
    }
}

fn object_pattern(pattern: &ObjectPattern) -> String {
    match pattern {
        ObjectPattern::NamedNode(node) => node.to_string(),
        ObjectPattern::BlankNode(node) => node.to_string(),
        ObjectPattern::Literal(literal) => literal.to_string(),
        ObjectPattern::Variable(variable) => variable.to_string(),
        ObjectPattern::Triple(triple) => embedded_triple_pattern(triple),
    }
}

#[cfg(test)]
mod tests {
    use oxrdf::{Literal, NamedNode, Variable};

    use super::*;

    #[test]
    fn serializes_basic_triple_pattern() {
        let pattern = TriplePattern {
            subject: SubjectPattern::Variable(Variable::new("person").expect("valid variable")),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("http://example.com/likes").expect("valid IRI"),
            ),
            object: ObjectPattern::Literal(Literal::new_simple_literal("pasta")),
        };

        assert_eq!(
            triple_pattern(&pattern),
            "?person <http://example.com/likes> \"pasta\""
        );
    }

    #[test]
    fn serializes_embedded_triple_pattern() {
        let pattern = TriplePattern {
            subject: SubjectPattern::Variable(Variable::new("person").expect("valid variable")),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("http://example.com/likes").expect("valid IRI"),
            ),
            object: ObjectPattern::Variable(Variable::new("food").expect("valid variable")),
        };

        assert_eq!(
            embedded_triple_pattern(&pattern),
            "<< ?person <http://example.com/likes> ?food >>"
        );
    }

    #[test]
    fn serializes_nested_triple_patterns_recursively() {
        let nested = TriplePattern {
            subject: SubjectPattern::Variable(Variable::new("person").expect("valid variable")),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("http://example.com/likes").expect("valid IRI"),
            ),
            object: ObjectPattern::Variable(Variable::new("food").expect("valid variable")),
        };

        let outer = TriplePattern {
            subject: SubjectPattern::Triple(Box::new(nested)),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("http://example.com/assertedBy").expect("valid IRI"),
            ),
            object: ObjectPattern::Variable(Variable::new("source").expect("valid variable")),
        };

        assert_eq!(
            triple_pattern(&outer),
            "<< ?person <http://example.com/likes> ?food >> \
<http://example.com/assertedBy> ?source"
        );
    }
}
