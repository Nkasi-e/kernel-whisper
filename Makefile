.PHONY: help api api-host api-mock cli cli-host cli-mock ui-build ui dev clean

help:
	@echo "KernelWhisper commands"
	@echo "  make api        - run API with default host tracer"
	@echo "  make api-host   - run API with host tracer + tuned thresholds"
	@echo "  make api-mock   - run API with deterministic mock tracer"
	@echo "  make cli        - run CLI with default host tracer"
	@echo "  make cli-host   - run CLI with host tracer + tuned thresholds"
	@echo "  make cli-mock   - run CLI with deterministic mock tracer"
	@echo "  make ui-build   - build WASM package into crates/ui-wasm/www/pkg"
	@echo "  make ui         - serve dashboard from crates/ui-wasm/www"
	@echo "  make dev        - run API and dashboard together"
	@echo "  make clean      - remove Rust build artifacts"

api:
	cargo run -p kw-api

api-host:
	KW_TRACER_MODE=host KW_CPU_HOT_THRESHOLD=55 KW_GPU_LOW_THRESHOLD=58 cargo run -p kw-api

api-mock:
	KW_TRACER_MODE=mock cargo run -p kw-api

cli:
	cargo run -p kw-cli

cli-host:
	KW_TRACER_MODE=host KW_CPU_HOT_THRESHOLD=55 KW_GPU_LOW_THRESHOLD=58 cargo run -p kw-cli

cli-mock:
	KW_TRACER_MODE=mock cargo run -p kw-cli

ui-build:
	wasm-pack build crates/ui-wasm --target web --out-dir www/pkg

ui:
	cd crates/ui-wasm/www && python3 -m http.server $${UI_PORT:-8080}

dev: ui-build
	@bash -c 'trap "kill 0" EXIT; cargo run -p kw-api & cd crates/ui-wasm/www && python3 -m http.server $${UI_PORT:-8080}'

clean:
	cargo clean
