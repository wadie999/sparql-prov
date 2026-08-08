//! Intermediate representation for provenance-aware query rewriting.
//!
//! Rewrite plans describe what the rewritten query must compute without
//! depending on a SPARQL parser, serializer, endpoint, or concrete
//! generated variable names.

use crate::model::{QueryExprId, TriplePattern};
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

/// One entry of the variable substitution `ν` used by NPCS DIFF.
///
/// The substitution maps an original query variable to a fresh internal
/// variable. The target is expected to be allocated by the rewrite context
/// and therefore must not collide with variables from the input query.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariableSubstitution {
    /// Variable occurring in the original query.
    pub from: Variable,

    /// Fresh variable replacing it in the renamed right operand.
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

    /// Compatibility-preserving OPTIONAL used by NPCS Rule 5.
    ///
    /// This node represents the construction
    ///
    /// `((left OPTIONAL ν(right)) FILTER Cν)`.
    ///
    /// `substitutions` represents the variable substitution `ν`. Each
    /// substitution must be applied to the entire right operand before it
    /// is evaluated. This includes variable occurrences in triple patterns,
    /// projections, grouping keys, BIND targets, and expressions referenced
    /// by preserved FILTER or BIND expression identifiers.
    ///
    /// Lowering this node must also generate the compatibility condition
    /// `Cν`. For every substitution `y -> ν(y)`, the corresponding condition
    /// is logically:
    ///
    /// `!BOUND(y) || !BOUND(ν(y)) || y = ν(y)`.
    ///
    /// This is not an ordinary OPTIONAL. The renaming and compatibility
    /// condition prevent bindings introduced only by the right operand from
    /// leaking into the solution represented by the left operand.
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

/// Result of rewriting a query.
///
/// Besides the plan itself, callers need to know which internal variable
/// contains the query's provenance annotation.
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
            PlanExpr::Product(vec![PlanExpr::Var(left), PlanExpr::Var(right),])
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

    #[test]
    fn plan_reuses_input_query_expression_identity() {
        let condition = QueryExprId::new(9);

        let plan = RewritePlan::Filter {
            input: Box::new(RewritePlan::Unit),
            condition,
        };

        assert!(matches!(
            plan,
            RewritePlan::Filter {
                condition: plan_condition,
                ..
            } if plan_condition == condition
        ));
    }
}
