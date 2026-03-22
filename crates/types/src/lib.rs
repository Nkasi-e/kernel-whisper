use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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
    /// Where `gpu_usage_pct` came from (NVIDIA `nvidia-smi` vs host-side estimate, etc.).
    #[serde(default)]
    pub gpu_util_source: GpuUtilSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventSource {
    Mock,
    Host,
    Ebpf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GpuUtilSource {
    #[default]
    EstimatedFromCpu,
    NvidiaSmi,
    Mock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub issue: String,
    pub confidence: f32,
    pub message: String,
    pub suggestions: Vec<String>,
    pub ts: DateTime<Utc>,
    /// Plain-language restatement of the signals (what the metrics / flame context mean).
    #[serde(default)]
    pub data_summary: String,
    /// Why this pattern hurts throughput, latency, or cost.
    #[serde(default)]
    pub impact_summary: String,
}

impl Insight {
    /// Builds a fully populated insight (avoids missing-field mistakes at call sites).
    pub fn new_detection(
        issue: impl Into<String>,
        confidence: f32,
        message: impl Into<String>,
        suggestions: Vec<String>,
        data_summary: impl Into<String>,
        impact_summary: impl Into<String>,
    ) -> Self {
        Self {
            issue: issue.into(),
            confidence,
            message: message.into(),
            suggestions,
            ts: Utc::now(),
            data_summary: data_summary.into(),
            impact_summary: impact_summary.into(),
        }
    }
}

/// Inclusive sample counts: each node’s `value` is the number of samples where that frame
/// appeared on the stack from the root down (classic flame graph merge).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlameNode {
    pub name: String,
    pub value: u64,
    pub children: Vec<FlameNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlameProfile {
    pub kind: FlameKind,
    pub total_samples: u64,
    pub updated_at: DateTime<Utc>,
    pub root: FlameNode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlameKind {
    Cpu,
    Gpu,
}

impl Serialize for FlameKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match self {
            FlameKind::Cpu => "cpu",
            FlameKind::Gpu => "gpu",
        })
    }
}

impl<'de> Deserialize<'de> for FlameKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = String::deserialize(d)?;
        match v.as_str() {
            "cpu" => Ok(FlameKind::Cpu),
            "gpu" => Ok(FlameKind::Gpu),
            _ => Err(serde::de::Error::unknown_variant(
                &v,
                &["cpu", "gpu"],
            )),
        }
    }
}

mod playbook;
pub use playbook::{Playbook, PlaybookPanel};
