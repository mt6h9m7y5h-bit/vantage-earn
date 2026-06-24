.PHONY: dev test e2e tunnel fmt clippy

dev:
	cargo run -p api-gateway

test:
	cargo test --workspace

e2e:
	./scripts/test-e2e.sh

tunnel:
	./scripts/tunnel-free.sh

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings
