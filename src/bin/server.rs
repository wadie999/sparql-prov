use std::env;

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use oxrdf::NamedNode;
use serde::{Deserialize, Serialize};

use sparql_proof::{RewriteStrategy, compiler::RdfStarReification, rewrite_sparql};

const DEFAULT_ADDRESS: &str = "127.0.0.1:3000";
const MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum HttpRewriteStrategy {
    Base,
    Optimized,
}

impl From<HttpRewriteStrategy> for RewriteStrategy {
    fn from(strategy: HttpRewriteStrategy) -> Self {
        match strategy {
            HttpRewriteStrategy::Base => RewriteStrategy::Base,
            HttpRewriteStrategy::Optimized => RewriteStrategy::Optimized,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RewriteRequest {
    query: String,

    #[serde(default)]
    strategy: Option<HttpRewriteStrategy>,

    provenance_predicate: String,
}

#[derive(Debug, Serialize)]
struct RewriteResponse {
    sparql: String,
    provenance_variable: String,
    strategy: HttpRewriteStrategy,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn client(message: String) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message,
        }
    }

    fn server(message: String) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = env::var("SPARQL_PROOF_ADDR").unwrap_or_else(|_| DEFAULT_ADDRESS.to_owned());

    let app = Router::new()
        .route("/health", get(health))
        .route("/rewrite", post(rewrite))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES));

    let listener = tokio::net::TcpListener::bind(&address).await?;

    println!("SPARQLProof server listening on http://{address}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn rewrite(Json(request): Json<RewriteRequest>) -> Result<Json<RewriteResponse>, ApiError> {
    tokio::task::spawn_blocking(move || rewrite_request(request))
        .await
        .map_err(|error| ApiError::server(format!("rewrite worker failed: {error}")))?
}

fn rewrite_request(request: RewriteRequest) -> Result<Json<RewriteResponse>, ApiError> {
    let strategy = request.strategy.unwrap_or(HttpRewriteStrategy::Optimized);

    let provenance_predicate = NamedNode::new(request.provenance_predicate)
        .map_err(|error| ApiError::client(format!("invalid provenance predicate: {error}")))?;

    let reification = RdfStarReification::new(provenance_predicate);

    let rewritten = rewrite_sparql(&request.query, strategy.into(), reification)
        .map_err(|error| ApiError::client(error.to_string()))?;

    Ok(Json(RewriteResponse {
        sparql: rewritten.sparql().to_owned(),
        provenance_variable: rewritten.provenance_variable().to_string(),
        strategy,
    }))
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("failed to install shutdown signal: {error}");
    }
}
