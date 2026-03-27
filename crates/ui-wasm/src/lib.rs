use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightView {
    pub issue: String,
    pub confidence: f32,
    pub message: String,
    #[serde(default)]
    pub suggestions: Vec<String>,
    #[serde(default)]
    pub ts: Option<String>,
    #[serde(default)]
    pub data_summary: String,
    #[serde(default)]
    pub impact_summary: String,
}

#[wasm_bindgen]
pub fn render_insight_card(json_payload: &str) -> Result<String, JsValue> {
    let insight: InsightView =
        serde_json::from_str(json_payload).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let severity = severity_label(insight.confidence);
    let title = issue_label(&insight.issue);
    let ts = insight.ts.as_deref().unwrap_or("now");
    let data_block = section_block(
        "What the data shows",
        insight.data_summary.trim(),
        "insight-data",
    );
    let impact_block = section_block(
        "Why it matters",
        insight.impact_summary.trim(),
        "insight-impact",
    );
    let suggestions = if insight.suggestions.is_empty() {
        "<li>No suggestions provided</li>".to_string()
    } else {
        insight
            .suggestions
            .iter()
            .map(|s| format!("<li>{}</li>", escape_html(s)))
            .collect::<Vec<_>>()
            .join("")
    };

    let html = format!(
        r#"
<article class="insight-card severity-{severity}">
  <header class="insight-header">
    <div class="issue-chip">{issue}</div>
    <div class="severity-chip">{severity}</div>
  </header>
  <h3 class="insight-title">{title}</h3>
  <p class="insight-message">{message}</p>
  {data_block}
  {impact_block}
  <div class="confidence-row">
    <span>Confidence</span>
    <span>{confidence:.2}</span>
  </div>
  <div class="confidence-bar">
    <div class="confidence-fill" style="width: {confidence_pct:.1}%"></div>
  </div>
  <div class="meta-row">Observed: {ts}</div>
  <h4 class="insight-sub">What to do next</h4>
  <ol class="suggestions-list">{suggestions}</ol>
</article>
        "#,
        severity = severity,
        issue = escape_html(&insight.issue),
        title = escape_html(title),
        message = escape_html(&insight.message),
        data_block = data_block,
        impact_block = impact_block,
        confidence = insight.confidence,
        confidence_pct = (insight.confidence * 100.0).clamp(0.0, 100.0),
        ts = escape_html(ts),
        suggestions = suggestions
    );

    Ok(html.trim().to_string())
}

fn section_block(title: &str, body: &str, class: &str) -> String {
    if body.is_empty() {
        return String::new();
    }
    format!(
        r#"<section class="insight-section {class}"><h4 class="insight-sub">{title}</h4><p class="insight-section-body">{body}</p></section>"#,
        class = class,
        title = escape_html(title),
        body = escape_html(body),
    )
}

/// Summary JSON for the dashboard metrics row.
///
/// - `active_issues`: count of rule firings (`cpu_bottleneck`, `io_pressure`, `gpu_underfed`, …)
///   — excludes `telemetry_snapshot`.
/// - `snapshot_count`: informational `telemetry_snapshot` rows in the same payload (liveness).
/// - `total`: same as `active_issues` (kept for older callers).
#[wasm_bindgen]
pub fn summarize_insights(json_payload: &str) -> Result<String, JsValue> {
    let insights: Vec<InsightView> =
        serde_json::from_str(json_payload).map_err(|e| JsValue::from_str(&e.to_string()))?;

    const SNAPSHOT_ISSUE: &str = "telemetry_snapshot";
    let snapshot_count = insights
        .iter()
        .filter(|i| i.issue.as_str() == SNAPSHOT_ISSUE)
        .count();

    let hard_insights: Vec<&InsightView> = insights
        .iter()
        .filter(|i| i.issue.as_str() != SNAPSHOT_ISSUE)
        .collect();
    let active_issues = hard_insights.len();

    if active_issues == 0 {
        let summary = serde_json::json!({
            "active_issues": 0,
            "snapshot_count": snapshot_count,
            "total": 0,
            "avg_confidence": 0.0,
            "high_risk": 0,
            "top_issue": "none",
            "health": "stable"
        });
        return Ok(summary.to_string());
    }

    let avg_confidence = hard_insights.iter().map(|i| i.confidence).sum::<f32>() / active_issues as f32;
    let high_risk = hard_insights.iter().filter(|i| i.confidence >= 0.7).count();

    let cpu_bottleneck = hard_insights
        .iter()
        .filter(|i| i.issue.as_str() == "cpu_bottleneck")
        .count();
    let io_pressure = hard_insights
        .iter()
        .filter(|i| i.issue.as_str() == "io_pressure")
        .count();
    let gpu_underfed = hard_insights
        .iter()
        .filter(|i| i.issue.as_str() == "gpu_underfed")
        .count();

    let top_issue = top_issue_from_counts(cpu_bottleneck, io_pressure, gpu_underfed);
    let health = health_from_avg_confidence(avg_confidence);

    let summary = serde_json::json!({
        "active_issues": active_issues,
        "snapshot_count": snapshot_count,
        "total": active_issues,
        "avg_confidence": avg_confidence,
        "high_risk": high_risk,
        "top_issue": top_issue,
        "health": health
    });

    Ok(summary.to_string())
}

fn top_issue_from_counts(
    cpu_bottleneck: usize,
    io_pressure: usize,
    gpu_underfed: usize,
) -> &'static str {
    if cpu_bottleneck > 0 && cpu_bottleneck >= io_pressure && cpu_bottleneck >= gpu_underfed {
        "cpu_bottleneck"
    } else if io_pressure > 0 && io_pressure >= gpu_underfed {
        "io_pressure"
    } else if gpu_underfed > 0 {
        "gpu_underfed"
    } else {
        "mixed"
    }
}

fn health_from_avg_confidence(avg_confidence: f32) -> &'static str {
    if avg_confidence >= 0.75 {
        "degraded"
    } else if avg_confidence >= 0.45 {
        "warning"
    } else {
        "stable"
    }
}

fn severity_label(confidence: f32) -> &'static str {
    if confidence >= 0.75 {
        "critical"
    } else if confidence >= 0.45 {
        "warning"
    } else {
        "low"
    }
}

fn issue_label(issue: &str) -> &str {
    match issue {
        "cpu_bottleneck" => "CPU bottleneck is reducing accelerator throughput",
        "io_pressure" => "I/O wait is stalling work (storage or NFS pressure)",
        "gpu_underfed" => "GPU is idle too often — pipeline not feeding the accelerator",
        "telemetry_snapshot" => "Live telemetry snapshot (no strong bottleneck yet)",
        "scheduling_inefficiency" => "Task scheduling is delaying compute",
        "blocking_delay" => "Blocking operations are stalling pipeline",
        _ => "Resource inefficiency detected",
    }
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_splits_active_and_snapshots() {
        let payload = r#"[
            {"issue":"telemetry_snapshot","confidence":0.2,"message":"m","data_summary":"","impact_summary":""},
            {"issue":"cpu_bottleneck","confidence":0.8,"message":"c","data_summary":"","impact_summary":""}
        ]"#;
        let s: serde_json::Value =
            serde_json::from_str(&summarize_insights(payload).unwrap()).unwrap();
        assert_eq!(s["active_issues"], 1);
        assert_eq!(s["snapshot_count"], 1);
        assert_eq!(s["total"], 1);
    }

    #[test]
    fn summarize_only_snapshots() {
        let payload = r#"[
            {"issue":"telemetry_snapshot","confidence":0.2,"message":"m","data_summary":"","impact_summary":""}
        ]"#;
        let s: serde_json::Value =
            serde_json::from_str(&summarize_insights(payload).unwrap()).unwrap();
        assert_eq!(s["active_issues"], 0);
        assert_eq!(s["snapshot_count"], 1);
        assert_eq!(s["health"], "stable");
    }
}
