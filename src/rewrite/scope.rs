use crate::model::CoreQuery;
use oxrdf::Variable;

use super::variables::{pattern_variables, push_unique};

/// Returns the variables in scope for a core query.
///
/// Variables are returned once, preserving their first occurrence order.
pub(crate) fn in_scope(query: &CoreQuery) -> Vec<Variable> {
    match query {
        CoreQuery::Empty => Vec::new(),

        CoreQuery::Triple(pattern) => pattern_variables(pattern),

        CoreQuery::And(left, right) | CoreQuery::Union(left, right) => {
            union(in_scope(left), in_scope(right))
        }

        CoreQuery::Diff(left, _) => in_scope(left),

        CoreQuery::Select { variables, input } => intersection(unique(variables), in_scope(input)),

        CoreQuery::Filter { input, .. } => in_scope(input),

        CoreQuery::Bind {
            input, variable, ..
        } => {
            let mut variables = in_scope(input);
            push_unique(&mut variables, variable);
            variables
        }
    }
}

/// Returns variables guaranteed to be bound in every query solution.
pub(crate) fn strongly_bound(query: &CoreQuery) -> Vec<Variable> {
    match query {
        CoreQuery::Empty => Vec::new(),

        CoreQuery::Triple(pattern) => pattern_variables(pattern),

        CoreQuery::And(left, right) => union(strongly_bound(left), strongly_bound(right)),

        CoreQuery::Union(left, right) => intersection(strongly_bound(left), strongly_bound(right)),

        CoreQuery::Diff(left, _) => strongly_bound(left),

        CoreQuery::Select { variables, input } => {
            intersection(unique(variables), strongly_bound(input))
        }

        CoreQuery::Filter { input, .. } => strongly_bound(input),

        CoreQuery::Bind { input, .. } => strongly_bound(input),
    }
}

/// Returns variables that must be renamed in the right operand of DIFF.
///
/// NPCS Rule 5 introduces fresh variables for variables shared by both
/// operands that are not strongly bound on the left.
pub(crate) fn diff_substitution_variables(left: &CoreQuery, right: &CoreQuery) -> Vec<Variable> {
    let right_scope = in_scope(right);
    let left_strong = strongly_bound(left);

    in_scope(left)
        .into_iter()
        .filter(|variable| right_scope.contains(variable) && !left_strong.contains(variable))
        .collect()
}

fn union(mut left: Vec<Variable>, right: Vec<Variable>) -> Vec<Variable> {
    for variable in right {
        push_unique(&mut left, &variable);
    }

    left
}

fn intersection(left: Vec<Variable>, right: Vec<Variable>) -> Vec<Variable> {
    left.into_iter()
        .filter(|variable| right.contains(variable))
        .collect()
}

fn unique(variables: &[Variable]) -> Vec<Variable> {
    let mut result = Vec::new();

    for variable in variables {
        push_unique(&mut result, variable);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ObjectPattern, PredicatePattern, SubjectPattern, TriplePattern};
    use oxrdf::NamedNode;

    fn variable_pattern(subject: Variable, object: Variable) -> CoreQuery {
        CoreQuery::Triple(TriplePattern {
            subject: SubjectPattern::Variable(subject),
            predicate: PredicatePattern::NamedNode(
                NamedNode::new("https://example.com/p").expect("valid IRI"),
            ),
            object: ObjectPattern::Variable(object),
        })
    }

    #[test]
    fn and_scope_is_union_of_operand_scopes() {
        let x = Variable::new("x").expect("valid variable");
        let y = Variable::new("y").expect("valid variable");
        let z = Variable::new("z").expect("valid variable");

        let query = CoreQuery::And(
            Box::new(variable_pattern(x.clone(), y.clone())),
            Box::new(variable_pattern(y.clone(), z.clone())),
        );

        assert_eq!(in_scope(&query), vec![x, y, z]);
    }

    #[test]
    fn select_scope_is_restricted_to_input_scope() {
        let x = Variable::new("x").expect("valid variable");
        let y = Variable::new("y").expect("valid variable");
        let absent = Variable::new("absent").expect("valid variable");

        let query = CoreQuery::Select {
            variables: vec![x.clone(), absent],
            input: Box::new(variable_pattern(x.clone(), y)),
        };

        assert_eq!(in_scope(&query), vec![x]);
    }

    #[test]
    fn and_strongly_bound_variables_are_combined() {
        let x = Variable::new("x").expect("valid variable");
        let y = Variable::new("y").expect("valid variable");
        let z = Variable::new("z").expect("valid variable");

        let query = CoreQuery::And(
            Box::new(variable_pattern(x.clone(), y.clone())),
            Box::new(variable_pattern(y.clone(), z.clone())),
        );

        assert_eq!(strongly_bound(&query), vec![x, y, z]);
    }

    #[test]
    fn union_requires_variable_to_be_strongly_bound_in_both_branches() {
        let x = Variable::new("x").expect("valid variable");
        let y = Variable::new("y").expect("valid variable");

        let query = CoreQuery::Union(
            Box::new(variable_pattern(x.clone(), y.clone())),
            Box::new(variable_pattern(x.clone(), x.clone())),
        );

        assert_eq!(strongly_bound(&query), vec![x]);
    }

    #[test]
    fn diff_renames_shared_variable_not_strongly_bound_on_left() {
        let x = Variable::new("x").expect("valid variable");
        let y = Variable::new("y").expect("valid variable");

        let left = CoreQuery::Union(
            Box::new(variable_pattern(x.clone(), y.clone())),
            Box::new(variable_pattern(x.clone(), x.clone())),
        );

        let right = variable_pattern(x, y.clone());

        assert_eq!(diff_substitution_variables(&left, &right), vec![y]);
    }
}
