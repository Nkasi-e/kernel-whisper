use async_trait::async_trait;
use chrono::Utc;
use kw_types::{EventSource, TraceEvent};
use std::pin::Pin;
use std::process::Command;
use tokio::time::{interval, Duration};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use uuid::Uuid;

pub type TraceStream = Pin<Box<dyn Stream<Item = TraceEvent> + Send>>;

#[async_trait]
pub trait Tracer: Send + Sync {
    async fn start(&self) -> anyhow::Result<TraceStream>;
}

pub struct MockTracer {
    pub sample_ms: u64,
}

#[async_trait]
impl Tracer for MockTracer {
    async fn start(&self) -> anyhow::Result<TraceStream> {
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        let mut ticker = interval(Duration::from_millis(self.sample_ms));

        tokio::spawn(async move {
            let mut seq: u64 = 0;
            loop {
                ticker.tick().await;
                // Oscillating signal to simulate changing pressure.
                let phase = (seq % 20) as f32;
                let cpu = 65.0 + (phase * 2.0).min(30.0);
                let gpu = (55.0 - (phase * 2.5)).max(10.0);

                let event = TraceEvent {
                    id: Uuid::new_v4(),
                    ts: Utc::now(),
                    cpu_usage_pct: cpu,
                    runnable_tasks: 4 + (seq % 8) as u32,
                    blocked_syscalls: 2 + (seq % 5) as u32,
                    gpu_usage_pct: gpu,
                    source: EventSource::Host,
                };

                if tx.send(event).await.is_err() {
                    break;
                }
                seq = seq.wrapping_add(1);
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }
}

pub async fn start_from_env(sample_ms: u64) -> anyhow::Result<TraceStream> {
    let mode = std::env::var("KW_TRACER_MODE").unwrap_or_else(|_| "host".to_string());
    if mode.eq_ignore_ascii_case("ebpf") {
        #[cfg(all(feature = "ebpf", target_os = "linux"))]
        {
            let object_path = std::env::var("KW_EBPF_OBJECT")
                .unwrap_or_else(|_| "crates/tracer/ebpf/kernelwhisper.bpf.o".to_string());
            let tracer = ebpf::EbpfTracer {
                sample_ms,
                object_path,
            };
            return match tracer.start().await {
                Ok(stream) => Ok(stream),
                Err(err) => {
                    tracing::warn!(
                        ?err,
                        "ebpf tracer startup failed, falling back to mock tracer"
                    );
                    MockTracer { sample_ms }.start().await
                }
            };
        }
        #[cfg(not(all(feature = "ebpf", target_os = "linux")))]
        {
            tracing::warn!("ebpf mode requested but unavailable on this target/build, using mock");
        }
    }
    if mode.eq_ignore_ascii_case("host") {
        return match (HostSignalTracer { sample_ms }).start().await {
            Ok(stream) => Ok(stream),
            Err(err) => {
                tracing::warn!(
                    ?err,
                    "host signal tracer startup failed, falling back to mock tracer"
                );
                MockTracer { sample_ms }.start().await
            }
        };
    }

    MockTracer { sample_ms }.start().await
}

pub struct HostSignalTracer {
    pub sample_ms: u64,
}

#[async_trait]
impl Tracer for HostSignalTracer {
    async fn start(&self) -> anyhow::Result<TraceStream> {
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        let mut ticker = interval(Duration::from_millis(self.sample_ms));

        tokio::spawn(async move {
            loop {
                ticker.tick().await;
                let sample = match sample_host_signals() {
                    Ok(v) => v,
                    Err(err) => {
                        tracing::warn!(?err, "failed sampling host signals");
                        continue;
                    }
                };

                let cpu_usage_pct = sample.cpu_usage_pct;
                // Keep GPU simulated, but tie it to real CPU pressure.
                let gpu_usage_pct = (85.0 - cpu_usage_pct * 0.7).clamp(8.0, 90.0);

                let event = TraceEvent {
                    id: Uuid::new_v4(),
                    ts: Utc::now(),
                    cpu_usage_pct,
                    runnable_tasks: sample.runnable_tasks,
                    blocked_syscalls: sample.blocked_tasks,
                    gpu_usage_pct,
                    source: EventSource::Host,
                };

                if tx.send(event).await.is_err() {
                    break;
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }
}

struct HostSample {
    cpu_usage_pct: f32,
    runnable_tasks: u32,
    blocked_tasks: u32,
}

fn sample_host_signals() -> anyhow::Result<HostSample> {
    // Gather process states and cpu load from `ps` so this works cross-platform
    // (including macOS where /proc and eBPF runtime are not available).
    let ps_output = Command::new("ps")
        .args(["-A", "-o", "state=", "-o", "%cpu="])
        .output()?;
    if !ps_output.status.success() {
        return Err(anyhow::anyhow!("ps command failed"));
    }

    let text = String::from_utf8(ps_output.stdout)?;
    let mut runnable: u32 = 0;
    let mut blocked: u32 = 0;
    let mut cpu_total: f32 = 0.0;

    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let state = match parts.next() {
            Some(v) => v,
            None => continue,
        };
        let cpu = parts
            .next()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0);

        if state.starts_with('R') {
            runnable = runnable.saturating_add(1);
        }
        if state.starts_with('D') || state.starts_with('U') {
            blocked = blocked.saturating_add(1);
        }
        cpu_total += cpu;
    }

    let cores = std::thread::available_parallelism()
        .map(|n| n.get() as f32)
        .unwrap_or(1.0);
    let cpu_usage_pct = (cpu_total / cores).clamp(0.0, 100.0);

    Ok(HostSample {
        cpu_usage_pct,
        runnable_tasks: runnable,
        blocked_tasks: blocked,
    })
}

#[cfg(all(feature = "ebpf", target_os = "linux"))]
pub mod ebpf {
    use super::{TraceStream, Tracer};
    use anyhow::Context;
    use async_trait::async_trait;
    use aya::maps::Array;
    use aya::programs::TracePoint;
    use aya::Ebpf;
    use chrono::Utc;
    use kw_types::{EventSource, TraceEvent};
    use tokio::time::{interval, Duration};
    use tokio_stream::wrappers::ReceiverStream;
    use uuid::Uuid;

    pub struct EbpfTracer {
        pub sample_ms: u64,
        pub object_path: String,
    }

    #[async_trait]
    impl Tracer for EbpfTracer {
        async fn start(&self) -> anyhow::Result<TraceStream> {
            let mut bpf = Ebpf::load_file(&self.object_path)
                .with_context(|| format!("loading ebpf object from {}", self.object_path))?;

            attach_tracepoint(&mut bpf, "handle_sched_switch", "sched", "sched_switch")?;
            attach_tracepoint(&mut bpf, "handle_sys_enter", "raw_syscalls", "sys_enter")?;

            let (tx, rx) = tokio::sync::mpsc::channel(256);
            let mut ticker = interval(Duration::from_millis(self.sample_ms));
            let mut cpu_prev = read_cpu_totals().context("reading initial /proc/stat")?;
            let mut prev_blocked: u64 = 0;
            let mut prev_runnable: u64 = 0;

            tokio::spawn(async move {
                let _bpf_guard = bpf;
                let mut bpf = _bpf_guard;
                loop {
                    ticker.tick().await;

                    let Ok(cpu_now) = read_cpu_totals() else {
                        continue;
                    };
                    let cpu_usage = cpu_percent(cpu_prev, cpu_now);
                    cpu_prev = cpu_now;

                    let blocked_total = read_counter(&mut bpf, "BLOCKED_SYSCALLS").unwrap_or(0);
                    let runnable_total = read_counter(&mut bpf, "RUNNABLE_TASKS").unwrap_or(0);

                    let blocked_delta = blocked_total.saturating_sub(prev_blocked);
                    let runnable_delta = runnable_total.saturating_sub(prev_runnable);
                    prev_blocked = blocked_total;
                    prev_runnable = runnable_total;

                    let gpu_usage_pct = (95.0 - cpu_usage).clamp(5.0, 90.0);
                    let event = TraceEvent {
                        id: Uuid::new_v4(),
                        ts: Utc::now(),
                        cpu_usage_pct: cpu_usage,
                        runnable_tasks: runnable_delta.min(u32::MAX as u64) as u32,
                        blocked_syscalls: blocked_delta.min(u32::MAX as u64) as u32,
                        gpu_usage_pct,
                        source: EventSource::Ebpf,
                    };

                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
            });

            Ok(Box::pin(ReceiverStream::new(rx)))
        }
    }

    fn attach_tracepoint(
        bpf: &mut Ebpf,
        prog_name: &str,
        category: &str,
        name: &str,
    ) -> anyhow::Result<()> {
        let program = bpf
            .program_mut(prog_name)
            .with_context(|| format!("missing ebpf program: {prog_name}"))?;
        let tp: &mut TracePoint = program
            .try_into()
            .with_context(|| format!("program {prog_name} is not a tracepoint"))?;
        tp.load()?;
        tp.attach(category, name)?;
        Ok(())
    }

    fn read_counter(bpf: &mut Ebpf, map_name: &str) -> anyhow::Result<u64> {
        let map = bpf
            .map_mut(map_name)
            .with_context(|| format!("missing map: {map_name}"))?;
        let counters = Array::<_, u64>::try_from(map)?;
        Ok(counters.get(0, 0)?)
    }

    fn read_cpu_totals() -> anyhow::Result<(u64, u64)> {
        let stat = std::fs::read_to_string("/proc/stat")?;
        let first = stat
            .lines()
            .next()
            .ok_or_else(|| anyhow::anyhow!("empty /proc/stat"))?;
        let mut fields = first.split_whitespace();
        let _cpu = fields.next();
        let nums: Vec<u64> = fields.take(8).filter_map(|v| v.parse().ok()).collect();
        if nums.len() < 4 {
            return Err(anyhow::anyhow!("unexpected /proc/stat format"));
        }
        let idle = nums[3] + nums.get(4).copied().unwrap_or(0);
        let total: u64 = nums.iter().sum();
        Ok((idle, total))
    }

    fn cpu_percent(prev: (u64, u64), now: (u64, u64)) -> f32 {
        let idle_delta = now.0.saturating_sub(prev.0) as f32;
        let total_delta = now.1.saturating_sub(prev.1) as f32;
        if total_delta <= f32::EPSILON {
            return 0.0;
        }
        ((1.0 - (idle_delta / total_delta)) * 100.0).clamp(0.0, 100.0)
    }
}
