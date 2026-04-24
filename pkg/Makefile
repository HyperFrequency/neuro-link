# neuro-link pkg/Makefile — top-level installer + deploy driver
#
# Targets:
#   all               — every platform + cloud target, proof files required
#   macos             — signed + notarized .pkg for arm64
#   linux-x86_64      — .deb + .rpm + AppImage for x86_64
#   linux-aarch64     — .deb + .rpm + AppImage for aarch64
#   docker            — docker compose build + health check
#   cloud-modal       — modal deploy + smoke
#   cloud-lambda      — lambda-labs instance provision + smoke
#   cloud-ray         — ray cluster up + job submit + result read
#   models            — pre-download model triple (Octen + Qwen3 reranker + qmd query-expansion)
#   dashboards        — bring optuna-dashboard + vbtpro-dashboard reachable
#   proof             — aggregate .ready.json files into pkg/.proof/ALL.ready.json
#   clean             — remove dist/ and .proof/

.PHONY: all local mac-only macos linux linux-x86_64 linux-aarch64 docker cloud cloud-modal cloud-lambda cloud-ray models dashboards proof clean

VERSION ?= $(shell git describe --tags --always --dirty)
ROOT := $(shell git rev-parse --show-toplevel)
PROOF_DIR := $(ROOT)/pkg/.proof
DIST_DIR := $(ROOT)/dist

$(PROOF_DIR):
	mkdir -p $(PROOF_DIR)

$(DIST_DIR):
	mkdir -p $(DIST_DIR)

# `all`   — every target (including cloud); skipped_no_creds will mark cloud deferred
# `local` — mac + docker + both linux targets (no cloud). green_excluding_skipped iff all 4 ready
# `mac-only` — just macos + proof (fast iteration)
# `linux` — both linux cross-compile targets + proof
# `cloud` — just the 3 cloud deploys (requires MODAL_TOKEN_ID / LAMBDA_CLOUD_API_KEY / AWS creds)
all: macos linux-x86_64 linux-aarch64 docker cloud-modal cloud-lambda cloud-ray proof

local: macos linux-x86_64 linux-aarch64 docker proof

mac-only: macos proof

linux: linux-x86_64 linux-aarch64 proof

cloud: cloud-modal cloud-lambda cloud-ray proof

macos: $(PROOF_DIR) $(DIST_DIR)
	@bash pkg/macos/build.sh $(VERSION) | tee $(PROOF_DIR)/macos-arm64.log

linux-x86_64: $(PROOF_DIR) $(DIST_DIR)
	@bash pkg/linux-x86_64/build.sh $(VERSION) | tee $(PROOF_DIR)/linux-x86_64.log

linux-aarch64: $(PROOF_DIR) $(DIST_DIR)
	@bash pkg/linux-aarch64/build.sh $(VERSION) | tee $(PROOF_DIR)/linux-aarch64.log

docker: $(PROOF_DIR)
	@bash pkg/docker/build.sh $(VERSION) | tee $(PROOF_DIR)/docker.log

cloud-modal: $(PROOF_DIR)
	@bash pkg/cloud/modal/deploy.sh $(VERSION) | tee $(PROOF_DIR)/modal.log

cloud-lambda: $(PROOF_DIR)
	@bash pkg/cloud/lambda/deploy.sh $(VERSION) | tee $(PROOF_DIR)/lambda.log

cloud-ray: $(PROOF_DIR)
	@bash pkg/cloud/ray/deploy.sh $(VERSION) | tee $(PROOF_DIR)/ray.log

models:
	@bash $(ROOT)/.claude/skills/neuro-link-setup/scripts/download_models.sh

dashboards:
	@bash pkg/dashboards/bring-up.sh

proof:
	@python3 pkg/scripts/aggregate_proof.py $(PROOF_DIR) > $(PROOF_DIR)/ALL.ready.json
	@echo "ALL.ready.json written:"
	@cat $(PROOF_DIR)/ALL.ready.json | python3 -m json.tool | head -40

clean:
	rm -rf $(DIST_DIR) $(PROOF_DIR)
