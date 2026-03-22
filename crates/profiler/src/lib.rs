pub mod native;

use chrono::Utc;
use kw_types::{FlameKind, FlameNode, FlameProfile, GpuUtilSource, TraceEvent};

const MAX_CHILDREN_PER_NODE: usize = 24;

/// Telemetry drives the GPU utilization tree; CPU flames come from [`native::capture_cpu_stacks`].
#[derive(Debug)]
pub struct ProfileAggregator {
    cpu_root: FlameNode,
    gpu_root: FlameNode,
    cpu_total: u64,
    gpu_total: u64,
}

impl ProfileAggregator {
    pub fn new() -> Self {
        Self {
            cpu_root: FlameNode {
                name: "cpu".to_string(),
                value: 0,
                children: Vec::new(),
            },
            gpu_root: FlameNode {
                name: "gpu".to_string(),
                value: 0,
                children: Vec::new(),
            },
            cpu_total: 0,
            gpu_total: 0,
        }
    }

    /// One telemetry tick: merge a small GPU tree from real or estimated utilization (never fake stacks for NVIDIA).
    pub fn ingest_telemetry(&mut self, event: &TraceEvent) {
        add_path(
            &mut self.gpu_root,
            &gpu_stack_from_telemetry(event),
            1,
        );
        self.gpu_total = self.gpu_total.saturating_add(1);
        sort_and_cap(&mut self.gpu_root);
    }

    /// Merge stacks from `sample` / `perf` (inclusive counts).
    pub fn merge_native_cpu(&mut self, stacks: &[Vec<String>]) {
        for p in stacks {
            if p.is_empty() {
                continue;
            }
            add_path(&mut self.cpu_root, p, 1);
            self.cpu_total = self.cpu_total.saturating_add(1);
        }
        sort_and_cap(&mut self.cpu_root);
    }

    pub fn cpu_profile(&self) -> FlameProfile {
        FlameProfile {
            kind: FlameKind::Cpu,
            total_samples: self.cpu_total,
            updated_at: Utc::now(),
            root: self.cpu_root.clone(),
        }
    }

    pub fn gpu_profile(&self) -> FlameProfile {
        FlameProfile {
            kind: FlameKind::Gpu,
            total_samples: self.gpu_total,
            updated_at: Utc::now(),
            root: self.gpu_root.clone(),
        }
    }
}

fn gpu_stack_from_telemetry(event: &TraceEvent) -> Vec<String> {
    let pct = event.gpu_usage_pct.clamp(0.0, 100.0);
    let pct_lbl = format!("{:.0}%", pct);
    match event.gpu_util_source {
        GpuUtilSource::NvidiaSmi => vec!["nvidia_smi".to_string(), pct_lbl],
        GpuUtilSource::EstimatedFromCpu => vec!["host_estimate".to_string(), pct_lbl],
        GpuUtilSource::Mock => vec!["mock_tracer".to_string(), pct_lbl],
    }
}

fn add_path(node: &mut FlameNode, frames: &[String], weight: u64) {
    node.value = node.value.saturating_add(weight);
    if frames.is_empty() {
        return;
    }
    let name = frames[0].clone();
    let child = match node.children.iter_mut().find(|c| c.name == name) {
        Some(c) => c,
        None => {
            node.children.push(FlameNode {
                name,
                value: 0,
                children: Vec::new(),
            });
            node.children.last_mut().expect("just pushed")
        }
    };
    add_path(child, &frames[1..], weight);
}

fn sort_and_cap(node: &mut FlameNode) {
    node.children.sort_by(|a, b| b.value.cmp(&a.value));
    if node.children.len() > MAX_CHILDREN_PER_NODE {
        let rest = node.children.split_off(MAX_CHILDREN_PER_NODE);
        let tail: u64 = rest.iter().map(|c| c.value).sum();
        let merged_names = rest.len();
        if tail > 0 {
            node.children.push(FlameNode {
                name: format!("… other ×{}", merged_names),
                value: tail,
                children: Vec::new(),
            });
        }
    }
    for c in &mut node.children {
        sort_and_cap(c);
    }
}
