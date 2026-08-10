//! Resolution of logical plan variables into concrete SPARQL variables.

use std::collections::{HashMap, HashSet};

use oxrdf::Variable;

use crate::plan::{InternalVar, PlanVar};

const INTERNAL_VARIABLE_PREFIX: &str = "__sparql_proof";

/// Resolves logical plan variables to concrete SPARQL variables.
///
/// User variables retain their original names. Internal variables receive
/// deterministic generated names that do not collide with variables
/// reserved by the input query.
#[derive(Debug)]
pub(super) struct PlanVariableResolver {
    reserved_names: HashSet<String>,
    resolved_internal: HashMap<InternalVar, Variable>,
}

impl PlanVariableResolver {
    /// Creates a resolver that avoids all variables from the input query.
    pub(super) fn new(reserved_variables: &[Variable]) -> Self {
        Self {
            reserved_names: reserved_variables
                .iter()
                .map(|variable| variable.as_str().to_owned())
                .collect(),
            resolved_internal: HashMap::new(),
        }
    }

    /// Resolves a logical plan variable to a concrete SPARQL variable.
    pub(super) fn resolve(&mut self, variable: &PlanVar) -> Variable {
        match variable {
            PlanVar::User(variable) => variable.clone(),
            PlanVar::Internal(internal) => self.resolve_internal(*internal),
        }
    }

    fn resolve_internal(&mut self, internal: InternalVar) -> Variable {
        if let Some(variable) = self.resolved_internal.get(&internal) {
            return variable.clone();
        }

        let variable = self.allocate_name(internal);

        self.reserved_names
            .insert(variable.as_str().to_owned());

        self.resolved_internal
            .insert(internal, variable.clone());

        variable
    }

    fn allocate_name(&self, internal: InternalVar) -> Variable {
        let base = format!("{INTERNAL_VARIABLE_PREFIX}_{}", internal.index());

        if !self.reserved_names.contains(&base) {
            return generated_variable(base);
        }

        let mut disambiguator = 1_u32;

        loop {
            let candidate = format!("{base}_{disambiguator}");

            if !self.reserved_names.contains(&candidate) {
                return generated_variable(candidate);
            }

            disambiguator = disambiguator
                .checked_add(1)
                .expect("generated SPARQL variable namespace exhausted");
        }
    }
}

fn generated_variable(name: String) -> Variable {
    Variable::new(name).expect("compiler-generated variable name must be valid SPARQL")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_variables_keep_their_identity() {
        let user = Variable::new("person").expect("valid SPARQL variable");

        let mut resolver = PlanVariableResolver::new(std::slice::from_ref(&user));

        assert_eq!(
            resolver.resolve(&PlanVar::User(user.clone())),
            user
        );
    }

    #[test]
    fn internal_variables_receive_deterministic_names() {
        let mut resolver = PlanVariableResolver::new(&[]);

        let variable = PlanVar::Internal(InternalVar::new(7));

        let first = resolver.resolve(&variable);
        let second = resolver.resolve(&variable);

        assert_eq!(first, second);
        assert_eq!(first.as_str(), "__sparql_proof_7");
    }

    #[test]
    fn distinct_internal_variables_receive_distinct_names() {
        let mut resolver = PlanVariableResolver::new(&[]);

        let first = resolver.resolve(&PlanVar::Internal(InternalVar::new(1)));
        let second = resolver.resolve(&PlanVar::Internal(InternalVar::new(2)));

        assert_eq!(first.as_str(), "__sparql_proof_1");
        assert_eq!(second.as_str(), "__sparql_proof_2");
        assert_ne!(first, second);
    }

    #[test]
    fn internal_variables_avoid_user_variable_collisions() {
        let reserved =
            Variable::new("__sparql_proof_0").expect("valid SPARQL variable");

        let mut resolver = PlanVariableResolver::new(&[reserved]);

        let generated =
            resolver.resolve(&PlanVar::Internal(InternalVar::new(0)));

        assert_eq!(generated.as_str(), "__sparql_proof_0_1");
    }

    #[test]
    fn collision_resolution_skips_all_reserved_candidates() {
        let first =
            Variable::new("__sparql_proof_0").expect("valid SPARQL variable");

        let second =
            Variable::new("__sparql_proof_0_1").expect("valid SPARQL variable");

        let mut resolver = PlanVariableResolver::new(&[first, second]);

        let generated =
            resolver.resolve(&PlanVar::Internal(InternalVar::new(0)));

        assert_eq!(generated.as_str(), "__sparql_proof_0_2");
    }

    #[test]
    fn generated_names_do_not_depend_on_resolution_order() {
        let mut first_resolver = PlanVariableResolver::new(&[]);
        let mut second_resolver = PlanVariableResolver::new(&[]);

        let zero = PlanVar::Internal(InternalVar::new(0));
        let two = PlanVar::Internal(InternalVar::new(2));

        let first_zero = first_resolver.resolve(&zero);
        let first_two = first_resolver.resolve(&two);

        let second_two = second_resolver.resolve(&two);
        let second_zero = second_resolver.resolve(&zero);

        assert_eq!(first_zero, second_zero);
        assert_eq!(first_two, second_two);
    }
}