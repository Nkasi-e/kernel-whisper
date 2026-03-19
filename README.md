# KernelWhisper

KernelWhisper is an observability product for developers and performance engineers who need to understand why accelerators are underutilized. It correlates CPU-side pressure with simulated GPU utilization to surface actionable inefficiency insights.

## Product Value

- Detects CPU bottlenecks that reduce GPU throughput
- Surfaces structured insights with confidence and concrete suggestions
- Offers three operator surfaces: API, CLI, and live dashboard
- Supports real host telemetry now, with Linux eBPF path available

## What You Can Run Right Now

- **API service** returning health and insight feeds
- **CLI stream** printing JSON insights in real time
- **Web dashboard** showing status, metrics, and recent detections

## Quick Start

Prerequisites:

- Rust toolchain
- `wasm-pack` for dashboard build (`cargo install wasm-pack`)
- Python 3 for static file serving

From project root:

```bash
make ui-build
make dev
```

Then open:

```text
http://localhost:8080
```

## Makefile Commands

Use `make help` to list all commands.

- `make api` - run API with default host tracer
- `make api-host` - run API with tuned host thresholds
- `make api-mock` - run API with deterministic mock tracer
- `make cli` - run CLI with default host tracer
- `make cli-host` - run CLI with tuned host thresholds
- `make cli-mock` - run CLI with deterministic mock tracer
- `make ui-build` - build WASM assets into `crates/ui-wasm/www/pkg`
- `make ui` - serve dashboard on `http://localhost:8080`
- `make dev` - run API and dashboard together

## API Endpoints

- `GET /health`
- `GET /v1/insights`

Example:

```bash
curl http://localhost:3000/health
curl http://localhost:3000/v1/insights
```

## Insight Shape

```json
{
  "issue": "cpu_bottleneck",
  "confidence": 0.87,
  "message": "High CPU usage is causing GPU underutilization",
  "suggestions": [
    "Batch operations",
    "Move preprocessing off CPU"
  ]
}
```

## Tracing Modes

- `host` (default): live host process/scheduler signals via `ps`
- `mock`: deterministic synthetic signal for reproducible demos/tests
- `ebpf`: Linux-only `aya` tracepoint path

Linux eBPF probe build:

```bash
./crates/tracer/ebpf/build-ebpf.sh
```

Run with eBPF:

```bash
KW_TRACER_MODE=ebpf cargo run -p kw-api
```

## Detection Tuning

Environment variables:

- `KW_MIN_SAMPLES` (default: `5`)
- `KW_CPU_HOT_THRESHOLD` (default: `60`)
- `KW_GPU_LOW_THRESHOLD` (default: `52`)
- `KW_BLOCKED_NORM_DIVISOR` (default: `8`)
- `KW_SUPPRESS_DUPLICATES` (default: `true`)

Example tuned host mode:

```bash
KW_TRACER_MODE=host KW_CPU_HOT_THRESHOLD=55 KW_GPU_LOW_THRESHOLD=58 cargo run -p kw-api
```

## Architecture

- `crates/tracer`: event production (host, mock, eBPF)
- `crates/engine`: rule evaluation and confidence scoring
- `crates/storage`: insight persistence abstraction (in-memory + ClickHouse seam)
- `crates/api`: Axum service and ingestion loop
- `crates/cli`: terminal insight stream
- `crates/ui-wasm`: WASM rendering and dashboard assets

## Container Workflow

Run API in Docker:

```bash
docker compose up --build api
```

Optional ClickHouse profile:

```bash
docker compose --profile clickhouse up --build
```
