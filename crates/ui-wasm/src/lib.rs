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
}

#[wasm_bindgen]
pub fn render_insight_card(json_payload: &str) -> Result<String, JsValue> {
    let insight: InsightView =
        serde_json::from_str(json_payload).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let severity = severity_label(insight.confidence);
    let title = issue_label(&insight.issue);
    let ts = insight.ts.as_deref().unwrap_or("now");
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
  <div class="confidence-row">
    <span>Confidence</span>
    <span>{confidence:.2}</span>
  </div>
  <div class="confidence-bar">
    <div class="confidence-fill" style="width: {confidence_pct:.1}%"></div>
  </div>
  <div class="meta-row">Observed: {ts}</div>
  <ul class="suggestions-list">{suggestions}</ul>
</article>
        "#,
        severity = severity,
        issue = escape_html(&insight.issue),
        title = escape_html(title),
        message = escape_html(&insight.message),
        confidence = insight.confidence,
        confidence_pct = (insight.confidence * 100.0).clamp(0.0, 100.0),
        ts = escape_html(ts),
        suggestions = suggestions
    );

    Ok(html.trim().to_string())
}

#[wasm_bindgen]
pub fn summarize_insights(json_payload: &str) -> Result<String, JsValue> {
    let insights: Vec<InsightView> =
        serde_json::from_str(json_payload).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let total = insights.len();
    if total == 0 {
        return Ok(
            r#"{"total":0,"avg_confidence":0.0,"high_risk":0,"top_issue":"none","health":"no_data"}"#
                .to_string(),
        );
    }

    let avg_confidence = insights.iter().map(|i| i.confidence).sum::<f32>() / total as f32;
    let high_risk = insights.iter().filter(|i| i.confidence >= 0.7).count();

    let mut cpu_bottleneck = 0usize;
    let mut scheduling = 0usize;
    let mut blocking = 0usize;
    for i in &insights {
        if i.issue.contains("cpu_bottleneck") {
            cpu_bottleneck += 1;
        } else if i.issue.contains("scheduling") {
            scheduling += 1;
        } else if i.issue.contains("blocking") {
            blocking += 1;
        }
    }

    let top_issue = if cpu_bottleneck >= scheduling && cpu_bottleneck >= blocking {
        "cpu_bottleneck"
    } else if scheduling >= blocking {
        "scheduling"
    } else {
        "blocking"
    };

    let health = if avg_confidence >= 0.75 {
        "degraded"
    } else if avg_confidence >= 0.45 {
        "warning"
    } else {
        "stable"
    };

    let summary = serde_json::json!({
        "total": total,
        "avg_confidence": avg_confidence,
        "high_risk": high_risk,
        "top_issue": top_issue,
        "health": health
    });

    Ok(summary.to_string())
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
