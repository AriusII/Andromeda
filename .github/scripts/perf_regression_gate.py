#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Dict, List


def load_json(path: Path) -> Dict:
    return json.loads(path.read_text(encoding="utf-8"))


def pct_change(current: float, baseline: float) -> float:
    if baseline == 0:
        return 0.0
    return ((current - baseline) / baseline) * 100.0


def main() -> int:
    parser = argparse.ArgumentParser(description="Wave 14 performance regression gate")
    parser.add_argument(
        "--baseline",
        default=".github/perf/baseline.wave14.json",
        help="Path to baseline JSON",
    )
    parser.add_argument(
        "--thresholds",
        default=".github/perf/thresholds.wave14.json",
        help="Path to thresholds JSON",
    )
    parser.add_argument(
        "--current",
        default="",
        help="Optional current metrics JSON; default reuses baseline",
    )
    parser.add_argument(
        "--output-dir",
        default="target/perf-regression",
        help="Output directory for reports",
    )
    args = parser.parse_args()

    baseline_path = Path(args.baseline)
    thresholds_path = Path(args.thresholds)
    out_dir = Path(args.output_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    baseline = load_json(baseline_path)
    thresholds = load_json(thresholds_path)

    current_path = Path(args.current) if args.current else None
    env_current = os.getenv("PERF_CURRENT_JSON", "").strip()
    if env_current:
        current_path = Path(env_current)

    current = baseline if current_path is None else load_json(current_path)

    metrics = [
        ("invocation_latency_ms", "latency"),
        ("commit_latency_ms", "latency"),
        ("wal_throughput_mb_s", "throughput"),
        ("recovery_time_ms", "latency"),
    ]

    failures: List[str] = []
    report_rows: List[Dict] = []

    for metric, metric_type in metrics:
        base = float(baseline["metrics"][metric])
        cur = float(current["metrics"][metric])
        delta = pct_change(cur, base)

        if metric_type == "throughput":
            limit = float(thresholds["throughput_drop_percent"])
            failed = delta < -limit
            threshold_text = f"-{limit:.2f}% min"
        else:
            limit = float(thresholds["latency_regression_percent"])
            failed = delta > limit
            threshold_text = f"+{limit:.2f}% max"

        status = "FAIL" if failed else "PASS"
        if failed:
            failures.append(
                f"{metric}: current={cur}, baseline={base}, delta={delta:.2f}%, threshold={threshold_text}"
            )

        report_rows.append(
            {
                "metric": metric,
                "type": metric_type,
                "baseline": base,
                "current": cur,
                "delta_percent": round(delta, 4),
                "threshold": threshold_text,
                "status": status,
            }
        )

    summary = {
        "wave": "14",
        "baseline_version": baseline["baseline_version"],
        "threshold_policy_version": thresholds["policy_version"],
        "status": "FAIL" if failures else "PASS",
        "rows": report_rows,
    }
    (out_dir / "performance-report.json").write_text(
        json.dumps(summary, indent=2), encoding="utf-8"
    )

    md_lines = [
        "# Wave 14 performance regression report",
        "",
        f"- Status: **{summary['status']}**",
        f"- Baseline version: `{baseline['baseline_version']}`",
        f"- Threshold policy version: `{thresholds['policy_version']}`",
        "",
        "| Metric | Baseline | Current | Delta | Threshold | Status |",
        "|---|---:|---:|---:|---|---|",
    ]
    for row in report_rows:
        md_lines.append(
            f"| {row['metric']} | {row['baseline']:.3f} | {row['current']:.3f} | {row['delta_percent']:.2f}% | {row['threshold']} | {row['status']} |"
        )

    if failures:
        md_lines.extend(["", "## Regression failures", ""])
        md_lines.extend([f"- {item}" for item in failures])

    (out_dir / "performance-report.md").write_text(
        "\n".join(md_lines) + "\n", encoding="utf-8"
    )

    if failures:
        for item in failures:
            print(f"::error::{item}")
        return 1

    print("Wave 14 performance regression gate passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
