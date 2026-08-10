//! Rendering of expressions preserved from the input SPARQL query.
//!
//! The core query model identifies input FILTER and BIND expressions through
//! `QueryExprId` without depending on a particular parser expression AST.
//!
//! The adapter that owns those expressions is therefore responsible for
//! rendering them back to SPARQL. Variable substitutions are part of this
//! contract because NPCS DIFF may rename variables inside preserved
//! expressions when lowering `optionalCν`.

use oxrdf::Variable;

use crate::model::QueryExprId;

/// One concrete variable renaming applied while rendering an input expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionVariableSubstitution {
    /// Variable occurring in the original expression.
    pub from: Variable,

    /// Concrete SPARQL variable replacing it.
    pub to: Variable,
}

/// Renders expressions originating from the input query.
///
/// Implementations normally belong to parser/compiler adapters that retain
/// the original expression representation associated with each `QueryExprId`.
pub trait InputExpressionRenderer {
    /// Renders an input expression as executable SPARQL.
    ///
    /// `substitutions` contains variable renamings that must be applied to
    /// the expression before serialization.
    ///
    /// Returning `None` means that the renderer does not know the supplied
    /// expression identifier.
    fn render(
        &self,
        expression: QueryExprId,
        substitutions: &[ExpressionVariableSubstitution],
    ) -> Option<String>;
}
