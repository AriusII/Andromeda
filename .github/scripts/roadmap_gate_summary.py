#!/usr/bin/env python3
"""CI entry point for the read-only roadmap gate summary."""

from __future__ import annotations

import runpy
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tools" / "testing" / "roadmap_gate_summary.py"


def main() -> int:
    if not SCRIPT.exists():
        print(f"Missing roadmap gate summary script: {SCRIPT}", file=sys.stderr)
        return 1

    sys.argv[0] = str(SCRIPT)
    runpy.run_path(str(SCRIPT), run_name="__main__")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
