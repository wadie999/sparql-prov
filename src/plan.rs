//! Intermediate representation for provenance-aware query rewriting.
//!
//! Rewrite plans describe what the rewritten query must compute without
//! depending on a SPARQL parser, serializer, endpoint, or concrete
//! generated variable names.

use crate::model::TriplePattern;
use oxrdf::Variable;

/// Identity of an internal variable introduced by rewriting.
///
/// Internal variables are intentionally unnamed at this layer. The SPARQL
/// compiler is responsible for assigning concrete fresh variable names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InternalVar(u32);

impl InternalVar {
    /// Creates an internal variable identity.
    pub fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the numeric identity of the variable.
    pub fn index(self) -> u32 {
        self.0
    }
}

/// Allocates fresh internal variables for a rewriting operation.
///
/// The allocator deals only with logical identities. Concrete SPARQL
/// variable names are assigned later by the compiler.
#[derive(Debug, Default)]
pub struct InternalVarAllocator {
    next: u32,
}

impl InternalVarAllocator {
    /// Creates an allocator starting from the first internal identity.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a fresh internal variable.
    pub fn fresh(&mut self) -> InternalVar {
        let variable = InternalVar::new(self.next);
        self.next = self
            .next
            .checked_add(1)
            .expect("internal variable identifier space exhausted");

        variable
    }
}

/// A variable referenced by a rewrite plan.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlanVar {
    /// Variable originating from the input query.
    User(Variable),

    /// Fresh variable introduced by rewriting.
    Internal(InternalVar),
}

/// Opaque reference to an expression originating from the input query.
///
/// FILTER and BIND expressions are not interpreted by the NPCS rewriting
/// rules. They are retained by reference and emitted unchanged by the
/// eventual SPARQL compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueryExprId(u32);

impl QueryExprId {
    /// Creates an expression reference.
    pub fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the numeric identity of the referenced expression.
    pub fn index(self) -> u32 {
        self.0
    }
}

/// Provenance computation introduced by the rewriting engine.
///
/// These expressions describe computations performed by the rewritten
/// SPARQL query. They are distinct from `ProvenanceExpr`, which represents
/// provenance values obtained after query execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanExpr {
    /// Multiplicative identity used for the empty graph pattern.
    One,

    /// Reference to a value bound by the plan.
    Var(PlanVar),

    /// Aggregate alternative derivations using provenance sum.
    AggregateSum(Box<PlanExpr>),

    /// Combine jointly required provenance values.
    Product(Vec<PlanExpr>),

    /// Compute provenance difference.
    Difference {
        left: Box<PlanExpr>,
        right: Box<PlanExpr>,
    },
}

/// An item projected by a plan-level SELECT operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectItem {
    /// Project an existing variable.
    Var(PlanVar),

    /// Compute an expression and expose it through an alias.
    Expr {
        expression: PlanExpr,
        alias: PlanVar,
    },
}

/// Renaming used by the compatibility-preserving OPTIONAL construction.
///
/// NPCS uses fresh variables for shared variables that are not strongly
/// bound on the left side of DIFF.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariableSubstitution {
    pub from: Variable,
    pub to: PlanVar,
}

/// Logical plan produced by provenance rewriting.
///
/// The plan describes rewritten query structure without containing
/// Spargebra nodes or concrete SPARQL syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewritePlan {
    /// Empty graph pattern.
    Unit,

    /// Match a triple pattern together with its provenance source.
    ReifiedTriple {
        pattern: TriplePattern,
        source: PlanVar,
    },

    /// Join several plan inputs.
    Join(Vec<RewritePlan>),

    /// Union several plan inputs.
    Union(Vec<RewritePlan>),

    /// OPTIONAL with the variable-renaming information required by DIFF.
    CompatibleOptional {
        left: Box<RewritePlan>,
        right: Box<RewritePlan>,
        substitutions: Vec<VariableSubstitution>,
    },

    /// Project variables and expressions, optionally grouping rows.
    Select {
        input: Box<RewritePlan>,
        items: Vec<SelectItem>,
        group_by: Vec<PlanVar>,
    },

    /// Preserve a FILTER expression from the input query.
    Filter {
        input: Box<RewritePlan>,
        condition: QueryExprId,
    },

    /// Preserve a BIND expression from the input query.
    Bind {
        input: Box<RewritePlan>,
        expression: QueryExprId,
        variable: PlanVar,
    },

    /// Bind a provenance computation introduced by rewriting.
    Extend {
        input: Box<RewritePlan>,
        expression: PlanExpr,
        variable: PlanVar,
    },
}

/// Result of rewriting one query subtree.
///
/// Besides the plan itself, callers need to know which internal variable
/// contains that subtree's provenance annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteFragment {
    pub plan: RewritePlan,
    pub provenance: PlanVar,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_variables_have_stable_identity() {
        let first = InternalVar::new(1);
        let second = InternalVar::new(2);

        assert_eq!(first.index(), 1);
        assert_ne!(first, second);
    }

    #[test]
    fn allocator_returns_distinct_variables_in_order() {
        let mut allocator = InternalVarAllocator::new();

        let first = allocator.fresh();
        let second = allocator.fresh();
        let third = allocator.fresh();

        assert_eq!(first.index(), 0);
        assert_eq!(second.index(), 1);
        assert_eq!(third.index(), 2);

        assert_ne!(first, second);
        assert_ne!(second, third);
        assert_ne!(first, third);
    }

    #[test]
    fn independent_allocators_have_independent_identity_spaces() {
        let mut first_allocator = InternalVarAllocator::new();
        let mut second_allocator = InternalVarAllocator::new();

        assert_eq!(first_allocator.fresh(), InternalVar::new(0));
        assert_eq!(second_allocator.fresh(), InternalVar::new(0));
    }

    #[test]
    fn query_expression_ids_are_opaque_stable_references() {
        let expression = QueryExprId::new(7);

        assert_eq!(expression.index(), 7);
    }

    #[test]
    fn represents_empty_pattern_rewrite() {
        let provenance = PlanVar::Internal(InternalVar::new(0));

        let fragment = RewriteFragment {
            plan: RewritePlan::Extend {
                input: Box::new(RewritePlan::Unit),
                expression: PlanExpr::One,
                variable: provenance.clone(),
            },
            provenance: provenance.clone(),
        };

        assert_eq!(fragment.provenance, provenance);

        assert!(matches!(
            fragment.plan,
            RewritePlan::Extend {
                expression: PlanExpr::One,
                ..
            }
        ));
    }

    #[test]
    fn represents_provenance_product() {
        let left = PlanVar::Internal(InternalVar::new(1));
        let right = PlanVar::Internal(InternalVar::new(2));

        let expression = PlanExpr::Product(vec![
            PlanExpr::Var(left.clone()),
            PlanExpr::Var(right.clone()),
        ]);

        assert_eq!(
            expression,
            PlanExpr::Product(vec![PlanExpr::Var(left), PlanExpr::Var(right)])
        );
    }

    #[test]
    fn represents_nested_provenance_computation() {
        let left = PlanVar::Internal(InternalVar::new(1));
        let right = PlanVar::Internal(InternalVar::new(2));

        let expression = PlanExpr::Difference {
            left: Box::new(PlanExpr::Var(left)),
            right: Box::new(PlanExpr::AggregateSum(Box::new(PlanExpr::Var(right)))),
        };

        assert!(matches!(expression, PlanExpr::Difference { .. }));
    }
}
