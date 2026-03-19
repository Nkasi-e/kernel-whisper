use chrono::Utc;
use kw_types::{Insight, TraceEvent};
use std::collections::VecDeque;

pub struct DetectionEngine {
    window: VecDeque<TraceEvent>,
    window_size: usize,
}

impl DetectionEngine {
    pub fn new(window_size: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(window_size),
            window_size,
        }
    }

    pub fn ingest(&mut self, event: TraceEvent) -> Option<Insight> {
        self.window.push_back(event);
        while self.window.len() > self.window_size {
            self.window.pop_front();
        }
        self.detect_cpu_bottleneck()
    }

    fn detect_cpu_bottleneck(&self) -> Option<Insight> {
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

        if avg_cpu >= 80.0 && avg_gpu <= 45.0 {
            // Confidence blend tuned for clear signal quality.
            let cpu_component = ((avg_cpu - 70.0) / 30.0).clamp(0.0, 1.0);
            let gpu_component = ((50.0 - avg_gpu) / 50.0).clamp(0.0, 1.0);
            let blocked_component = (avg_blocked / 10.0).clamp(0.0, 1.0);
            let confidence =
                (0.5 * cpu_component + 0.35 * gpu_component + 0.15 * blocked_component)
                    .clamp(0.1, 0.99);

            return Some(Insight {
                issue: "cpu_bottleneck".to_string(),
                confidence,
                message: "High CPU usage is causing GPU underutilization".to_string(),
                suggestions: vec![
                    "Batch operations".to_string(),
                    "Move preprocessing off CPU".to_string(),
                    "Parallelize CPU preprocessing stage".to_string(),
                ],
                ts: Utc::now(),
            });
        }

        None
    }
}
