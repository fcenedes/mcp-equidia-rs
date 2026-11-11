#!/usr/bin/env python3
"""Generate the MCP catalogue offline from the checked-in Equidia OpenAPI snapshot.

Run from any directory. No third-party dependencies. --check verifies drift.
Refresh api/openapi.json separately and review its diff before regenerating.
"""
import argparse
import copy
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SPEC = json.loads((ROOT / "api/openapi.json").read_text())
ACCESS = json.loads((ROOT / "api/access-policy.json").read_text())
TYPES = {"array", "boolean", "integer", "null", "number", "object", "string"}
ALIASES = {
    "equidia_races": "equidia_get_mobile_races",
    "equidia_meetings": "equidia_get_mobile_meetings",
    "equidia_race": "equidia_get_mobile_race",
    "equidia_runners": "equidia_get_mobile_runners",
    "equidia_horse": "equidia_get_mobile_horse",
    "equidia_horse_stats": "equidia_get_mobile_horse_stats",
    "equidia_predictions": "equidia_get_mobile_daily_predictions",
    "equidia_live_streams": "equidia_get_mobile_live_streams",
    "equidia_tv_schedule": "equidia_get_tv_epg",
    "equidia_articles": "equidia_get_v2_articles",
    "equidia_tags": "equidia_get_v2_tags",
}
PURPOSE = {
    "/api/mobile/meetings": "List race meetings by date, including racecourse, meeting number, status and weather.",
    "/api/mobile/races": "List races by date, meeting/race number or discipline; includes results when available. Use UUID or GUID values for related lookups.",
    "/api/mobile/race": "Get race details, results and available replay metadata by race UUID or GUID.",
    "/api/mobile/runners": "List race runners by race UUID/GUID, or filter by horse, rider or trainer. Includes horse and connection identifiers.",
    "/api/mobile/runners-analyse": "Read runner analysis for a race UUID or GUID.",
    "/api/mobile/horse": "Read a horse profile by UUID or slug.",
    "/api/mobile/horse-stats": "Read horse statistics by UUID or slug.",
    "/api/mobile/runners-by-horse": "Read a horse's previous race performances using horse-uuid, with optional comparison filters.",
    "/api/mobile/daily-predictions": "Read Equidia's editorial predictions for a date. The upstream returns a dailies object.",
    "/api/mobile/live-streams": "Read live channel and meeting stream metadata for a date. Does not fetch or play media.",
    "/api/tv/epg": "Read the TV programme schedule by date and optional Equidia channel.",
    "/api/v2/articles": "List editorial articles, optionally filtering by tag UUIDs.",
    "/api/v2/tags": "Read tag metadata, optionally selecting UUIDs.",
}


def schema(raw, definitions):
    """Convert OpenAPI 3.0 Schema Objects to JSON Schema 2020-12 input schemas."""
    if not isinstance(raw, dict):
        return {"type": "string"}  # Equidia uses [] for untyped query parameters.
    src = copy.deepcopy(raw)
    if "$ref" in src:
        ref = src["$ref"]
        if not ref.startswith("#/components/schemas/"):
            raise ValueError(f"unsupported reference: {ref}")
        name = ref.rsplit("/", 1)[1]
        if name not in definitions:
            definitions[name] = {}  # Break recursive references.
            definitions[name] = schema(SPEC["components"]["schemas"][name], definitions)
        out = {"$ref": "#/$defs/" + name}
    else:
        out = {k: v for k, v in src.items() if k in {
            "type", "description", "enum", "default", "format", "pattern",
            "minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum",
            "minLength", "maxLength", "minItems", "maxItems", "uniqueItems",
            "multipleOf", "required", "additionalProperties", "minProperties", "maxProperties",
        }}
        if out.get("type") not in TYPES and "type" in out:
            # The source sometimes puts class names (e.g. Category) in `type`.
            name = out.pop("type")
            out["description"] = (out.get("description", "") + f" Upstream type: {name}; contract unspecified.").strip()
        if "properties" in src:
            out["properties"] = {k: schema(v, definitions) for k, v in src["properties"].items() if not v.get("readOnly")}
            if "required" in out:
                out["required"] = [k for k in out["required"] if k in out["properties"]]
        if "items" in src:
            out["items"] = schema(src["items"], definitions)
        for key in ("oneOf", "anyOf", "allOf"):
            if key in src:
                out[key] = [schema(s, definitions) for s in src[key]]
        if isinstance(out.get("additionalProperties"), dict):
            out["additionalProperties"] = schema(out["additionalProperties"], definitions)
        for key in ("exclusiveMinimum", "exclusiveMaximum"):
            if isinstance(out.get(key), bool):
                enabled = out.pop(key)
                bound = key.replace("exclusive", "").lower()
                if enabled and bound in out:
                    out[key] = out.pop(bound)
    if src.get("nullable"):
        out = {"anyOf": [out, {"type": "null"}]}
    return out


def generate():
    operations = []
    known = {p for p, methods in SPEC["paths"].items() if "get" in methods}
    reviewed = set(ACCESS["allowed_paths"]) | set(ACCESS["excluded"])
    if known != reviewed:
        raise ValueError("GET routes changed; review anonymous access and update api/access-policy.json")
    for path, item in SPEC["paths"].items():
        if path not in ACCESS["allowed_paths"]:
            continue
        for method, op in item.items():
            if method != "get":
                continue
            auth_errors = [r.get("description", "") for r in op.get("responses", {}).values()]
            if op.get("security") or any(re.search(r"token|api.?key|unauthori[sz]ed", d, re.I) for d in auth_errors):
                raise ValueError(f"credential-dependent route cannot be public: {path}")
            suffix = path.removeprefix("/api/").replace("{", "by_").replace("}", "")
            name = "equidia_" + method + "_" + re.sub(r"[^a-zA-Z0-9]+", "_", suffix).strip("_")
            definitions, props, required, parameters = {}, {}, [], []
            raw_params = item.get("parameters", []) + (op.get("parameters", []) if isinstance(op.get("parameters", []), list) else [])
            for p in raw_params:
                prop = schema(p.get("schema", {}), definitions)
                if not prop:
                    prop = {"type": "string"}
                description = p.get("description", "")
                if description:
                    prop["description"] = description
                key = p["name"]
                if key == "page":
                    prop.update(type="integer", minimum=1, maximum=100000)
                if key == "items-per-page":
                    prop.update(type="integer", minimum=1, maximum=100, default=10)
                if "yyyy-mm-dd" in description.lower() or prop.get("format") == "date":
                    prop.update(type="string", format="date", pattern=r"^\d{4}-\d{2}-\d{2}$")
                if p["in"] == "path":
                    prop.update(type="string", minLength=1, maxLength=512, pattern=r"^[A-Za-z0-9_-][A-Za-z0-9_.-]*$")
                props[key] = prop
                # Verified against live GETs on 2026-11-11: race-guid alone
                # selects runners; horse-uuid alone selects horse history.
                required_here = p.get("required", False)
                if path == "/api/mobile/runners" and key in {"horse-uuid", "rider-uuid", "trainer-uuid"}:
                    required_here = False
                if path == "/api/mobile/runners-by-horse" and key in {"rider-uuid", "trainer-uuid"}:
                    required_here = False
                if required_here:
                    required.append(key)
                parameters.append({"name": key, "location": p["in"], "explode": p.get("explode", False), "style": p.get("style", "form")})
            # Equidia incorrectly documents a response-shaped requestBody on
            # GET daily-predictions. Read calls never send a request body.
            body = {}
            if body:
                content = body.get("content", {})
                if "application/json" not in content:
                    raise ValueError(f"unsupported body: {name}")
                props["body"] = schema(content["application/json"].get("schema", {}), definitions)
                if body.get("required"):
                    required.append("body")
            input_schema = {"type": "object", "properties": props, "required": required, "additionalProperties": False}
            if definitions:
                input_schema["$defs"] = definitions
            operations.append({
                "name": name, "method": method.upper(), "path": path,
                "operation_id": op.get("operationId", ""), "tags": op.get("tags", []),
                "description": f"{method.upper()} {path}. {PURPOSE.get(path) or op.get('description') or op.get('summary', '')}",
                "parameters": parameters, "input_schema": input_schema,
                "has_body": bool(body),
            })
    names = [o["name"] for o in operations]
    assert len(names) == len(set(names)) and all(len(n) <= 64 for n in names)
    for alias, target in ALIASES.items():
        assert target in names, target
    return {"source": "https://apiv2.equidia.fr/api/docs.json", "retrieved": "2026-11-11", "aliases": ALIASES, "operations": operations}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    catalog = generate()
    output = json.dumps(catalog, indent=2, ensure_ascii=False) + "\n"
    table = "# Read tool reference\n\nGenerated from the bundled Equidia OpenAPI snapshot (2026-11-11). All canonical tools use GET.\n\n"
    table += "| Tool | Endpoint | Required arguments |\n| --- | --- | --- |\n"
    for op in catalog["operations"]:
        table += f"| `{op['name']}` | `{op['path']}` | {', '.join('`' + k + '`' for k in op['input_schema']['required']) or '—'} |\n"
    table += "\n## Common aliases\n\n| Alias | Canonical tool |\n| --- | --- |\n"
    for alias, target in ALIASES.items():
        table += f"| `{alias}` | `{target}` |\n"
    table += "\n`equidia_api_catalog` searches enabled reads locally. Use `schema TOOL` or `tools` for full parameter descriptions, enums, types and limits.\n"
    for path, text in [(ROOT / "api/catalog.json", output), (ROOT / "docs/tools.md", table)]:
        if args.check:
            if not path.exists() or path.read_text() != text:
                sys.exit(f"{path.relative_to(ROOT)} is stale; run scripts/generate_catalog.py")
        else:
            path.write_text(text)
    print(f"{'Checked' if args.check else 'Generated'} {len(catalog['operations'])} operations")


if __name__ == "__main__":
    main()
