//! OS-native CPU stack capture (real samples, not synthesized).

use std::process::{Command, Stdio};

fn sample_duration_secs() -> f64 {
    std::env::var("KW_PROFILE_SAMPLE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|v: &f64| *v > 0.01 && *v < 60.0)
        .unwrap_or(0.2)
}

/// Capture one batch of CPU call stacks for `pid` using the best available backend.
pub fn capture_cpu_stacks(pid: u32) -> anyhow::Result<Vec<Vec<String>>> {
    #[cfg(target_os = "macos")]
    {
        return macos::capture(pid);
    }
    #[cfg(target_os = "linux")]
    {
        return linux::capture(pid);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        anyhow::bail!("native CPU sampling is only implemented for macOS and Linux");
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    pub fn capture(pid: u32) -> anyhow::Result<Vec<Vec<String>>> {
        let dur = sample_duration_secs();
        let dur_s = format!("{dur:.3}");
        let out = Command::new("sample")
            .args([pid.to_string(), dur_s])
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .output()?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("sample failed: {err}");
        }
        let text = String::from_utf8(out.stdout)?;
        parse_sample_text(&text)
    }

    fn parse_sample_frame(trimmed: &str) -> Option<String> {
        let first = trimmed.split_whitespace().next()?;
        if first.parse::<u64>().is_err() {
            return None;
        }
        let after_count = trimmed[first.len()..].trim_start();
        let idx = after_count.find(" (in ")?;
        let sym = after_count[..idx].trim();
        if sym.is_empty() {
            return None;
        }
        let rest = &after_count[idx + 5..];
        let module = rest.split(')').next().unwrap_or("").trim();
        Some(format!("{sym} ({module})"))
    }

    fn parse_sample_text(text: &str) -> anyhow::Result<Vec<Vec<String>>> {
        let mut stacks: Vec<Vec<String>> = Vec::new();
        let mut in_cg = false;
        let mut path: Vec<String> = Vec::new();

        for line in text.lines() {
            if line.contains("Call graph:") {
                in_cg = true;
                continue;
            }
            if !in_cg {
                continue;
            }
            if line.starts_with("Total number") || line.starts_with("Sort by") {
                break;
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            let ls = line.len() - line.trim_start().len();
            let t = line.trim_start();
            if t.contains("Thread_") {
                if !path.is_empty() {
                    stacks.push(std::mem::take(&mut path));
                }
                continue;
            }
            let Some(frame) = parse_sample_frame(t) else {
                continue;
            };
            let depth = ls.saturating_sub(4) / 2;
            if depth == 0 {
                continue;
            }
            while path.len() >= depth {
                path.pop();
            }
            path.push(frame);
        }
        if !path.is_empty() {
            stacks.push(path);
        }
        if stacks.is_empty() {
            anyhow::bail!("sample produced no parseable stacks (permission or short sample?)");
        }
        Ok(stacks)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn capture(pid: u32) -> anyhow::Result<Vec<Vec<String>>> {
        let dur = sample_duration_secs();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let data_path = std::env::temp_dir().join(format!("kw_perf_{pid}_{nanos}.data"));

        let record = Command::new("perf")
            .args([
                "record",
                "-g",
                "-p",
                &pid.to_string(),
                "-o",
                data_path.to_str().ok_or_else(|| anyhow::anyhow!("bad temp path"))?,
                "--",
            ])
            .arg("sleep")
            .arg(format!("{dur:.3}"))
            .stderr(Stdio::piped())
            .stdout(Stdio::null())
            .status()?;

        if !record.success() {
            let _ = std::fs::remove_file(&data_path);
            anyhow::bail!(
                "perf record failed (install perf, allow ptrace, or run as root for other users' PIDs)"
            );
        }

        let script = Command::new("perf")
            .args([
                "script",
                "-i",
                data_path.to_str().unwrap(),
                "--no-inline",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;

        let _ = std::fs::remove_file(&data_path);

        if !script.status.success() {
            let err = String::from_utf8_lossy(&script.stderr);
            anyhow::bail!("perf script failed: {err}");
        }
        let text = String::from_utf8(script.stdout)?;
        let stacks = parse_perf_script(&text);
        if stacks.is_empty() {
            anyhow::bail!("perf script produced no stacks");
        }
        Ok(stacks)
    }

    fn parse_perf_line(line: &str) -> Option<String> {
        let t = line.trim_start_matches('\t').trim_start();
        let addr_end = t.find(|c: char| c.is_whitespace())?;
        let sym_part = t[addr_end..].trim();
        if sym_part.is_empty() {
            return None;
        }
        let name = sym_part.split(" (").next()?.trim();
        if name.is_empty() {
            return None;
        }
        if name.len() <= 2 && name.chars().all(|c| c == '0' || c == 'x') {
            return Some("[unknown]".to_string());
        }
        Some(name.to_string())
    }

    fn parse_perf_script(out: &str) -> Vec<Vec<String>> {
        let mut stacks: Vec<Vec<String>> = Vec::new();
        let mut stack: Vec<String> = Vec::new();
        for line in out.lines() {
            if line.starts_with('\t') {
                if let Some(f) = parse_perf_line(line) {
                    stack.push(f);
                }
            } else if !line.trim().is_empty() {
                if !stack.is_empty() {
                    stack.reverse();
                    stacks.push(std::mem::take(&mut stack));
                }
            }
        }
        if !stack.is_empty() {
            stack.reverse();
            stacks.push(stack);
        }
        stacks
    }
}
