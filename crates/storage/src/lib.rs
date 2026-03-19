use async_trait::async_trait;
use kw_types::Insight;
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait]
pub trait InsightStore: Send + Sync {
    async fn put(&self, insight: Insight) -> anyhow::Result<()>;
    async fn latest(&self, limit: usize) -> anyhow::Result<Vec<Insight>>;
}

#[derive(Clone, Default)]
pub struct InMemoryStore {
    insights: Arc<RwLock<Vec<Insight>>>,
}

#[async_trait]
impl InsightStore for InMemoryStore {
    async fn put(&self, insight: Insight) -> anyhow::Result<()> {
        let mut guard = self.insights.write().await;
        guard.push(insight);
        Ok(())
    }

    async fn latest(&self, limit: usize) -> anyhow::Result<Vec<Insight>> {
        let guard = self.insights.read().await;
        let len = guard.len();
        let start = len.saturating_sub(limit);
        Ok(guard[start..].to_vec())
    }
}

#[cfg(feature = "clickhouse")]
pub mod clickhouse_store {
    use super::InsightStore;
    use async_trait::async_trait;
    use kw_types::Insight;

    pub struct ClickHouseStore {
        _endpoint: String,
    }

    impl ClickHouseStore {
        pub fn new(endpoint: impl Into<String>) -> Self {
            Self {
                _endpoint: endpoint.into(),
            }
        }
    }

    #[async_trait]
    impl InsightStore for ClickHouseStore {
        async fn put(&self, _insight: Insight) -> anyhow::Result<()> {
            // Stub: define schema and insert pipeline in next iteration.
            Ok(())
        }

        async fn latest(&self, _limit: usize) -> anyhow::Result<Vec<Insight>> {
            Ok(vec![])
        }
    }
}
