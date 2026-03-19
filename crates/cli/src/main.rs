use kw_engine::DetectionEngine;
use kw_tracer::start_from_env;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut stream = start_from_env(500).await?;
    let mut engine = DetectionEngine::new(20);

    while let Some(event) = stream.next().await {
        if let Some(insight) = engine.ingest(event) {
            println!("{}", serde_json::to_string_pretty(&insight)?);
        }
    }
    Ok(())
}
