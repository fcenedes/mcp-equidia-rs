#!/usr/bin/env python3
"""Write a compact, portable source coverage summary from cargo-llvm-cov JSON."""
import argparse
import datetime
import json
import pathlib
import platform

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=pathlib.Path, default=ROOT / "target/coverage/coverage.json")
    parser.add_argument("--output", type=pathlib.Path, default=ROOT / "docs/test-coverage.json")
    args = parser.parse_args()
    coverage = json.loads(args.input.read_text())["data"][0]
    report = {
        "measured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "tool": "cargo-llvm-cov 0.8.7", "platform": f"{platform.system()} {platform.machine()}",
        "scope": "Rust source files; integration test/helper files excluded. Subprocess profiles preserved and HTTP/TLS servers terminated gracefully.",
        "branch_coverage_measured": False,
        "totals": {key: coverage["totals"][key] for key in ("lines", "functions", "regions")},
        "files": [{"path": "src/" + pathlib.Path(f["filename"]).name,
                   **{key: f["summary"][key] for key in ("lines", "functions", "regions")}}
                  for f in coverage["files"]],
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Lines: {report['totals']['lines']['percent']:.2f}%; functions: {report['totals']['functions']['percent']:.2f}%")


if __name__ == "__main__":
    main()
