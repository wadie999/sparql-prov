//! Compiler-side application of NPCS variable substitutions.
//!
//! `VariableSubstitution` belongs to the logical rewrite plan. This module
//! interprets those substitutions while lowering the plan without mutating
//! the plan itself.

use std::collections::HashSet;

use oxrdf::Variable;

use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};
use crate::plan::{PlanVar, VariableSubstitution};

use super::input_expression::ExpressionVariableSubstitution;
use super::variables::PlanVariableResolver;

/// Resolves a plan variable under an active NPCS substitution environment.
pub(super) fn resolve_plan_variable(
    variable: &PlanVar,
    substitutions: &[VariableSubstitution],
    variables: &mut PlanVariableResolver,
) -> Variable {
    match variable {
        PlanVar::User(variable) => resolve_user_variable(variable, substitutions, variables),

        PlanVar::Internal(_) => variables.resolve(variable),
    }
}

/// Resolves an input-query variable under an active substitution environment.
///
/// Later substitutions take precedence. This allows a locally introduced ν
/// to override an outer substitution when compiling a nested right operand.
pub(super) fn resolve_user_variable(
    variable: &Variable,
    substitutions: &[VariableSubstitution],
    variables: &mut PlanVariableResolver,
) -> Variable {
    substitutions
        .iter()
        .rev()
        .find(|substitution| substitution.from == *variable)
        .map_or_else(
            || variable.clone(),
            |substitution| variables.resolve(&substitution.to),
        )
}

/// Extends an outer substitution environment with a local ν.
pub(super) fn extend(
    outer: &[VariableSubstitution],
    local: &[VariableSubstitution],
) -> Vec<VariableSubstitution> {
    let mut substitutions = Vec::with_capacity(outer.len() + local.len());

    substitutions.extend_from_slice(outer);
    substitutions.extend_from_slice(local);

    substitutions
}

/// Converts the active logical substitutions into concrete substitutions for
/// the adapter that owns preserved input expressions.
///
/// Only the effective substitution for each source variable is exposed.
pub(super) fn input_expression_substitutions(
    substitutions: &[VariableSubstitution],
    variables: &mut PlanVariableResolver,
) -> Vec<ExpressionVariableSubstitution> {
    let mut seen = HashSet::new();
    let mut concrete = Vec::new();

    for substitution in substitutions.iter().rev() {
        if seen.insert(substitution.from.clone()) {
            concrete.push(ExpressionVariableSubstitution {
                from: substitution.from.clone(),
                to: variables.resolve(&substitution.to),
            });
        }
    }

    concrete.reverse();
    concrete
}

/// Applies the active substitution environment to a triple pattern.
pub(super) fn triple_pattern(
    pattern: &TriplePattern,
    substitutions: &[VariableSubstitution],
    variables: &mut PlanVariableResolver,
) -> TriplePattern {
    TriplePattern {
        subject: subject_pattern(&pattern.subject, substitutions, variables),
        predicate: predicate_pattern(&pattern.predicate, substitutions, variables),
        object: object_pattern(&pattern.object, substitutions, variables),
    }
}

fn subject_pattern(
    pattern: &SubjectPattern,
    substitutions: &[VariableSubstitution],
    variables: &mut PlanVariableResolver,
) -> SubjectPattern {
    match pattern {
        SubjectPattern::NamedNode(node) => SubjectPattern::NamedNode(node.clone()),

        SubjectPattern::BlankNode(node) => SubjectPattern::BlankNode(node.clone()),

        SubjectPattern::Variable(variable) => {
            SubjectPattern::Variable(resolve_user_variable(variable, substitutions, variables))
        }

        SubjectPattern::Triple(pattern) => {
            SubjectPattern::Triple(Box::new(triple_pattern(pattern, substitutions, variables)))
        }
    }
}

fn predicate_pattern(
    pattern: &PredicatePattern,
    substitutions: &[VariableSubstitution],
    variables: &mut PlanVariableResolver,
) -> PredicatePattern {
    match pattern {
        PredicatePattern::NamedNode(node) => PredicatePattern::NamedNode(node.clone()),

        PredicatePattern::Variable(variable) => {
            PredicatePattern::Variable(resolve_user_variable(variable, substitutions, variables))
        }
    }
}

fn object_pattern(
    pattern: &ObjectPattern,
    substitutions: &[VariableSubstitution],
    variables: &mut PlanVariableResolver,
) -> ObjectPattern {
    match pattern {
        ObjectPattern::NamedNode(node) => ObjectPattern::NamedNode(node.clone()),

        ObjectPattern::BlankNode(node) => ObjectPattern::BlankNode(node.clone()),

        ObjectPattern::Literal(literal) => ObjectPattern::Literal(literal.clone()),

        ObjectPattern::Variable(variable) => {
            ObjectPattern::Variable(resolve_user_variable(variable, substitutions, variables))
        }

        ObjectPattern::Triple(pattern) => {
            ObjectPattern::Triple(Box::new(triple_pattern(pattern, substitutions, variables)))
        }
    }
}

#[cfg(test)]
mod tests {
    use oxrdf::{NamedNode, Variable};

    use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};
    use crate::plan::{InternalVar, PlanVar, VariableSubstitution};

    use super::*;

    #[test]
    fn resolves_user_variable_through_nu() {
        let original = Variable::new("y").expect("valid variable");

        let substitution = VariableSubstitution {
            from: original.clone(),
            to: PlanVar::Internal(InternalVar::new(7)),
        };

        let mut variables = PlanVariableResolver::new(std::slice::from_ref(&original));

        let resolved = resolve_user_variable(&original, &[substitution], &mut variables);

        assert_eq!(resolved.as_str(), "__sparql_proof_7");
    }

    #[test]
    fn leaves_unsubstituted_user_variable_unchanged() {
        let variable = Variable::new("x").expect("valid variable");

        let mut variables = PlanVariableResolver::new(std::slice::from_ref(&variable));

        let resolved = resolve_user_variable(&variable, &[], &mut variables);

        assert_eq!(resolved, variable);
    }

    #[test]
    fn local_substitution_overrides_outer_substitution() {
        let variable = Variable::new("y").expect("valid variable");

        let outer = VariableSubstitution {
            from: variable.clone(),
            to: PlanVar::Internal(InternalVar::new(1)),
        };

        let local = VariableSubstitution {
            from: variable.clone(),
            to: PlanVar::Internal(InternalVar::new(2)),
        };

        let substitutions = extend(&[outer], &[local]);

        let mut variables = PlanVariableResolver::new(std::slice::from_ref(&variable));

        let resolved = resolve_user_variable(&variable, &substitutions, &mut variables);

        assert_eq!(resolved.as_str(), "__sparql_proof_2");
    }

    #[test]
    fn applies_nu_recursively_inside_triple_terms() {
        let y = Variable::new("y").expect("valid variable");

        let nested = TriplePattern {
            subject: SubjectPattern::Variable(y.clone()),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("http://example.com/p").expect("valid IRI"),
            ),
            object: ObjectPattern::Variable(y.clone()),
        };

        let pattern = TriplePattern {
            subject: SubjectPattern::Triple(Box::new(nested)),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("http://example.com/q").expect("valid IRI"),
            ),
            object: ObjectPattern::Variable(y.clone()),
        };

        let substitution = VariableSubstitution {
            from: y.clone(),
            to: PlanVar::Internal(InternalVar::new(4)),
        };

        let mut variables = PlanVariableResolver::new(std::slice::from_ref(&y));

        let renamed = triple_pattern(&pattern, &[substitution], &mut variables);

        let expected = Variable::new("__sparql_proof_4").expect("valid variable");

        assert_eq!(
            renamed,
            TriplePattern {
                subject: SubjectPattern::Triple(Box::new(TriplePattern {
                    subject: SubjectPattern::Variable(expected.clone(),),
                    predicate: PredicatePattern::NamedNode(
                        NamedNode::new("http://example.com/p").expect("valid IRI"),
                    ),
                    object: ObjectPattern::Variable(expected.clone(),),
                },)),
                predicate: PredicatePattern::NamedNode(
                    NamedNode::new("http://example.com/q").expect("valid IRI"),
                ),
                object: ObjectPattern::Variable(expected),
            }
        );
    }
}
