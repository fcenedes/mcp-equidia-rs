# mcp-equidia-rs

[Release and publishing guide](docs/release.md) · [Tool reference](docs/tools.md) · [Test evidence](docs/testing.md)

A **read-only Equidia API MCP server and CLI**, built in Rust using the same
catalogue, transport and deployment approach as [mcp-gmap-rs](https://github.com/fcenedes/mcp-gmap-rs).

It covers **60 public GET operations** from [Equidia's published OpenAPI contract](https://apiv2.equidia.fr/api/docs.json),
with 11 short aliases for everyday queries and one offline discovery tool.
There are **72 MCP tools** in total. No POST, PUT, PATCH or DELETE operation is
exposed or dispatched. The specification is bundled, so startup and tool
discovery do not depend on the API being available.

Use it for meetings, races, results, runners, horse profiles and statistics,
predictions, replay metadata, live stream metadata, TV programmes, articles,
tags, broadcast editorial information and other documented read resources.
The credential-dependent notification routes and Braze user-status route are excluded.
The checked-in access policy prevents newly added upstream routes from being
exposed before their anonymous access has been reviewed.

## Build and run

Requires Rust 1.88 or later.

```sh
cargo build --release --locked
./target/release/mcp-equidia-rs doctor
./target/release/mcp-equidia-rs doctor --live
./target/release/mcp-equidia-rs races --date 2026-11-11 --items-per-page 5
```

For a local installation:

```sh
cargo install --path . --locked
mcp-equidia-rs --help
```

No Equidia API token is needed, configured or sent. This project does not implement
login, token refresh, media decryption or permission elevation.

## MCP stdio

With no command, the executable starts the MCP stdio server. You can also use
`serve stdio`. Diagnostics go to stderr; stdout contains only MCP messages.

An MCP client configuration using an absolute executable path:

```json
{
  "mcpServers": {
    "equidia": {
      "command": "/absolute/path/to/mcp-equidia-rs",
      "args": ["--no-env-file"],
      "env": {
        "EQUIDIA_ENABLED_TOOLS": "equidia_meetings,equidia_races,equidia_race,equidia_runners,equidia_horse,equidia_horse_stats,equidia_predictions,equidia_live_streams,equidia_tv_schedule,equidia_articles,equidia_tags,equidia_api_catalog"
      }
    }
  }
}
```

Omit `EQUIDIA_ENABLED_TOOLS` to expose all 72 tools. A disabled tool cannot be
called, including by its alias. Aliases and canonical names are separate
entries: enable the exact names your client will use.

## CLI

Every tool has a command with its `equidia_` prefix removed and underscores
replaced by hyphens. JSON calls and named flags share the same validation.
API query argument names retain their exact spelling in JSON.

```sh
mcp-equidia-rs tools
mcp-equidia-rs schema runners
mcp-equidia-rs api-catalog --query horse

mcp-equidia-rs meetings --date 2026-11-11
mcp-equidia-rs races --date 2026-11-11 --family-discipline FLAT --is-quinte
mcp-equidia-rs race --guid 20261111_STR_1
mcp-equidia-rs runners --race-guid 20261111_STR_1 --items-per-page 20
mcp-equidia-rs predictions --date 2026-11-11
mcp-equidia-rs tv-schedule --date 2026-11-11 --stream equidia-live

mcp-equidia-rs call equidia_races --args '{"date":"2026-11-11","page":2,"items-per-page":5}'
mcp-equidia-rs get-v2-tags --uuids '["tag-uuid-1","tag-uuid-2"]'
mcp-equidia-rs horse --uuid HORSE_UUID
mcp-equidia-rs get-mobile-runners-by-horse --horse-uuid HORSE_UUID
mcp-equidia-rs get-v2-runners-by-uuid --uuid RUNNER_UUID

printf '%s' '{"date":"2026-11-11"}' | mcp-equidia-rs call races --args -
mcp-equidia-rs races --date 2026-11-11 --output races.json
```

Start with `meetings` and `races`; use the returned UUIDs or GUIDs for related
resources. A race GUID such as `20261111_STR_1` identifies a race; meeting
numbers alone are not GUIDs. Dates are explicit `YYYY-MM-DD` strings; missing
dates use the API's own default, rather than a locally inferred date.

Boolean flags can be switches (`--is-quinte`) or explicit values
(`--is-quinte false`). Untyped upstream filters are accepted as strings. Use
`schema TOOL` for the precise types. The family discipline enum is `FLAT`,
`JUMP`, `HARNESS`, `SADDLED`; sort enums are `ASC` and `DESC`.

## Results and pagination

Responses use the same JSON envelope in the CLI and MCP `structuredContent`:

```json
{
  "data": [{"uuid": "...", "name": "..."}],
  "meta": {
    "source": "Equidia",
    "url": "https://apiv2.equidia.fr/api/mobile/races?date=2026-11-11&items-per-page=10",
    "status": 200,
    "contentType": "application/json",
    "bytes": 1234
  }
}
```

`data` preserves the upstream object, array or null value. The catalogue tool
returns its own offline metadata. `--output` writes the JSON envelope.
Replay/player and stream URLs in JSON remain metadata and are
not followed or downloaded.

Collections accept `page` and `items-per-page` where documented. Page numbers
start at 1, page size is restricted to 1–100, and the client requests 10 when
page size is omitted. No automatic pagination occurs. Some endpoints, including
streams and EPG, may ignore page size or return an object despite their
documented array response; the client preserves what the API returns.
The response byte limit still applies.

## Streamable HTTP

```sh
MCP_HTTP_TOKEN='replace-with-your-token' mcp-equidia-rs serve http --listen 127.0.0.1:3000
```

- `/mcp`: MCP Streamable HTTP, with sessions and JSON/SSE negotiation.
- `/health`: public process liveness; it does not probe Equidia.
- `/metrics`: optional Prometheus metrics, protected by the MCP bearer token.

Clients send `Authorization: Bearer <MCP_HTTP_TOKEN>`. This token protects your
own MCP listener and is never sent to Equidia. Non-loopback
bindings require an MCP token. With no allowlist, the transport accepts loopback
hosts and rejects browser origins. Configure the public host and permitted
browser origins when using a proxy:

```sh
MCP_HTTP_TOKEN='replace-with-your-token' \
MCP_HTTP_ALLOWED_HOSTS='equidia.example.com' \
MCP_HTTP_ALLOWED_ORIGINS='https://your-client.example.com' \
mcp-equidia-rs serve http --listen 0.0.0.0:3000 --metrics true
```

Native TLS is available with `--tls-cert chain.pem --tls-key key.pem`.
Add `--tls-client-ca clients-ca.pem` to require client certificates; bearer
authentication still applies. On Unix, `SIGHUP` reloads TLS files atomically
and retains the previous configuration if loading fails. A reverse proxy may
also terminate TLS. SIGINT/SIGTERM trigger graceful shutdown.

## Configuration

Precedence is **CLI flags > process environment > dotenv file > defaults**.
The optional `.env` is loaded from the working directory without mutating the
process environment. Select a file using `--env-file` or `MCP_ENV_FILE`;
`--no-env-file` disables loading. See [.env.example](.env.example).

| Variable | Default | Purpose |
| --- | --- | --- |
| `EQUIDIA_BASE_URL` | `https://apiv2.equidia.fr` | API origin only, without `/api`; HTTPS required except loopback tests |
| `EQUIDIA_ENABLED_TOOLS` | `*` | Comma-separated exact MCP tool names |
| `EQUIDIA_TIMEOUT` | `30` | Per-request timeout in seconds, 1–600 |
| `EQUIDIA_TOOL_TIMEOUT` | `120` | Total tool deadline including queueing and retries, 1–3600 seconds |
| `EQUIDIA_MAX_RETRIES` | `2` | Extra attempts for network failures, HTTP 429 and 5xx, 0–5 |
| `EQUIDIA_MAX_RESPONSE_BYTES` | `8388608` | Per-response limit, 1 byte–100 MiB |
| `EQUIDIA_CONCURRENCY` | `4` | Concurrent upstream calls per process, 1–64 |
| `MCP_HTTP_LISTEN` | `127.0.0.1:3000` | HTTP bind address |
| `MCP_HTTP_TOKEN` | empty | MCP listener bearer token; required off loopback |
| `MCP_HTTP_ALLOWED_HOSTS` | loopback | Comma-separated Host allowlist |
| `MCP_HTTP_ALLOWED_ORIGINS` | none | Comma-separated Origin allowlist |
| `MCP_HTTP_RATE_LIMIT` | `120` | Process-wide authenticated `/mcp` requests/minute with an equal burst; 0 disables |
| `MCP_METRICS_ENABLED` | `false` | Enable `/metrics` |
| `MCP_TLS_CERT`, `MCP_TLS_KEY` | empty | Native TLS certificate/key pair |
| `MCP_TLS_CLIENT_CA` | empty | Client CA for mTLS |
| `MCP_LOG_FORMAT` | `text` | `text` or `json` |
| `RUST_LOG` | `warn` | Diagnostic log filter |

Retries respect `Retry-After` in seconds or HTTP-date form and the total tool
deadline. Redirects are refused. Upstream error bodies and authorization values
are not included in error messages. Results are not cached; fetch again for
updates. API freshness and completeness are controlled by Equidia.

## Docker

Versioned local images and a portable two-platform OCI archive are prepared for
distribution. GHCR publishing, native binary releases, crates.io publishing and
docs.rs builds are configured in the [release guide](docs/release.md). Registry
URLs there become usable after the first publication.

```sh
docker build -t mcp-equidia-rs .
docker run --rm -p 127.0.0.1:3000:3000 \
  -e MCP_HTTP_TOKEN='replace-with-your-token' mcp-equidia-rs

# Or use Compose (the variable is required):
MCP_HTTP_TOKEN='replace-with-your-token' docker compose -f deploy/compose.yaml up --build
```

The container runs as UID 10001. For stdio, use
`docker run --rm -i mcp-equidia-rs serve stdio` without publishing a port.

After publishing, `deploy/compose.image.yaml` pulls the versioned GHCR image;
`deploy/compose.yaml` builds the local source. Set `EQUIDIA_IMAGE` to pin a
different version or digest when using the image-based file.

## Verification and API maintenance

```sh
python3 scripts/generate_catalog.py --check
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps

# Optional public API checks through the compiled executable; no credentials:
python3 scripts/live_smoke.py --date 2026-11-11 --output docs/live-validation.json
```

Integration tests use local mock upstreams and exercise the actual CLI and MCP
executables. They cover catalogue completeness, GET-only dispatch, schemas,
query serialization, pagination, tokens, transient retries, response limits,
deadlines, JSON responses, dotenv precedence, stdio, HTTP sessions, origin/host
validation and metrics. Live checks are separate from the offline test suite.
The latest recorded live results are in [docs/live-validation.json](docs/live-validation.json).

For measured source coverage, the complete 60-route live matrix and remaining
gaps, see [docs/testing.md](docs/testing.md). Run `python3 scripts/live_matrix.py`
for a fresh full-route check; its report distinguishes nonempty successes,
empty successes, missing fixtures and failures. Source coverage is 95.05% on the
recorded macOS run; CI enforces a 90% minimum.

The anonymous access audit is [docs/anonymous-access.json](docs/anonymous-access.json).
The reviewed route allowlist is [api/access-policy.json](api/access-policy.json).
The unmodified upstream contract is [api/openapi.json](api/openapi.json).
The generated read-only manifest is [api/catalog.json](api/catalog.json);
[docs/tools.md](docs/tools.md) lists every canonical tool and alias. Refresh the
snapshot explicitly, review its diff and update `api/access-policy.json` after
checking anonymous access for any changed routes, then regenerate:

```sh
curl --fail --silent --show-error https://apiv2.equidia.fr/api/docs.json -o api/openapi.json
python3 scripts/generate_catalog.py
cargo test --locked
```

When refreshing the snapshot, update its retrieval date in the generator and
catalogue metadata in `src/catalog.rs` as well.

The generator repairs known contract defects: `[]` query schemas become strings;
OpenAPI nullable schemas become JSON Schema unions; invalid class-name types
remain unconstrained with an explanatory description; a response-shaped GET
request body is ignored. Horse/rider/trainer filters on `mobile/runners`, and
rider/trainer filters on `mobile/runners-by-horse`, are optional, as verified by
live calls on 2026-11-11. Page limits are deliberate client bounds.

MIT licensed. This is an independent client, unaffiliated with Equidia.
Equidia data and media remain subject to their owners' rights; see [NOTICE](NOTICE).
