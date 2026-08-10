//! Native SPARQL provenance computation.
//!
//! SPARQLProof provides native how-provenance computation for SPARQL through
//! query rewriting based on NPCS.

mod api;

pub mod adapter;
pub mod compiler;
pub mod model;
pub mod plan;
pub mod provenance;
pub mod rewrite;

pub use api::{RewriteError, RewriteStrategy, RewrittenQuery, rewrite_sparql};
