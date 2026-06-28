.PHONY: build install test lint status npm-pack clean

build:
	cd server && cargo build --release

install:
	bash scripts/init.sh

test:
	cd server && cargo test

lint:
	cd server && cargo clippy --all-targets -- -D warnings
	shellcheck hooks/*.sh scripts/*.sh

status:
	@echo "=== Rust server ==="
	@cd server && cargo check 2>&1 | tail -1
	@echo "=== nlr binary ==="
	@test -x server/target/release/nlr && echo "OK" || echo "NOT BUILT: run 'make build'"
	@echo "=== Qdrant ==="
	@curl -sf http://localhost:6333/healthz > /dev/null 2>&1 && echo "OK" || echo "NOT RUNNING"
	@echo "=== Neo4j ==="
	@curl -sf http://localhost:7474 > /dev/null 2>&1 && echo "OK" || echo "NOT RUNNING"
	@echo "=== Hooks ==="
	@grep -q "auto-rag-inject" ~/.claude/settings.json 2>/dev/null && echo "OK" || echo "NOT REGISTERED: run 'make install'"
	@echo "=== Skills ==="
	@ls ~/.claude/skills/neuro-link/SKILL.md > /dev/null 2>&1 && echo "OK" || echo "NOT INSTALLED: run 'make install'"

npm-pack: build
	mkdir -p npm/native
	cp server/target/release/nlr npm/native/nlr
	chmod +x npm/native/nlr
	cd npm && npm pack

clean:
	cd server && cargo clean
	rm -rf npm/native/nlr npm/*.tgz
