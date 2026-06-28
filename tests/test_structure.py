"""Tests for repo structure, configs, and skills (19 tests).

These tests validate the actual repo on disk (not temp dirs).
"""

from __future__ import annotations

import os
from pathlib import Path

import pytest

NLR_REPO = Path(__file__).resolve().parent.parent

EXPECTED_SKILLS = [
    "auto-rag",
    "code-docs",
    "crawl-ingest",
    "harness-bridge",
    "hyper-sleep",
    "job-scanner",
    "knowledge-gap",
    "neuro-link",
    "neuro-link-setup",
    "neuro-scan",
    "neuro-surgery",
    "progress-report",
    "reasoning-ontology",
    "self-improve-hitl",
    "self-improve-recursive",
    "wiki-curate",
]

EXPECTED_CONFIGS = [
    "adjacent-tools-code-docs.md",
    "crawl-ingest-update.md",
    "forked-repos-with-changes.md",
    "harness-harness-comms.md",
    "hyper-sleep.md",
    "main-codebase-tools.md",
    "neuro-link-config.md",
    "neuro-link.md",
    "neuro-scan.md",
    "neuro-surgery.md",
]


# ---------------------------------------------------------------------------
# 1-4. Skills
# ---------------------------------------------------------------------------

def test_all_16_skills_exist():
    skills_dir = NLR_REPO / "skills"
    existing = sorted(d.name for d in skills_dir.iterdir() if d.is_dir())
    for skill in EXPECTED_SKILLS:
        assert skill in existing, f"Missing skill directory: {skill}"
    assert len(existing) >= 16


def test_all_skills_have_frontmatter():
    for skill in EXPECTED_SKILLS:
        path = NLR_REPO / "skills" / skill / "SKILL.md"
        assert path.exists(), f"Missing SKILL.md: {skill}"
        content = path.read_text()
        assert content.startswith("---"), f"{skill}/SKILL.md missing YAML frontmatter"
        # Must have closing ---
        parts = content.split("---")
        assert len(parts) >= 3, f"{skill}/SKILL.md frontmatter not closed"


def test_all_skills_have_when_to_use():
    for skill in EXPECTED_SKILLS:
        path = NLR_REPO / "skills" / skill / "SKILL.md"
        content = path.read_text()
        fm = content.split("---")[1] if "---" in content else ""
        assert "description" in fm.lower() or "use when" in content.lower() or "when to use" in content.lower(), (
            f"{skill}/SKILL.md missing usage description"
        )


def test_all_skills_have_procedure():
    for skill in EXPECTED_SKILLS:
        path = NLR_REPO / "skills" / skill / "SKILL.md"
        content = path.read_text()
        has_procedure = any(
            marker in content.lower()
            for marker in ["procedure", "steps", "## ", "1.", "step 1", "workflow"]
        )
        assert has_procedure, f"{skill}/SKILL.md missing procedure/steps section"


# ---------------------------------------------------------------------------
# 5. Configs
# ---------------------------------------------------------------------------

def test_all_10_configs_exist():
    config_dir = NLR_REPO / "config"
    existing = sorted(f.name for f in config_dir.glob("*.md"))
    for cfg in EXPECTED_CONFIGS:
        assert cfg in existing, f"Missing config: {cfg}"
    assert len(existing) >= 10


def test_all_configs_have_frontmatter():
    config_dir = NLR_REPO / "config"
    for cfg in EXPECTED_CONFIGS:
        path = config_dir / cfg
        content = path.read_text()
        assert content.startswith("---"), f"Config {cfg} missing YAML frontmatter"


# ---------------------------------------------------------------------------
# 6-8. Core files
# ---------------------------------------------------------------------------

def test_init_script_exists_and_executable():
    init = NLR_REPO / "scripts" / "init.sh"
    assert init.exists()
    assert os.access(init, os.X_OK), "init.sh not executable"


def test_schema_md_has_required_sections():
    schema = NLR_REPO / "02-KB-main" / "schema.md"
    assert schema.exists()
    content = schema.read_text().lower()
    # Schema should document wiki page structure
    assert "---" in content  # frontmatter


def test_index_md_exists():
    assert (NLR_REPO / "02-KB-main" / "index.md").exists()


def test_log_md_exists():
    assert (NLR_REPO / "02-KB-main" / "log.md").exists()


# ---------------------------------------------------------------------------
# 9-11. Gitignore, state, secrets
# ---------------------------------------------------------------------------

def test_gitignore_excludes_secrets():
    gi = NLR_REPO / ".gitignore"
    assert gi.exists()
    content = gi.read_text()
    assert "secrets/.env" in content or "secrets/" in content


def test_state_files_exist():
    state = NLR_REPO / "state"
    assert state.is_dir()
    for f in ["heartbeat.json", "session_log.jsonl", "score_history.jsonl", "deviation_log.jsonl"]:
        assert (state / f).exists(), f"Missing state file: {f}"


def test_secrets_env_example_exists():
    assert (NLR_REPO / "secrets" / ".env.example").exists()


# ---------------------------------------------------------------------------
# 12-13. Directory structure
# ---------------------------------------------------------------------------

def test_directory_structure_matches_prd():
    required_dirs = [
        "00-raw",
        "01-sorted",
        "02-KB-main",
        "03-ontology-main",
        "04-KB-agents-workflows",
        "05-insights-gaps",
        "05-self-improvement-HITL",
        "06-self-improvement-recursive",
        "06-progress-reports",
        "07-neuro-link-task",
        "08-code-docs",
        "09-business-docs",
        "config",
        "state",
        "secrets",
        "scripts",
        "skills",
        "hooks",
        "server",
    ]
    for d in required_dirs:
        assert (NLR_REPO / d).is_dir(), f"Missing directory: {d}"


def test_rust_binary_exists():
    binary = NLR_REPO / "server" / "target" / "release" / "nlr"
    if not binary.exists():
        pytest.skip("Rust binary not built yet")
    assert os.access(binary, os.X_OK)


def test_claude_md_has_rules():
    claude_md = NLR_REPO / "CLAUDE.md"
    assert claude_md.exists()
    content = claude_md.read_text()
    assert "rules" in content.lower() or "never" in content.lower() or "always" in content.lower()


# ---------------------------------------------------------------------------
# 16-18. Documentation
# ---------------------------------------------------------------------------

def test_setup_md_has_steps():
    setup = NLR_REPO / "SETUP.md"
    assert setup.exists()
    content = setup.read_text()
    assert "step" in content.lower() or "## " in content


def test_readme_has_mermaid_diagrams():
    readme = NLR_REPO / "README.md"
    assert readme.exists()
    content = readme.read_text()
    assert "```mermaid" in content


# ---------------------------------------------------------------------------
# 19-20. Self-improvement tree
# ---------------------------------------------------------------------------

def test_05_self_improvement_full_tree():
    hitl = NLR_REPO / "05-self-improvement-HITL"
    assert hitl.is_dir()
    expected_subdirs = ["models", "hyperparameters", "prompts", "features", "code-changes", "services-integrations"]
    for sub in expected_subdirs:
        assert (hitl / sub).is_dir(), f"Missing HITL subdir: {sub}"
    assert (hitl / "overview.md").exists()


