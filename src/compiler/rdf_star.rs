//! NPCS-style RDF-star reification.

use oxrdf::{NamedNode, Variable};

use crate::model::TriplePattern;

use super::reification::ReificationScheme;
use super::syntax::embedded_triple_pattern;

/// Reifies matched triples using an RDF-star embedded triple annotated by
/// a configurable provenance predicate.
///
/// Conceptually, a triple pattern `T` and source variable `z` become:
///
/// `<< T >> provenance_predicate z .`
///
/// The strategy deliberately performs no grouping or aggregation. If the
/// same triple has several provenance identifiers, normal SPARQL matching
/// exposes one solution for each identifier.
#[derive(Debug, Clone)]
pub struct RdfStarReification {
    provenance_predicate: NamedNode,
}

impl RdfStarReification {
    /// Creates an RDF-star reification strategy.
    pub fn new(provenance_predicate: NamedNode) -> Self {
        Self {
            provenance_predicate,
        }
    }

    /// Returns the predicate used to associate triples with identifiers.
    pub fn provenance_predicate(&self) -> &NamedNode {
        &self.provenance_predicate
    }
}

impl ReificationScheme for RdfStarReification {
    fn reify(&self, pattern: &TriplePattern, source: &Variable) -> String {
        format!(
            "{} {} {} .",
            embedded_triple_pattern(pattern),
            self.provenance_predicate,
            source,
        )
    }
}

#[cfg(test)]
mod tests {
    use oxrdf::{NamedNode, Variable};

    use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};

    use super::*;

    #[test]
    fn reifies_a_triple_without_aggregating_its_source() {
        let strategy = RdfStarReification::new(
            NamedNode::new("http://example.com/wasDerivedFrom").expect("valid IRI"),
        );

        let pattern = TriplePattern {
            subject: SubjectPattern::Variable(Variable::new("person").expect("valid variable")),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("http://example.com/likes").expect("valid IRI"),
            ),
            object: ObjectPattern::Variable(Variable::new("food").expect("valid variable")),
        };

        let source = Variable::new("__sparql_proof_0").expect("valid variable");

        assert_eq!(
            strategy.reify(&pattern, &source),
            "<< ?person <http://example.com/likes> ?food >> \
<http://example.com/wasDerivedFrom> ?__sparql_proof_0 ."
        );
    }

    #[test]
    fn exposes_the_configured_provenance_predicate() {
        let predicate = NamedNode::new("http://example.com/source").expect("valid IRI");

        let strategy = RdfStarReification::new(predicate.clone());

        assert_eq!(strategy.provenance_predicate(), &predicate);
    }
}
