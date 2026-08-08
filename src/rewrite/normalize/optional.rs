use crate::model::CoreQuery;

/// Normalizes SPARQL OPTIONAL into the NPCS core query algebra.
///
/// NPCS 2024 rewrites
///
/// `P1 OPTIONAL P2`
///
/// as
///
/// `(P1 DIFF P2) UNION (P1 AND P2)`.
///
/// The operands are owned because each must occur twice in the normalized
/// query. One occurrence is cloned and the other consumes the original.
pub fn normalize_optional(left: CoreQuery, right: CoreQuery) -> CoreQuery {
    CoreQuery::Union(
        Box::new(CoreQuery::Diff(
            Box::new(left.clone()),
            Box::new(right.clone()),
        )),
        Box::new(CoreQuery::And(Box::new(left), Box::new(right))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_optional_into_diff_union_and() {
        let left = CoreQuery::Empty;
        let right = CoreQuery::Empty;

        let normalized = normalize_optional(left.clone(), right.clone());

        assert_eq!(
            normalized,
            CoreQuery::Union(
                Box::new(CoreQuery::Diff(
                    Box::new(left.clone()),
                    Box::new(right.clone()),
                )),
                Box::new(CoreQuery::And(Box::new(left), Box::new(right),)),
            )
        );
    }
}
