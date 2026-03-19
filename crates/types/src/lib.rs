use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub id: Uuid,
    pub ts: DateTime<Utc>,
    pub cpu_usage_pct: f32,
    pub runnable_tasks: u32,
    pub blocked_syscalls: u32,
    pub gpu_usage_pct: f32,
    pub source: EventSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventSource {
    Mock,
    Host,
    Ebpf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub issue: String,
    pub confidence: f32,
    pub message: String,
    pub suggestions: Vec<String>,
    pub ts: DateTime<Utc>,
}
