# sparql-prov

A Rust library for native SPARQL how-provenance.

## HTTP API

Start SPARQLProv:

```bash
cargo run --release --features server --bin sparql-proof-server
```

Rewrite a SPARQL query:

```bash
curl -X POST http://127.0.0.1:3000/rewrite \
  -H 'content-type: application/json' \
  -d '{
    "query": "SELECT ?person ?city WHERE { ?person <http://example.com/bornIn> ?place . ?place <http://example.com/locatedIn> ?city . }",
    "strategy": "optimized",
    "provenance_predicate": "http://example.com/wasDerivedFrom"
  }'
```

Response:

```json
{
  "sparql": "SELECT * WHERE { ... }",
  "provenance_variable": "?__sparql_proof_0",
  "strategy": "optimized"
}
```

`base` and `optimized` implement the two NPCS rewriting strategies.

Health check:

```bash
curl http://127.0.0.1:3000/health
```

To expose the service on the network:

```bash
SPARQL_PROOF_ADDR=0.0.0.0:3000 \
cargo run --release --features server --bin sparql-proof-server
```

## Built on 

The [NPCS method](https://doi.org/10.1145/3589334.3645557) and the Java implementation in the [NPCS repository](https://github.com/ZubariaForthAcc/NPCS).
