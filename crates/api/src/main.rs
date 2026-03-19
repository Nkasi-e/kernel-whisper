use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use kw_engine::DetectionEngine;
use kw_storage::{InMemoryStore, InsightStore};
use kw_tracer::start_from_env;
use kw_types::Insight;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

#[derive(Clone)]
struct AppState {
    store: InMemoryStore,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .compact()
        .init();

    let store = InMemoryStore::default();
    let state = AppState {
        store: store.clone(),
    };
    let engine = Arc::new(Mutex::new(DetectionEngine::new(20)));

    let mut stream = start_from_env(500).await?;
    let store_bg = store.clone();
    let engine_bg = Arc::clone(&engine);

    tokio::spawn(async move {
        while let Some(event) = stream.next().await {
            let maybe_insight = {
                let mut guard = engine_bg.lock().await;
                guard.ingest(event)
            };
            if let Some(insight) = maybe_insight {
                if let Err(err) = store_bg.put(insight.clone()).await {
                    tracing::warn!(?err, "failed storing insight");
                } else {
                    info!(
                        issue = %insight.issue,
                        confidence = insight.confidence,
                        "detected inefficiency"
                    );
                }
            }
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/insights", get(list_insights))
        .with_state(state)
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("kernelwhisper api listening on 0.0.0.0:3000");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn list_insights(State(state): State<AppState>) -> Json<Vec<Insight>> {
    let items = state.store.latest(50).await.unwrap_or_default();
    Json(items)
}
