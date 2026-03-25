use kw_types::{GpuUtilSource, Insight, TraceEvent};
use std::collections::VecDeque;

pub struct DetectionEngine {
    window: VecDeque<TraceEvent>,
    window_size: usize,
    sample_count: u64,
    last_hard_detection_sample: u64,
}

// Snapshot tuning (when no hard rule matches).
const SNAPSHOT_INTERVAL_SAMPLES: u64 = 25;
const SNAPSHOT_COOLDOWN_AFTER_HARD_SAMPLES: u64 = 30;

struct WindowStats {
    len: usize,
    avg_cpu: f32,
    avg_gpu: f32,
    avg_blocked: f32,
    avg_runnable: f32,
    gpu_source: GpuUtilSource,
}

impl DetectionEngine {
    pub fn new(window_size: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(window_size),
            window_size,
            sample_count: 0,
            last_hard_detection_sample: 0,
        }
    }

    pub fn ingest(&mut self, event: TraceEvent) -> Option<Insight> {
        self.window.push_back(event);
        self.sample_count = self.sample_count.saturating_add(1);
        while self.window.len() > self.window_size {
            self.window.pop_front();
        }
        let res = self.detect();
        if let Some(ref i) = res {
            // Treat hard detections as the "anchor" for snapshot cooldown.
            if i.issue != "telemetry_snapshot" {
                self.last_hard_detection_sample = self.sample_count;
            }
        }
        res
    }

    fn window_stats(&self) -> Option<WindowStats> {
        if self.window.len() < 5 {
            return None;
        }
        let len = self.window.len() as f32;
        let avg_cpu = self.window.iter().map(|e| e.cpu_usage_pct).sum::<f32>() / len;
        let avg_gpu = self.window.iter().map(|e| e.gpu_usage_pct).sum::<f32>() / len;
        let avg_blocked = self
            .window
            .iter()
            .map(|e| e.blocked_syscalls as f32)
            .sum::<f32>()
            / len;
        let avg_runnable = self
            .window
            .iter()
            .map(|e| e.runnable_tasks as f32)
            .sum::<f32>()
            / len;
        let gpu_source = self
            .window
            .back()
            .map(|e| e.gpu_util_source)
            .unwrap_or_default();
        Some(WindowStats {
            len: self.window.len(),
            avg_cpu,
            avg_gpu,
            avg_blocked,
            avg_runnable,
            gpu_source,
        })
    }

    fn gpu_source_label(src: GpuUtilSource) -> &'static str {
        match src {
            GpuUtilSource::NvidiaSmi => "measured via nvidia-smi",
            GpuUtilSource::EstimatedFromCpu => "estimated from host CPU load (not a GPU counter)",
            GpuUtilSource::Mock => "mock tracer (not real hardware)",
        }
    }

    fn detect(&self) -> Option<Insight> {
        let s = self.window_stats()?;
        if let Some(i) = self.detect_cpu_bottleneck(&s) {
            return Some(i);
        }
        if let Some(i) = self.detect_io_pressure(&s) {
            return Some(i);
        }
        if let Some(i) = self.detect_gpu_underfed(&s) {
            return Some(i);
        }
        if self.sample_count % SNAPSHOT_INTERVAL_SAMPLES == 0
            && (self.last_hard_detection_sample == 0
                || self.sample_count.saturating_sub(self.last_hard_detection_sample)
                    >= SNAPSHOT_COOLDOWN_AFTER_HARD_SAMPLES)
        {
            return Some(self.telemetry_snapshot(&s));
        }
        None
    }

    fn telemetry_snapshot(&self, s: &WindowStats) -> Insight {
        let cpu_norm = (s.avg_cpu / 100.0).clamp(0.0, 1.0);
        let gpu_norm = (s.avg_gpu / 100.0).clamp(0.0, 1.0);
        let blocked_norm = (s.avg_blocked / 20.0).clamp(0.0, 1.0);
        let runnable_norm = (s.avg_runnable / 16.0).clamp(0.0, 1.0);
        let imbalance = (cpu_norm - gpu_norm).abs();
        let confidence = (0.25 + 0.35 * imbalance + 0.2 * blocked_norm + 0.2 * runnable_norm)
            // Keep snapshots below the UI "warning" threshold so they look informational.
            .clamp(0.15, 0.4);

        let data_summary = format!(
            "Rolling snapshot from {} samples: CPU {:.0}%, GPU {:.0}% ({}), runnable {:.0}, blocked {:.0}.",
            s.len,
            s.avg_cpu,
            s.avg_gpu,
            Self::gpu_source_label(s.gpu_source),
            s.avg_runnable,
            s.avg_blocked
        );
        let impact_summary = "No hard bottleneck rule fired in this window. Treat this as a live telemetry summary and continue watching for sustained divergence between CPU and GPU.";

        Insight::new_detection(
            "telemetry_snapshot",
            confidence,
            "Live telemetry snapshot (no strong bottleneck rule matched yet).",
            vec![
                "Keep collecting for another 30-60s to increase signal stability.".to_string(),
                "If GPU stays low while CPU rises, focus on input pipeline and batching."
                    .to_string(),
                "If blocked tasks climb, inspect disk/NFS latency and queue depth.".to_string(),
            ],
            data_summary,
            impact_summary,
        )
    }

    fn detect_cpu_bottleneck(&self, s: &WindowStats) -> Option<Insight> {
        if s.avg_cpu < 80.0 || s.avg_gpu > 45.0 {
            return None;
        }
        let cpu_component = ((s.avg_cpu - 70.0) / 30.0).clamp(0.0, 1.0);
        let gpu_component = ((50.0 - s.avg_gpu) / 50.0).clamp(0.0, 1.0);
        let blocked_component = (s.avg_blocked / 10.0).clamp(0.0, 1.0);
        let confidence =
            (0.5 * cpu_component + 0.35 * gpu_component + 0.15 * blocked_component)
                .clamp(0.1, 0.99);

        let data_summary = format!(
            "Across the last {} samples: host CPU load averages {:.0}% (from the process table), GPU utilization averages {:.0}% ({}). Runnable threads ≈ {:.0}, blocked in uninterruptible I/O ≈ {:.0}.",
            s.len,
            s.avg_cpu,
            s.avg_gpu,
            Self::gpu_source_label(s.gpu_source),
            s.avg_runnable,
            s.avg_blocked
        );
        let impact_summary = "The accelerator is idle relative to demand while the host stays busy: you are paying for GPU time you are not using, and throughput is capped by CPU-side work (prep, Python, serialization, small launches).";

        Some(Insight::new_detection(
            "cpu_bottleneck",
            confidence,
            "Host CPU is saturated while GPU utilization stays low — classic CPU bottleneck before the accelerator.",
            vec![
                "Batch more work per launch so the GPU gets larger chunks and the CPU launches less often.".to_string(),
                "Move preprocessing, decoding, or tokenization to C++/Rust, GPU, or a dedicated worker pool.".to_string(),
                "Use the CPU flame graph (KW_PROFILE_PID on your training/inference process) to find the widest frames and optimize those first.".to_string(),
                "Enable mixed precision / fused ops only after the CPU can feed the GPU fast enough.".to_string(),
            ],
            data_summary,
            impact_summary,
        ))
    }

    fn detect_io_pressure(&self, s: &WindowStats) -> Option<Insight> {
        if s.avg_blocked < 8.0 || s.avg_cpu < 35.0 {
            return None;
        }
        let confidence = ((s.avg_blocked / 20.0).min(1.0) * 0.55 + (s.avg_cpu / 100.0) * 0.35)
            .clamp(0.2, 0.92);

        let data_summary = format!(
            "Across the last {} samples: many processes are in uninterruptible sleep (blocked ≈ {:.0}) while CPU load is still {:.0}%. That usually means disks, network filesystems, or drivers are stalling threads.",
            s.len, s.avg_blocked, s.avg_cpu
        );
        let impact_summary = "Threads block in D-state when the kernel waits on storage or NFS: pipelines stall in bursts even if average CPU looks moderate, and GPU work may starve for data.";

        Some(Insight::new_detection(
            "io_pressure",
            confidence,
            "Heavy I/O wait is likely stalling your pipeline",
            vec![
                "Check disk/NFS latency and queue depth; move datasets to local NVMe if possible.".to_string(),
                "Prefetch and cache shards; increase dataloader workers only if I/O can keep up.".to_string(),
                "Avoid synchronous reads on the critical path; use async I/O or larger readahead.".to_string(),
            ],
            data_summary,
            impact_summary,
        ))
    }

    fn detect_gpu_underfed(&self, s: &WindowStats) -> Option<Insight> {
        if s.avg_gpu >= 40.0 || s.avg_cpu >= 78.0 {
            return None;
        }
        let confidence = ((40.0 - s.avg_gpu) / 40.0 * 0.5 + (70.0 - s.avg_cpu) / 70.0 * 0.3)
            .clamp(0.15, 0.85);

        let data_summary = format!(
            "Across the last {} samples: GPU utilization averages {:.0}% ({}) while CPU is only {:.0}% — the GPU is often idle even though the host is not maxed out.",
            s.len,
            s.avg_gpu,
            Self::gpu_source_label(s.gpu_source),
            s.avg_cpu
        );
        let impact_summary = "Low GPU duty cycle with headroom on CPU often means small batches, sync points, or launch overhead — you are not extracting the hardware’s potential FLOPs.";

        Some(Insight::new_detection(
            "gpu_underfed",
            confidence,
            "GPU is under-utilized while CPU still has headroom — likely batching or pipeline structure",
            vec![
                "Increase batch size (memory permitting) or gradient accumulation to amortize launch cost.".to_string(),
                "Overlap H2D copy with compute using streams; hide transfer latency.".to_string(),
                "Reduce per-step Python work: compile loops, use torch.compile / JIT, or move control logic off the hot path.".to_string(),
            ],
            data_summary,
            impact_summary,
        ))
    }
}
