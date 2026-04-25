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

.PHONY: all local mac-only macos linux linux-x86_64 linux-aarch64 docker cloud cloud-modal cloud-lambda cloud-ray models dashboards proof proof-full clean

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

# Gate-20 MM1: wire verify.py into the package proof flow so the
# install-completeness checks (MCP initialize probe, hooks roster,
# arch detection, etc.) actually feed the shipping gate. Two artifacts
# get written:
#   - ALL.ready.json: aggregate of per-target builder proofs (existing).
#   - INSTALL_COMPLETE.ready.json: verify.py output (new this gate).
# `aggregate_proof.py` consumes ALL.ready as the green/skipped builder
# state. INSTALL_COMPLETE.ready is the install-completeness gate the
# parent Makefile (top-level orchestrator) requires for the full
# `make all` ship contract. NLR_VERIFY_OFFLINE=1 is set here because
# `make proof` runs inside the package builder before any rag stack
# is up; full-stack reachability is left to `proof-full` in the
# parent Makefile.
# Gate-24 PP1: proof vs proof-full split by SCOPE, not skip-mask.
#   - `make proof` (called by `make all`): ONLY aggregates per-target
#     builder proofs (macos / linux-* / docker / cloud-*) into
#     ALL.ready.json. Does NOT invoke verify.py. Does NOT write
#     INSTALL_COMPLETE.ready.json. Clean-clone `make all` produces a
#     builder-bundle attestation, not an install-completeness one.
#   - `make proof-full`: runs verify.py with NO skips (operator must
#     have provisioned qmd/multilspy/pyright + brought rag-up + seeded
#     vaults) — emits state="ready" INSTALL_COMPLETE.ready.json — then
#     re-aggregates. This is the canonical install-completeness ship
#     gate; users run it after `make rag-up` + dev-tool install.
# Prior behavior conflated the two: `make all` emitted an
# INSTALL_COMPLETE proof with intentional skips, and the aggregator
# happily treated partial proofs as green_excluding_skipped. Codex
# gate-22/23 correctly flagged that as false-green. The new contract:
# an INSTALL_COMPLETE.ready.json on disk means verify.py had no skips
# and no fails; anything else (incomplete/fail) is aggregator-red.
proof:
	# Gate-24 PP1: delete any stale INSTALL_COMPLETE from a prior
	# proof-full so the builder-only aggregate doesn't inherit a
	# partial or outdated completeness attestation.
	@rm -f $(PROOF_DIR)/INSTALL_COMPLETE.ready.json
	@echo "Aggregating per-target builder proofs (no install-completeness in this target):"
	@python3 pkg/scripts/aggregate_proof.py $(PROOF_DIR) > $(PROOF_DIR)/ALL.ready.json
	@cat $(PROOF_DIR)/ALL.ready.json | python3 -m json.tool | head -40

proof-full:
	# Gate-24 PP1 + Gate-25 QQ3 + Gate-26 RR2: verify.py writes its
	# own proof file (INSTALL_COMPLETE.{ready,incomplete,fail}.json).
	# The defensive rm here protects the file-presence-equals-ready
	# invariant against verifier startup failures (interpreter not
	# found, import error, write-path issue) — without it, a stale
	# .ready.json from a prior green run could survive and mislead
	# downstream consumers. verify.py's main() also clears stale state
	# files but only after it reaches that code path; this rm runs
	# before the interpreter is even invoked.
	@rm -f $(PROOF_DIR)/INSTALL_COMPLETE.ready.json \
	       $(PROOF_DIR)/INSTALL_COMPLETE.incomplete.json \
	       $(PROOF_DIR)/INSTALL_COMPLETE.fail.json
	@echo "Running full install-completeness verifier (no skips, live probes):"
	@NLR_VERIFY_OFFLINE=0 NLR_VERIFY_SKIP= python3 pkg/scripts/verify.py \
	  || (echo "ERROR: verify.py reported install-completeness failure (see above)"; exit 1)
	@if [ -f $(PROOF_DIR)/INSTALL_COMPLETE.ready.json ]; then \
	    cat $(PROOF_DIR)/INSTALL_COMPLETE.ready.json | python3 -m json.tool | head -30; \
	fi
	@echo
	@echo "Aggregating all .ready.json proofs into ALL.ready.json:"
	@python3 pkg/scripts/aggregate_proof.py $(PROOF_DIR) > $(PROOF_DIR)/ALL.ready.json
	@cat $(PROOF_DIR)/ALL.ready.json | python3 -m json.tool | head -40

clean:
	rm -rf $(DIST_DIR) $(PROOF_DIR)
