# Equidia MCP and CLI

This crate provides a standalone read-only Equidia API executable and its
[`run`] entry point. A shared catalogue validates inputs for the CLI, MCP stdio
and MCP Streamable HTTP. It covers 60 public GET routes, with 11 short aliases
and offline discovery: 72 MCP tools in total. No Equidia credentials are used.

## Installation

After the first crates.io publication:

```sh
cargo install mcp-equidia-rs --locked
mcp-equidia-rs --help
```

For a source checkout, use `cargo install --path . --locked`. Release archives
and container images are described in the
[distribution guide](https://github.com/fcenedes/mcp-equidia-rs/blob/main/docs/release.md).

## Read racing data

```sh
mcp-equidia-rs meetings --date 2026-11-11
mcp-equidia-rs races --date 2026-11-11 --items-per-page 5
mcp-equidia-rs race --guid 20261111_STR_1
mcp-equidia-rs runners --race-guid 20261111_STR_1
mcp-equidia-rs call equidia_horse --args '{"uuid":"HORSE_UUID"}'
```

Start with meetings and races, then use the returned UUIDs or GUIDs for related
resources. Dates use `YYYY-MM-DD`; pagination is explicit. Responses preserve
upstream data inside a JSON envelope with source, request URL, status and byte
count metadata. Media URLs remain metadata and are not downloaded.

## Inspect the catalogue offline

```sh
mcp-equidia-rs tools
mcp-equidia-rs schema runners
mcp-equidia-rs api-catalog --query horse
```

The bundled catalogue is derived from the 2026-11-11 OpenAPI snapshot, reviewed
for anonymous access. Credential-dependent routes and non-JSON document
retrieval are excluded. Tool discovery and startup do not download the contract.

## MCP transports

With no command the executable starts MCP stdio; `serve stdio` is equivalent.
Diagnostics go to stderr and MCP messages use stdout.

```sh
mcp-equidia-rs --no-env-file serve stdio
MCP_HTTP_TOKEN='choose-a-listener-token' mcp-equidia-rs serve http
```

Streamable HTTP uses `/mcp`; `/health` reports process liveness. Optional
Prometheus metrics use `/metrics`. The listener token protects this MCP server
and is never forwarded to Equidia. Non-loopback bindings require it. Native TLS,
mTLS, host/origin allowlists and graceful shutdown are supported.

## Configuration and failures

CLI flags override process environment, then the optional dotenv file, then
defaults. `--no-env-file` disables dotenv loading. `EQUIDIA_ENABLED_TOOLS` limits
the exact exposed MCP tool names. Requests have deadlines, bounded response
sizes, bounded concurrency and transient retries; redirects are refused.

Use `doctor` to check local configuration and `doctor --live` to probe meetings.
CLI errors set a nonzero exit status. MCP tool execution failures return an
error result; unknown tool names are invalid parameters. Empty upstream arrays
are successful responses and do not imply that matching data exists.

The public Rust API is the process entry point [`run`]; API queries are exposed
through the CLI and MCP tools. The
[README](https://github.com/fcenedes/mcp-equidia-rs#readme) documents all environment
variables, deployment examples and the current test evidence.
