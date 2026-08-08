use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};
use oxrdf::Variable;

/// Returns the variables occurring in a triple pattern.
///
/// Variables are returned once, preserving their first occurrence order.
pub(crate) fn pattern_variables(pattern: &TriplePattern) -> Vec<Variable> {
    let mut variables = Vec::new();
    collect_pattern_variables(pattern, &mut variables);
    variables
}

pub(crate) fn push_unique(variables: &mut Vec<Variable>, variable: &Variable) {
    if !variables.contains(variable) {
        variables.push(variable.clone());
    }
}

fn collect_pattern_variables(pattern: &TriplePattern, variables: &mut Vec<Variable>) {
    match &pattern.subject {
        SubjectPattern::Variable(variable) => {
            push_unique(variables, variable);
        }
        SubjectPattern::Triple(pattern) => {
            collect_pattern_variables(pattern, variables);
        }
        SubjectPattern::NamedNode(_) | SubjectPattern::BlankNode(_) => {}
    }

    if let PredicatePattern::Variable(variable) = &pattern.predicate {
        push_unique(variables, variable);
    }

    match &pattern.object {
        ObjectPattern::Variable(variable) => {
            push_unique(variables, variable);
        }
        ObjectPattern::Triple(pattern) => {
            collect_pattern_variables(pattern, variables);
        }
        ObjectPattern::NamedNode(_) | ObjectPattern::BlankNode(_) | ObjectPattern::Literal(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_pattern_variables_in_first_occurrence_order() {
        let subject = Variable::new("s").expect("valid variable");
        let predicate = Variable::new("p").expect("valid variable");
        let object = Variable::new("o").expect("valid variable");

        let pattern = TriplePattern {
            subject: SubjectPattern::Variable(subject.clone()),
            predicate: PredicatePattern::Variable(predicate.clone()),
            object: ObjectPattern::Variable(object.clone()),
        };

        assert_eq!(
            pattern_variables(&pattern),
            vec![subject, predicate, object]
        );
    }

    #[test]
    fn does_not_duplicate_repeated_variables() {
        let variable = Variable::new("x").expect("valid variable");

        let pattern = TriplePattern {
            subject: SubjectPattern::Variable(variable.clone()),
            predicate: PredicatePattern::Variable(variable.clone()),
            object: ObjectPattern::Variable(variable.clone()),
        };

        assert_eq!(pattern_variables(&pattern), vec![variable]);
    }
}
