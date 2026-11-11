# What is verified

Coverage is not complete. On 2026-11-11 the offline suite measured **95.05% line
coverage** (1,018 of 1,071 source lines), **84.13% function coverage** and
**92.15% region coverage** on macOS aarch64. Branch coverage was not measured.
The baseline before the additional tests was 86.33% line coverage.

The suite has 17 executable/unit tests plus one compiling documentation example.
The parameterized dispatch test makes **71 calls**: every one of the 60 canonical
GET tools and all 11 aliases. It checks their actual request paths, all declared
query parameters, escaping, GET-only behavior, response envelopes and absence of
upstream authentication against a local mock. This verifies the client's wiring;
it does not establish that Equidia implements every filter correctly.

## Offline behavior checks

| Area | Verified |
| --- | --- |
| Catalogue | All 60 allowed routes and 11 aliases dispatch; excluded routes and all writes are absent; filtering and offline discovery |
| Inputs | Required parameters, date validity, pagination limits, enums, unknown fields and path traversal rejection |
| Requests | Exact route/query dispatch, Unicode and reserved-character encoding, array serialization and no upstream Authorization header |
| Failures | Connection refusal, HTTP 403/404/429/503, retries, redirects, request deadline, invalid JSON, unexpected HTML, content-length and streamed size limits |
| Responses | JSON arrays/objects, missing content type with JSON and empty 204 responses |
| CLI | Named flags, generic JSON calls, JSON from stdin, output files, schema inspection, live doctor success/failure against mocks |
| Configuration | Dotenv/environment/CLI precedence, invalid bounds, origin validation, invalid TLS combinations, sanitized dotenv errors and JSON logging |
| MCP | Stdio initialization/list/call/error flow; HTTP sessions and calls; bearer protection, host/origin checks, rate limiting and graceful shutdown |
| Metrics | Tool success/error counts, duration histogram and upstream request count after real MCP calls |
| TLS | Trusted/untrusted certificates, mTLS requirements, certificate rotation on SIGHUP and retention of the last valid pair after a failed reload |

The test harness forwards `LLVM_PROFILE_FILE` to child processes. HTTP and TLS
tests send SIGTERM and wait for exit, allowing LLVM profiles to flush. Forced
termination previously discarded those measurements.

## Live API results

All **60 routes** were exercised through the actual CLI without credentials on
2026-11-11. The latest [endpoint matrix](endpoint-validation.md) records each
route's result; its [JSON version](endpoint-validation.json) can be processed by
tools. No response bodies or user identifiers are retained in these reports.

| Result | Routes | Meaning |
| --- | ---: | --- |
| Successful with nonempty data | 57 | The representative request succeeded and decoded |
| Successful with empty data | 3 | Access and decoding worked; useful content and filter correctness remain unproven |

The empty results were broadcast tickets, pre-runners and stable noise.

These checks do not establish the accuracy of race results, odds, predictions or
statistics. Each route uses one representative request; pagination across all
pages, every filter combination, media playback and performance under load have
not been qualified. Transient upstream outages can change these results.

## Remaining code coverage gaps

53 source lines remain uncovered. They include defensive paths for invalid
built-in catalogue data, mutex/semaphore failure fallbacks, socket/signal setup
failures, interrupted response streams, HTTP-date Retry-After parsing, uncommon
query-array styles not used by the current contract, and some stdin/filesystem
failure paths. Concurrent queue saturation has not been stress-tested. Line
coverage does not imply every conditional branch has been exercised.

The compact [coverage summary](test-coverage.json) gives per-module counts.
The local HTML report is generated at `target/coverage/html/index.html` and marks
the uncovered source regions. CI has a separate coverage job with a **90% line
coverage floor** and uploads the full report. This workflow is configured; a
hosted run is not part of the local verification.

## Reproduce

```sh
cargo test --locked --all-features
mkdir -p target/coverage
cargo llvm-cov test --locked --all-features --ignore-filename-regex '/tests/' \
  --json --output-path target/coverage/coverage.json --fail-under-lines 90
cargo llvm-cov report --ignore-filename-regex '/tests/' \
  --html --output-dir target/coverage
python3 scripts/coverage_report.py

# Calls Equidia; fixtures are discovered from public collections.
cargo build --locked
python3 scripts/live_matrix.py --date 2026-11-11
```

`live_matrix.py` exits nonzero for request/application errors. Resource
unavailability and missing fixtures are separately labeled; use `--strict` to
also fail on those. A zero exit without `--strict`
therefore does not mean every route returned useful data.
