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

        CoreQuery::Select { variables, .. } => unique(variables),

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

fn union(mut left: Vec<Variable>, right: Vec<Variable>) -> Vec<Variable> {
    for variable in right {
        push_unique(&mut left, &variable);
    }

    left
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

    fn variable_pattern(subject: Variable, object: Variable) -> CoreQuery {
        CoreQuery::Triple(TriplePattern {
            subject: SubjectPattern::Variable(subject),
            predicate: PredicatePattern::NamedNode(
                oxrdf::NamedNode::new("https://example.com/p").expect("valid IRI"),
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
}
