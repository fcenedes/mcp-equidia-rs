#!/usr/bin/env python3
"""Check every allowed route through the actual CLI; retain no response content.

HTTP success, nonempty data, missing fixtures and failures are reported separately.
This verifies request dispatch and basic response handling, not every returned
field's correctness or every filter combination. Uses no Equidia credentials.
"""
import argparse
import collections
import concurrent.futures
import datetime
import json
import os
import pathlib
import re
import subprocess
import sys
import zoneinfo

ROOT = pathlib.Path(__file__).resolve().parent.parent


def rows(data):
    if isinstance(data, list):
        return data
    if isinstance(data, dict) and isinstance(data.get("data"), list):
        return data["data"]
    return []


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, default=ROOT / "target/debug/mcp-equidia-rs")
    parser.add_argument("--date", default=datetime.datetime.now(zoneinfo.ZoneInfo("Europe/Paris")).date().isoformat())
    parser.add_argument("--output", type=pathlib.Path, default=ROOT / "docs/endpoint-validation.json")
    parser.add_argument("--strict", action="store_true", help="Fail for missing fixtures and unavailable resources too")
    args = parser.parse_args()
    manifest = json.loads((ROOT / "api/catalog.json").read_text())
    operations = {op["name"]: op for op in manifest["operations"]}
    env = {k: v for k, v in os.environ.items() if not k.startswith(("EQUIDIA_", "MCP_"))}
    env["RUST_LOG"] = "error"
    results = {}

    def call(op, params):
        record = {"tool": op["name"], "path": op["path"]}
        try:
            completed = subprocess.run(
                [str(args.binary.resolve()), "--no-env-file", "--max-retries", "1", "--tool-timeout", "30",
                 "call", op["name"], "--args", json.dumps(params)],
                env=env, capture_output=True, text=True, timeout=35,
            )
        except subprocess.TimeoutExpired:
            record.update(result="request_error", reason="process timeout")
            return record, None
        if completed.returncode:
            match = re.search(r"Equidia returned HTTP (\d+)", completed.stderr)
            status = int(match[1]) if match else None
            record["http_status"] = status
            record["result"] = "resource_unavailable" if status == 404 else "request_error"
            # The CLI's redacted error contains no upstream content. Parameter
            # values (including user IDs and email addresses) are never saved.
            record["reason"] = completed.stderr.strip()[:400]
            return record, None
        try:
            output = json.loads(completed.stdout)
            data, meta = output["data"], output["meta"]
            record.update(http_status=meta["status"], bytes=meta["bytes"], data_type=type(data).__name__)
            if isinstance(data, dict) and any(k in data for k in ("error", "errors")):
                record.update(result="application_error", reason="API returned an error object with HTTP success")
                return record, None
            if not isinstance(data, (dict, list, type(None))):
                record.update(result="application_error", reason="Unexpected scalar response")
                return record, None
            empty = data is None or not data or (isinstance(data, dict) and "data" in data and data["data"] == [])
            record["result"] = "success_empty" if empty else "success_nonempty"
            if isinstance(data, list) or (isinstance(data, dict) and isinstance(data.get("data"), list)):
                record["items"] = len(rows(data))
            return record, data
        except (ValueError, KeyError):
            record.update(result="application_error", reason="CLI output did not contain a valid response envelope")
            return record, None

    def seed(name, params):
        record, data = call(operations[name], params)
        results[name] = record
        candidates = rows(data)
        return candidates[0] if candidates and isinstance(candidates[0], dict) else {}

    meeting = seed("equidia_get_mobile_meetings", {"date": args.date, "items-per-page": 1})
    race = seed("equidia_get_mobile_races", {"date": args.date, "items-per-page": 1})
    runner = seed("equidia_get_mobile_runners", {"race-guid": race["guid"], "items-per-page": 1}) if race else {}
    article = seed("equidia_get_v2_articles", {"items-per-page": 1})
    image = seed("equidia_get_v2_image_parts", {"items-per-page": 1})
    rundown = seed("equidia_get_v2_rundowns", {"items-per-page": 1})
    segment = seed("equidia_get_v2_segments", {"items-per-page": 1})
    subscription = seed("equidia_get_v2_subscriptions", {"items-per-page": 1})
    user = seed("equidia_get_v2_users", {"items-per-page": 1})
    horse = runner.get("horse", {})
    fixture = {
        "date": args.date, "start": args.date, "end": args.date,
        "race-guid": race.get("guid"), "race-uuid": race.get("uuid"),
        "horse-uuid": horse.get("uuid"), "horse": horse.get("uuid"),
        "rider-uuid": runner.get("rider", {}).get("uuid"),
        "trainer-uuid": runner.get("trainer", {}).get("uuid"),
    }

    def check(op):
        props = op["input_schema"]["properties"]
        params = {"items-per-page": 1} if "items-per-page" in props else {}
        for key in ("date", "start", "end", "race-guid", "meeting-date-pre-runners"):
            if key in props:
                value = args.date if key == "meeting-date-pre-runners" else fixture.get(key)
                if value is not None:
                    params[key] = value
        if "meeting-number" in props:
            params["meeting-number"] = str(meeting.get("meetingNumber", 1))
        path = op["path"]
        if "guid" in props:
            params["guid"] = meeting.get("guid") if "meetings" in path else race.get("guid")
        if "uuid" in props:
            resource = horse if "/horse" in path else user if "/users" in path else rundown if "rundowns" in path else race if path.endswith("/race") else runner
            if "guid" not in params:
                params["uuid"] = resource.get("uuid")
        if "id" in props:
            resource = image if "image-parts" in path else subscription if "subscriptions" in path else segment
            if resource.get("id") is not None:
                params["id"] = str(resource["id"])
        if path == "/api/web/article-mobile":
            params["slug"] = article.get("slug")
        if "family-discipline" in props:
            params["family-discipline"] = "FLAT"
        if path == "/api/mobile/runner-by-trainer-or-rider" and fixture["trainer-uuid"]:
            params["trainer-uuid"] = fixture["trainer-uuid"]
        if path == "/api/mobile/runners-by-horse":
            params = {"horse-uuid": horse.get("uuid"), "items-per-page": 1}
        params = {k: v for k, v in params.items() if v is not None}
        missing = set(op["input_schema"]["required"]) - params.keys()
        if missing:
            return {"tool": op["name"], "path": path,
                    "result": "fixture_missing", "reason": "Missing public fixture: " + ", ".join(sorted(missing))}
        record, _ = call(op, params)
        return record

    remaining = [op for name, op in operations.items() if name not in results]
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        for record in pool.map(check, remaining):
            results[record["tool"]] = record
            print(f"{record['result']:22} {record['tool']}", flush=True)
    ordered = [results[name] for name in operations]
    summary = dict(collections.Counter(r["result"] for r in ordered))
    report = {
        "checked_at": datetime.datetime.now(datetime.timezone.utc).isoformat(), "racing_date": args.date,
        "credentials_used": False, "canonical_routes": len(ordered), "summary": summary,
        "limits": ["One representative request per route; filter combinations and returned field accuracy are not exhaustively verified.",
                   "Empty success establishes access and decoding, not correct filtering or useful data.",
                   "Aliases share canonical routes and are covered separately by the offline dispatch test."],
        "operations": ordered,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    markdown = "# Live endpoint validation\n\n" + f"Checked {report['checked_at']} through the actual CLI, without credentials. Racing date: {args.date}.\n\n"
    markdown += "| Result | Routes |\n| --- | ---: |\n" + "".join(f"| {k} | {v} |\n" for k, v in summary.items())
    markdown += "\n" + "\n".join("- " + s for s in report["limits"]) + "\n\n| Tool | Result | HTTP | Note |\n| --- | --- | --- | --- |\n"
    for r in ordered:
        markdown += f"| `{r['tool']}` | {r['result']} | {r.get('http_status', '—')} | {r.get('reason', '').replace('|', '/')} |\n"
    args.output.with_suffix(".md").write_text(markdown)
    print(json.dumps(summary, sort_keys=True))
    failures = {"request_error", "application_error"}
    if args.strict:
        failures |= {"fixture_missing", "resource_unavailable"}
    sys.exit(1 if any(r["result"] in failures for r in ordered) else 0)


if __name__ == "__main__":
    main()
