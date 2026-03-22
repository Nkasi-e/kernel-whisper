use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PlaybookPanel {
    pub id: &'static str,
    pub title: &'static str,
    pub how_to_read: &'static str,
    pub when_its_bad: &'static str,
    pub what_to_do: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Playbook {
    pub intro: &'static str,
    pub panels: Vec<PlaybookPanel>,
}

impl Playbook {
    pub fn bundled() -> Self {
        Self {
            intro: "KernelWhisper combines host telemetry, optional real CPU stacks, and GPU utilization hints. Use this page to interpret what you see and what to change next.",
            panels: vec![
                PlaybookPanel {
                    id: "cpu_flame",
                    title: "CPU flame graph",
                    how_to_read: "Width shows how often each function appeared on sampled stacks (inclusive). Taller stacks are deeper call chains. Hover a block for sample count and share of total.",
                    when_its_bad: "One or a few frames dominate width for a long time: you are spending most CPU time there. Empty graph means native sampling is off—set KW_PROFILE_PID or KW_PROFILE_SELF.",
                    what_to_do: vec![
                        "Profile the process that does your real work (set KW_PROFILE_PID to that PID).",
                        "Optimize or parallelize the widest frames; consider moving work off the hot path.",
                        "If the graph only shows the API server, you are not yet profiling your training/inference job.",
                    ],
                },
                PlaybookPanel {
                    id: "gpu_tree",
                    title: "GPU panel",
                    how_to_read: "This is not a hardware kernel flame graph. It encodes GPU utilization from nvidia-smi when available, or a host-side estimate from CPU load otherwise.",
                    when_its_bad: "nvidia_smi with low % while CPU is busy: host may be bottlenecking launches or data prep. host_estimate: treat GPU % as directional until you have a real GPU.",
                    what_to_do: vec![
                        "With NVIDIA: verify nvidia-smi is on PATH so utilization is real.",
                        "Increase batch size / overlap copies and compute / reduce Python overhead in the launch path.",
                        "Profile with vendor tools (Nsight, rocprof) if you need kernel-level GPU detail.",
                    ],
                },
                PlaybookPanel {
                    id: "insights",
                    title: "Insights cards",
                    how_to_read: "Each card ties numbers to a pattern. “What the data shows” restates the signals; “Why it matters” is the performance risk; numbered steps are concrete fixes.",
                    when_its_bad: "Higher confidence means the pattern held across recent samples. Critical = act soon; warning = investigate.",
                    what_to_do: vec![
                        "Start with the latest card, then confirm in the CPU flame and GPU panel.",
                        "Re-run after changes; confidence should drop if the bottleneck moved.",
                    ],
                },
            ],
        }
    }
}
