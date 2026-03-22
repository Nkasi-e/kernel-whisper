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
  flameCpu: document.getElementById("flame-cpu"),
  flameGpu: document.getElementById("flame-gpu"),
  flameCpuMeta: document.getElementById("flame-cpu-meta"),
  flameGpuMeta: document.getElementById("flame-gpu-meta"),
  playbookIntro: document.getElementById("playbook-intro"),
  playbookPanels: document.getElementById("playbook-panels"),
};

const FLAME_WIDTH = 920;
const FLAME_ROW = 18;

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

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

let playbookLoaded = false;

function renderPlaybook(pb) {
  ui.playbookIntro.textContent = pb.intro || "";
  const panels = pb.panels || [];
  ui.playbookPanels.innerHTML = panels
    .map(
      (p) => `
    <article class="playbook-card">
      <h3>${escapeHtml(p.title)}</h3>
      <dl>
        <dt>How to read it</dt>
        <dd>${escapeHtml(p.how_to_read)}</dd>
        <dt>When it is bad</dt>
        <dd>${escapeHtml(p.when_its_bad)}</dd>
        <dt>What to do</dt>
        <dd><ol>${(p.what_to_do || []).map((x) => `<li>${escapeHtml(x)}</li>`).join("")}</ol></dd>
      </dl>
    </article>
  `,
    )
    .join("");
}

function renderSummary(insights) {
  const summary = JSON.parse(summarize_insights(JSON.stringify(insights)));
  const issueLabels = {
    cpu_bottleneck: "CPU bottleneck",
    io_pressure: "I/O pressure",
    gpu_underfed: "GPU underfed",
    mixed: "Mixed signals",
    none: "none",
  };
  ui.metricTotal.textContent = summary.total;
  ui.metricConfidence.textContent = Number(summary.avg_confidence).toFixed(2);
  ui.metricHighRisk.textContent = summary.high_risk;
  ui.metricTopIssue.textContent = issueLabels[summary.top_issue] || summary.top_issue;

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
    ui.latestInsight.innerHTML =
      '<p class="muted">No issue matched the rules in the current rolling window — often that means the pipeline looks fine <em>for those thresholds</em>. Keep watching the CPU/GPU panels; if something looks off, use the <strong>How to read this</strong> guide on the left.</p>';
    ui.insightList.textContent = "";
    return;
  }

  const latest = insights[insights.length - 1];
  ui.latestInsight.innerHTML = render_insight_card(JSON.stringify(latest));

  const recent = insights.slice(-8).reverse();
  ui.insightList.innerHTML = recent
    .map((insight) => render_insight_card(JSON.stringify(insight)))
    .join("");
}

function escapeXml(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function truncate(s, max) {
  if (s.length <= max) return s;
  return `${s.slice(0, max - 1)}…`;
}

function colorForFrame(name, depth) {
  let h = 2166136261;
  for (let i = 0; i < name.length; i += 1) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  const hue = (h >>> 0) % 360;
  const sat = 42 + (depth % 4) * 6;
  const light = 26 + (depth % 6) * 3;
  return `hsl(${hue} ${sat}% ${light}%)`;
}

function collectFlameRects(node, x, y, w, rowH, out) {
  out.push({ x, y, w, name: node.name, v: node.value });
  const kids = node.children || [];
  if (!kids.length) return;
  const sum = kids.reduce((s, c) => s + c.value, 0);
  if (!sum) return;
  let cx = x;
  const ny = y + rowH;
  for (const c of kids) {
    const cw = (c.value / sum) * w;
    if (cw >= 0.75) collectFlameRects(c, cx, ny, cw, rowH, out);
    cx += cw;
  }
}

function flameGraphHeight(root, rowH) {
  let maxY = 0;
  const walk = (node, d) => {
    maxY = Math.max(maxY, d);
    for (const c of node.children || []) walk(c, d + 1);
  };
  walk(root, 0);
  return (maxY + 1) * rowH;
}

function renderFlameMount(profile, rowH) {
  const root = profile.root;
  const total = profile.total_samples;
  if (!total || !root.value) {
    if (profile.kind === "cpu") {
      return `<p class="muted" style="padding:12px;line-height:1.5">No real CPU stacks yet. Start the API with <code>KW_PROFILE_PID</code> set to your workload’s PID, or <code>KW_PROFILE_SELF=1</code> to sample this server. See <strong>How to read this</strong> in the sidebar.</p>`;
    }
    return `<p class="muted" style="padding:12px;line-height:1.5">Collecting GPU telemetry… If GPU stays flat, confirm whether utilization is from <code>nvidia-smi</code> or a host estimate (see sidebar guide).</p>`;
  }

  const rects = [];
  collectFlameRects(root, 0, 0, FLAME_WIDTH, rowH, rects);
  const height = flameGraphHeight(root, rowH);
  const parts = rects.map((r) => {
    const depth = Math.round(r.y / rowH);
    const fill = colorForFrame(r.name, depth);
    const rw = Math.max(r.w - 0.5, 0);
    const rh = rowH - 1;
    const pct = total ? ((100 * r.v) / total).toFixed(1) : "0";
    const title = `${r.name} — ${r.v} samples (${pct}%)`;
    const label = r.w > 52 ? escapeXml(truncate(r.name, 28)) : "";
    const text =
      label !== ""
        ? `<text x="${r.x + 3}" y="${r.y + 13}" fill="#e6edf3" font-size="11" font-family="system-ui, sans-serif">${label}</text>`
        : "";
    return `<g><rect x="${r.x}" y="${r.y}" width="${rw}" height="${rh}" fill="${fill}" stroke="#30363d" stroke-width="0.5"><title>${escapeXml(
      title,
    )}</title></rect>${text}</g>`;
  });

  const kind = profile.kind === "gpu" ? "GPU" : "CPU";
  return `<svg class="flame-svg" viewBox="0 0 ${FLAME_WIDTH} ${height}" preserveAspectRatio="xMinYMin meet" role="img" aria-label="${kind} flame graph">${parts.join(
    "",
  )}</svg>`;
}

function renderFlameProfiles(cpuProfile, gpuProfile) {
  const fmtTime = (iso) => {
    try {
      return new Date(iso).toLocaleTimeString();
    } catch {
      return iso;
    }
  };
  ui.flameCpuMeta.textContent = `samples: ${cpuProfile.total_samples} · updated ${fmtTime(cpuProfile.updated_at)}`;
  ui.flameGpuMeta.textContent = `samples: ${gpuProfile.total_samples} · updated ${fmtTime(gpuProfile.updated_at)}`;
  ui.flameCpu.innerHTML = renderFlameMount(cpuProfile, FLAME_ROW);
  ui.flameGpu.innerHTML = renderFlameMount(gpuProfile, FLAME_ROW);
}

async function refresh() {
  const baseUrl = ui.apiUrl.value.trim().replace(/\/$/, "");
  setStatus(ui.apiStatus, "fetching", "#58a6ff");
  try {
    const [healthRes, insights, cpuProfile, gpuProfile] = await Promise.all([
      fetch(`${baseUrl}/health`),
      fetchJson(`${baseUrl}/v1/insights`),
      fetchJson(`${baseUrl}/v1/profile/cpu`),
      fetchJson(`${baseUrl}/v1/profile/gpu`),
    ]);

    if (!playbookLoaded) {
      try {
        const pb = await fetchJson(`${baseUrl}/v1/playbook`);
        renderPlaybook(pb);
        playbookLoaded = true;
      } catch {
        ui.playbookIntro.textContent =
          "Could not load the reading guide from the API (is it running the latest version?).";
      }
    }

    if (healthRes.ok) {
      setStatus(ui.apiStatus, "connected", "#2ea043");
    } else {
      setStatus(ui.apiStatus, "degraded", "#d29922");
    }

    renderSummary(insights);
    renderInsights(insights);
    renderFlameProfiles(cpuProfile, gpuProfile);
    ui.lastRefresh.textContent = new Date().toLocaleTimeString();
  } catch (err) {
    setStatus(ui.apiStatus, "offline", "#f85149");
    setStatus(ui.healthStatus, "unknown", "#9ca6b2");
    ui.latestInsight.textContent = `Failed to fetch: ${err.message}`;
    ui.flameCpu.textContent = "Unavailable (API offline).";
    ui.flameGpu.textContent = "Unavailable (API offline).";
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
