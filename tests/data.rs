use oxrdf::NamedNode;

use sparql_proof::adapter::parse_sparql;
use sparql_proof::compiler::{RdfStarReification, SparqlCompiler};
use sparql_proof::plan::{PlanExpr, RewritePlan, SelectItem};
use sparql_proof::rewrite::base;

fn compiler() -> SparqlCompiler<RdfStarReification> {
    SparqlCompiler::new(RdfStarReification::new(
        NamedNode::new("http://example.com/wasDerivedFrom").expect("valid provenance predicate"),
    ))
}

#[test]
fn gold_base_rewriting_of_projected_single_triple() {
    let parsed = parse_sparql(
        r#"
        SELECT ?person
        WHERE {
            ?person <http://example.com/likes> ?food .
        }
        "#,
    )
    .expect("gold query should parse");

    let rewritten = base::rewrite(parsed.query());

    /*
     * NPCS base rewriting must produce:
     *
     * outer SELECT                     projection rule
     *   outer ProvAggSum(...)
     *   GROUP BY ?person
     *
     *     inner SELECT                 triple rule
     *       inner ProvAggSum(...)
     *       GROUP BY ?person ?food
     *
     *         ReifiedTriple
     */

    let RewritePlan::Select {
        input: outer_input,
        items: outer_items,
        group_by: outer_group_by,
    } = &rewritten.plan
    else {
        panic!("projected triple must rewrite to an outer SELECT");
    };

    assert_eq!(outer_group_by.len(), 1);

    assert!(
        matches!(
            outer_items.as_slice(),
            [
                SelectItem::Var(_),
                SelectItem::Expr {
                    expression: PlanExpr::AggregateSum(_),
                    ..
                }
            ]
        ),
        "projection must preserve ?person and aggregate provenance"
    );

    let RewritePlan::Select {
        input: triple_input,
        items: triple_items,
        group_by: triple_group_by,
    } = outer_input.as_ref()
    else {
        panic!("projection input must be the rewritten triple SELECT");
    };

    assert_eq!(
        triple_group_by.len(),
        2,
        "triple provenance must be grouped by ?person and ?food"
    );

    assert!(
        matches!(
            triple_items.as_slice(),
            [
                SelectItem::Var(_),
                SelectItem::Var(_),
                SelectItem::Expr {
                    expression: PlanExpr::AggregateSum(_),
                    ..
                }
            ]
        ),
        "triple rule must preserve both bindings and aggregate source IDs"
    );

    assert!(
        matches!(triple_input.as_ref(), RewritePlan::ReifiedTriple { .. }),
        "the base triple rule must ultimately call Reify"
    );

    let sparql = compiler()
        .compile_with_expressions(&rewritten.plan, parsed.variables(), &parsed)
        .expect("gold rewriting should compile");

    /*
     * Compiler-level requirements.
     *
     * We deliberately avoid asserting fresh-variable numbers.
     * Those names are compiler identities, not NPCS semantics.
     */

    assert!(
        sparql.contains("<< ?person <http://example.com/likes> ?food >>"),
        "compiled query must reify the original triple pattern"
    );

    assert!(
        sparql.contains("<http://example.com/wasDerivedFrom>"),
        "compiled query must use the configured RDF-star provenance predicate"
    );

    assert_eq!(
        sparql.matches("GROUP_CONCAT").count(),
        2,
        "triple annotation aggregation and projection aggregation \
         must remain distinct"
    );

    assert!(
        sparql.contains("GROUP BY ?person ?food"),
        "inner triple rewriting must group by the full triple mapping"
    );

    assert!(
        sparql.contains("GROUP BY ?person"),
        "outer projection rewriting must group by the projected mapping"
    );
}
