use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use kw_engine::DetectionEngine;
use kw_profiler::ProfileAggregator;
use kw_storage::{InMemoryStore, InsightStore};
use kw_tracer::start_from_env;
use kw_types::{FlameProfile, Insight, Playbook};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

#[derive(Clone)]
struct AppState {
    store: InMemoryStore,
    profiler: Arc<Mutex<ProfileAggregator>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .compact()
        .init();

    let store = InMemoryStore::default();
    let profiler = Arc::new(Mutex::new(ProfileAggregator::new()));
    let state = AppState {
        store: store.clone(),
        profiler: Arc::clone(&profiler),
    };
    let engine = Arc::new(Mutex::new(DetectionEngine::new(20)));

    let mut stream = start_from_env(500).await?;
    let store_bg = store.clone();
    let engine_bg = Arc::clone(&engine);
    let profiler_bg = Arc::clone(&profiler);

    if let Some(pid) = resolve_profile_pid() {
        let interval_ms = std::env::var("KW_PROFILE_INTERVAL_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2500u64);
        let prof_native = Arc::clone(&profiler);
        info!(
            pid,
            interval_ms,
            "native CPU sampling on (macOS `sample` or Linux `perf`)"
        );
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms));
            loop {
                ticker.tick().await;
                let res =
                    tokio::task::spawn_blocking(move || kw_profiler::native::capture_cpu_stacks(pid))
                        .await;
                match res {
                    Ok(Ok(stacks)) => {
                        let mut g = prof_native.lock().await;
                        g.merge_native_cpu(&stacks);
                    }
                    Ok(Err(e)) => tracing::warn!(?e, "native CPU capture failed"),
                    Err(e) => tracing::warn!(?e, "native CPU task panicked"),
                }
            }
        });
    } else {
        info!("set KW_PROFILE_PID=<pid> or KW_PROFILE_SELF=1 for real CPU flame graphs");
    }

    tokio::spawn(async move {
        while let Some(event) = stream.next().await {
            {
                let mut prof = profiler_bg.lock().await;
                prof.ingest_telemetry(&event);
            }
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
        .route("/v1/profile/cpu", get(cpu_flame))
        .route("/v1/profile/gpu", get(gpu_flame))
        .route("/v1/playbook", get(playbook))
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

async fn cpu_flame(State(state): State<AppState>) -> Json<FlameProfile> {
    let prof = state.profiler.lock().await;
    Json(prof.cpu_profile())
}

async fn gpu_flame(State(state): State<AppState>) -> Json<FlameProfile> {
    let prof = state.profiler.lock().await;
    Json(prof.gpu_profile())
}

async fn playbook() -> Json<Playbook> {
    Json(Playbook::bundled())
}

fn resolve_profile_pid() -> Option<u32> {
    let self_on = std::env::var("KW_PROFILE_SELF")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);
    if self_on {
        return Some(std::process::id());
    }
    std::env::var("KW_PROFILE_PID")
        .ok()
        .and_then(|s| s.parse().ok())
}
