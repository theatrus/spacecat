//! Axum web server for the hub.

use super::config::HubConfig;
use super::db::{Db, DbError};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum HubError {
    #[error("hub database error: {0}")]
    Db(#[from] DbError),
    #[error("hub I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Shared state handed to every request handler.
#[derive(Clone)]
pub struct HubState {
    pub db: Db,
    pub config: Arc<HubConfig>,
}

pub fn router(state: HubState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/healthz", get(healthz))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(
        "<!doctype html><title>Chatstronomy hub</title>\
         <h1>Chatstronomy hub</h1>\
         <p>space | cat — observatory chat hub. Login arrives in a later phase.</p>",
    )
}

/// Liveness and readiness in one: proves the process is up and the database
/// answers a query.
async fn healthz(State(state): State<HubState>) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.schema_version() {
        Ok(version) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "version": crate::version::VERSION_STRING,
                "schema_version": version,
            })),
        ),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "error",
                "error": e.to_string(),
            })),
        ),
    }
}

/// Open the database, bind, and serve until ctrl-c.
pub async fn run(config: HubConfig) -> Result<(), HubError> {
    let db = Db::open(&config.database_path)?;
    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;
    println!(
        "Hub listening on http://{} (database: {})",
        listener.local_addr()?,
        config.database_path
    );
    serve(listener, config, db).await
}

/// Serve on an already-bound listener. Split from `run` so tests can bind
/// port 0 and use an in-memory database.
pub async fn serve(
    listener: tokio::net::TcpListener,
    config: HubConfig,
    db: Db,
) -> Result<(), HubError> {
    let state = HubState {
        db,
        config: Arc::new(config),
    };
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    println!("Hub shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn spawn_test_hub() -> String {
        let db = Db::open_in_memory().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener, HubConfig::default(), db));
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn healthz_reports_ok_and_schema_version() {
        let base = spawn_test_hub().await;
        let response = reqwest::get(format!("{base}/healthz")).await.unwrap();
        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        assert!(body["schema_version"].as_u64().unwrap() >= 1);
        assert!(body["version"].is_string());
    }

    #[tokio::test]
    async fn index_serves_html() {
        let base = spawn_test_hub().await;
        let response = reqwest::get(&base).await.unwrap();
        assert_eq!(response.status(), 200);
        let body = response.text().await.unwrap();
        assert!(body.contains("Chatstronomy hub"));
    }
}
