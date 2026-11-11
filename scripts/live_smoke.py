#!/usr/bin/env python3
"""Exercise public read tools through the built executable, without credentials."""
import argparse
import datetime
import json
import os
import pathlib
import subprocess
import sys
import zoneinfo

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, default=ROOT / "target/debug/mcp-equidia-rs")
    parser.add_argument("--date", default=datetime.datetime.now(zoneinfo.ZoneInfo("Europe/Paris")).date().isoformat())
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args()
    checks = []
    env = {k: v for k, v in os.environ.items() if not k.startswith(("EQUIDIA_", "MCP_"))}

    def call(tool, params):
        result = subprocess.run([str(args.binary), "--no-env-file", "--max-retries", "1", "--tool-timeout", "30", "call", tool, "--args", json.dumps(params)], env=env, capture_output=True, timeout=35)
        check = {"tool": tool, "ok": result.returncode == 0}
        checks.append(check)
        if result.returncode:
            check["error"] = result.stderr.decode(errors="replace").strip()
            print(f"FAIL {tool}: {check['error']}", file=sys.stderr)
            return None
        output = json.loads(result.stdout)
        data = output["data"]
        check.update(status=output["meta"]["status"], bytes=output["meta"]["bytes"], data_type=type(data).__name__)
        if isinstance(data, list):
            check["items"] = len(data)
        print(f"PASS {tool} ({check['bytes']} bytes)")
        return data

    date = {"date": args.date, "items-per-page": 1}
    meetings = call("equidia_meetings", date)
    races = call("equidia_races", date)
    for tool in ["equidia_predictions", "equidia_live_streams", "equidia_tv_schedule",
                 "equidia_get_mobile_race_quinte", "equidia_get_mobile_races_by_highlight",
                 "equidia_get_tv_meetings"]:
        call(tool, date)
    for tool in ["equidia_articles", "equidia_tags", "equidia_get_v2_categories", "equidia_get_tv_shows"]:
        call(tool, {"items-per-page": 1})
    if meetings:
        call("equidia_get_v2_meetings_by_guid", {"guid": meetings[0]["guid"], "items-per-page": 1})
    if races:
        race = races[0]
        call("equidia_race", {"guid": race["guid"], "items-per-page": 1})
        runners = call("equidia_runners", {"race-guid": race["guid"], "items-per-page": 1})
        call("equidia_get_mobile_runners_analyse", {"race-guid": race["guid"], "items-per-page": 1})
        if runners:
            runner = runners[0]
            call("equidia_get_v2_runners_by_uuid", {"uuid": runner["uuid"]})
            horse = runner["horse"]["uuid"]
            call("equidia_horse", {"uuid": horse, "items-per-page": 1})
            call("equidia_horse_stats", {"uuid": horse, "items-per-page": 1})
            call("equidia_get_mobile_runners_by_horse", {"horse-uuid": horse, "items-per-page": 1})
    report = {"checked_at": datetime.datetime.now(datetime.timezone.utc).isoformat(), "racing_date": args.date,
              "origin": "https://apiv2.equidia.fr", "credentials_used": False,
              "ok": all(c["ok"] for c in checks), "checks": checks}
    if args.output:
        args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"{sum(c['ok'] for c in checks)}/{len(checks)} public read checks passed")
    sys.exit(0 if report["ok"] else 1)


if __name__ == "__main__":
    main()
