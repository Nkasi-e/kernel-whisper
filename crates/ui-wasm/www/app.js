import init, { render_insight_card, summarize_insights } from "./pkg/kw_ui_wasm.js";

const ui = {
  apiStatus: document.getElementById("api-status"),
  healthStatus: document.getElementById("health-status"),
  lastRefresh: document.getElementById("last-refresh"),
  apiUrl: document.getElementById("api-url"),
  refreshBtn: document.getElementById("refresh-btn"),
  latestInsight: document.getElementById("latest-insight"),
  insightList: document.getElementById("insight-list"),
  metricTotal: document.getElementById("metric-total"),
  metricConfidence: document.getElementById("metric-confidence"),
  metricHighRisk: document.getElementById("metric-high-risk"),
  metricTopIssue: document.getElementById("metric-top-issue"),
};

function setStatus(el, label, tone) {
  el.textContent = label;
  el.style.borderColor = tone;
  el.style.color = tone;
}

async function fetchJson(url) {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  return res.json();
}

function renderSummary(insights) {
  const summary = JSON.parse(summarize_insights(JSON.stringify(insights)));
  ui.metricTotal.textContent = summary.total;
  ui.metricConfidence.textContent = Number(summary.avg_confidence).toFixed(2);
  ui.metricHighRisk.textContent = summary.high_risk;
  ui.metricTopIssue.textContent = summary.top_issue;

  if (summary.health === "degraded") {
    setStatus(ui.healthStatus, "degraded", "#f85149");
  } else if (summary.health === "warning") {
    setStatus(ui.healthStatus, "warning", "#d29922");
  } else if (summary.health === "stable") {
    setStatus(ui.healthStatus, "stable", "#2ea043");
  } else {
    setStatus(ui.healthStatus, "no data", "#9ca6b2");
  }
}

function renderInsights(insights) {
  if (insights.length === 0) {
    ui.latestInsight.textContent = "No insights yet.";
    ui.insightList.textContent = "No insights yet.";
    return;
  }

  const latest = insights[insights.length - 1];
  ui.latestInsight.innerHTML = render_insight_card(JSON.stringify(latest));

  const recent = insights.slice(-8).reverse();
  ui.insightList.innerHTML = recent
    .map((insight) => render_insight_card(JSON.stringify(insight)))
    .join("");
}

async function refresh() {
  const baseUrl = ui.apiUrl.value.trim().replace(/\/$/, "");
  setStatus(ui.apiStatus, "fetching", "#58a6ff");
  try {
    const [healthRes, insights] = await Promise.all([
      fetch(`${baseUrl}/health`),
      fetchJson(`${baseUrl}/v1/insights`),
    ]);

    if (healthRes.ok) {
      setStatus(ui.apiStatus, "connected", "#2ea043");
    } else {
      setStatus(ui.apiStatus, "degraded", "#d29922");
    }

    renderSummary(insights);
    renderInsights(insights);
    ui.lastRefresh.textContent = new Date().toLocaleTimeString();
  } catch (err) {
    setStatus(ui.apiStatus, "offline", "#f85149");
    setStatus(ui.healthStatus, "unknown", "#9ca6b2");
    ui.latestInsight.textContent = `Failed to fetch: ${err.message}`;
  }
}

async function main() {
  await init();
  ui.refreshBtn.addEventListener("click", refresh);
  await refresh();
  setInterval(refresh, 3000);
}

main().catch((err) => {
  ui.latestInsight.textContent = `Failed to initialize WASM UI: ${err.message}`;
});
