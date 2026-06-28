#!/usr/bin/env python3
"""Generate a rollback bash script from an approved-proposals YAML file.

Each proposal must include a `rollback` field with a shell command or
`rollback_files` (list of path/content pairs that restore prior file
content). The script produces one bash file that rolls changes back in
reverse order.

Usage:
    generate_rollback.py <approved-proposals.yaml> -o <rollback-YYYY-MM-DD.sh>
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("proposals", type=Path)
    ap.add_argument("-o", "--output", type=Path, required=True)
    args = ap.parse_args()

    try:
        import yaml
    except ImportError:
        print("ERROR: pip install pyyaml", file=sys.stderr)
        return 1

    data = yaml.safe_load(args.proposals.read_text())
    if not isinstance(data, dict) or "proposals" not in data:
        print("ERROR: proposals file must have a top-level 'proposals' list", file=sys.stderr)
        return 1

    approved = [p for p in data["proposals"] if p.get("decision") == "approved"]
    print(f"found {len(approved)} approved proposals")

    lines = [
        "#!/usr/bin/env bash",
        "# Auto-generated rollback. Rolls approved proposals back in reverse order.",
        "# Generated from: " + str(args.proposals),
        "set -euo pipefail",
        "",
        "REPO_ROOT=\"${NLR_ROOT:-$(cd \"$(dirname \"${BASH_SOURCE[0]}\")/..\" && pwd)}\"",
        "cd \"$REPO_ROOT\"",
        "",
        "echo \"Rolling back {} approved proposals...\"".format(len(approved)),
        "",
    ]

    # Reverse so later changes roll back before earlier ones they may depend on
    for i, p in enumerate(reversed(approved)):
        lines.append(f"# [{i}] {p.get('slug', 'unnamed')}: {p.get('change', '')}")
        if "rollback" in p:
            lines.append(p["rollback"])
        elif "rollback_files" in p:
            for rf in p["rollback_files"]:
                path = rf["path"].replace('"', '\\"')
                content = rf["content"]
                # Write via heredoc to preserve content exactly
                lines.append(f"cat > \"{path}\" <<'__ROLLBACK_EOF__'")
                lines.append(content.rstrip())
                lines.append("__ROLLBACK_EOF__")
        else:
            lines.append(f"# NO ROLLBACK — proposal lacked rollback field. Manual intervention required.")
            lines.append(f"echo 'WARN: proposal {p.get(\"slug\")} had no rollback' >&2")
        lines.append("")

    lines.append("echo \"Rollback complete.\"")

    args.output.write_text("\n".join(lines) + "\n")
    args.output.chmod(0o755)
    print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
