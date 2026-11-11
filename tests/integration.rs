mod support;

use std::{
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

fn binary() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mcp-equidia-rs"));
    command.env_clear();
    // Preserve OS certificate/proxy configuration, but isolate application config.
    for name in [
        "PATH",
        "HOME",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "NO_PROXY",
        "LLVM_PROFILE_FILE",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.arg("--no-env-file").kill_on_drop(true);
    command
}

#[derive(Clone, Default)]
struct Mock {
    requests: Arc<Mutex<Vec<(String, String, String)>>>,
    attempts: Arc<AtomicUsize>,
    mode: &'static str,
}

async fn handler(State(state): State<Mock>, request: Request) -> Response {
    let auth = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_owned();
    state.requests.lock().unwrap().push((
        request.method().to_string(),
        request.uri().to_string(),
        auth,
    ));
    let attempt = state.attempts.fetch_add(1, Ordering::Relaxed);
    match state.mode {
        "retry" if attempt == 0 => (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "0")],
            "busy",
        )
            .into_response(),
        "redirect" => (
            StatusCode::FOUND,
            [(header::LOCATION, "https://example.org/secret")],
        )
            .into_response(),
        "forbidden" => (StatusCode::FORBIDDEN, "secret-token-must-not-leak").into_response(),
        "large" => (
            [(header::CONTENT_TYPE, "application/json")],
            "x".repeat(5000),
        )
            .into_response(),
        "chunked" => Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from_stream(futures_util::stream::iter([Ok::<
                _,
                std::io::Error,
            >(
                vec![b'x'; 5000],
            )])))
            .unwrap(),
        "invalid-json" => ([(header::CONTENT_TYPE, "application/json")], "invalid").into_response(),
        "html" => (
            [(header::CONTENT_TYPE, "text/html")],
            "<html>unexpected</html>",
        )
            .into_response(),
        "empty" => StatusCode::NO_CONTENT.into_response(),
        "server-error" => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        "not-found" => StatusCode::NOT_FOUND.into_response(),
        "no-content-type" => Response::new(Body::from("{\"value\":42}")),
        "slow" => {
            tokio::time::sleep(Duration::from_secs(3)).await;
            axum::Json(json!([])).into_response()
        }
        _ => axum::Json(json!([{"uuid":"race-123","name":"PRIX TEST"}])).into_response(),
    }
}

struct Upstream {
    url: String,
    state: Mock,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Upstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn upstream(mode: &'static str) -> Upstream {
    let state = Mock {
        mode,
        ..Default::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .fallback(any(handler))
        .with_state(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Upstream { url, state, task }
}

#[tokio::test]
async fn every_allowed_get_has_a_tool_and_restricted_operations_are_absent() {
    let output = binary().arg("tools").output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tools: Value = serde_json::from_slice(&output.stdout).unwrap();
    let tools = tools.as_array().unwrap();
    assert_eq!(tools.len(), 72); // 60 public reads + 11 aliases + catalogue.
    let spec: Value = serde_json::from_str(include_str!("../api/openapi.json")).unwrap();
    let policy: Value = serde_json::from_str(include_str!("../api/access-policy.json")).unwrap();
    for (path, item) in spec["paths"].as_object().unwrap() {
        if item.get("get").is_some()
            && policy["allowed_paths"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p == path)
        {
            assert!(
                tools.iter().any(|t| t["description"]
                    .as_str()
                    .unwrap()
                    .starts_with(&format!("GET {path}."))),
                "missing {path}"
            );
        }
    }
    for tool in tools {
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["annotations"]["destructiveHint"], false);
        assert!(!tool["name"].as_str().unwrap().contains("_post_"));
        assert!(!tool["name"].as_str().unwrap().contains("_delete_"));
        assert_ne!(tool["name"], "equidia_get_v2_notifications");
        assert_ne!(tool["name"], "equidia_get_v2_rundowns_pdf_by_uuid");
        assert_ne!(tool["name"], "equidia_get_v2_notifications_users_by_guid");
        assert_ne!(
            tool["name"],
            "equidia_get_v2_braze_subscription_user_status"
        );
    }
    let write = binary()
        .args(["call", "equidia_post_v2_tickets", "--args", "{}"])
        .output()
        .await
        .unwrap();
    assert!(!write.status.success());
    assert!(String::from_utf8_lossy(&write.stderr).contains("unknown or disabled"));
    for name in [
        "equidia_get_v2_rundowns_pdf_by_uuid",
        "equidia_get_v2_notifications",
        "equidia_get_v2_notifications_users_by_guid",
        "equidia_get_v2_braze_subscription_user_status",
    ] {
        let output = binary()
            .args(["call", name, "--args", "{}"])
            .output()
            .await
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("unknown or disabled"));
    }
}

#[tokio::test]
async fn cli_alias_json_and_query_serialization_use_only_get() {
    let upstream = upstream("ok").await;
    let output = binary()
        .env("EQUIDIA_BASE_URL", &upstream.url)
        .env("EQUIDIA_API_TOKEN", "test-token")
        .args([
            "races",
            "--date",
            "2026-11-11",
            "--is-quinte",
            "--family-discipline",
            "FLAT",
            "--page",
            "2",
            "--items-per-page",
            "5",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"][0]["uuid"], "race-123");
    let request = upstream.state.requests.lock().unwrap()[0].clone();
    assert_eq!(request.0, "GET");
    assert!(request.1.contains("is-quinte=true"));
    assert!(request.1.contains("items-per-page=5"));
    // Upstream authorization is intentionally unsupported, even if an old
    // environment variable remains in a user's shell.
    assert!(request.2.is_empty());
    let output = binary()
        .env("EQUIDIA_BASE_URL", &upstream.url)
        .args([
            "call",
            "equidia_get_v2_tags",
            "--args",
            r#"{"uuids":["a","b"]}"#,
        ])
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    assert!(
        upstream.state.requests.lock().unwrap()[1]
            .1
            .contains("uuids=a%2Cb")
    );
    let output = binary()
        .env("EQUIDIA_BASE_URL", &upstream.url)
        .args(["get-v2-runners-by-uuid", "--uuid", "runner-123"])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        upstream.state.requests.lock().unwrap()[2]
            .1
            .starts_with("/api/v2/runners/runner-123")
    );
}

#[tokio::test]
async fn invalid_arguments_never_reach_upstream() {
    let upstream = upstream("ok").await;
    for (tool, input) in [
        ("races", r#"{"date":"2026-02-30"}"#),
        ("races", r#"{"page":0}"#),
        ("races", r#"{"items-per-page":0}"#),
        ("races", r#"{"family-discipline":"BAD"}"#),
        ("races", r#"{"unknown":true}"#),
        ("races", r#"{"body":{}}"#),
        ("get-v2-runners-by-uuid", r#"{"uuid":"../users"}"#),
        ("get-v2-runners-by-uuid", r#"{"uuid":"%2fusers"}"#),
        ("get-v2-runners-by-uuid", "{}"),
    ] {
        let output = binary()
            .env("EQUIDIA_BASE_URL", &upstream.url)
            .args(["call", tool, "--args", input])
            .output()
            .await
            .unwrap();
        assert!(
            !output.status.success(),
            "unexpected success for {tool} {input}"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("invalid arguments"));
    }
    assert!(upstream.state.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn retries_redirects_error_redaction_and_response_limits_work() {
    for (mode, success, expected_attempts, expected_error) in [
        ("retry", true, 2, ""),
        ("redirect", false, 1, "redirect refused"),
        ("forbidden", false, 1, "anonymous access was denied"),
        ("large", false, 1, "byte limit"),
        ("chunked", false, 1, "byte limit"),
        ("invalid-json", false, 1, "valid JSON"),
    ] {
        let upstream = upstream(mode).await;
        let output = binary()
            .env("EQUIDIA_BASE_URL", &upstream.url)
            .env("EQUIDIA_API_TOKEN", "secret-token-must-not-leak")
            .args(["--max-response-bytes", "1024", "races"])
            .output()
            .await
            .unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            upstream.state.attempts.load(Ordering::Relaxed),
            expected_attempts
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected_error));
        assert!(!stderr.contains("secret-token-must-not-leak"));
    }
}

#[tokio::test]
async fn total_deadline_is_enforced() {
    let slow = upstream("slow").await;
    let output = binary()
        .env("EQUIDIA_BASE_URL", &slow.url)
        .args(["--tool-timeout", "1", "races"])
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("deadline exceeded"));
}

#[tokio::test]
async fn dotenv_precedence_filters_and_offline_doctor_work() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        "EQUIDIA_ENABLED_TOOLS=equidia_horse\nEQUIDIA_TIMEOUT=5\n",
    )
    .unwrap();
    // A fresh command avoids --no-env-file, which intentionally conflicts.
    let output = Command::new(env!("CARGO_BIN_EXE_mcp-equidia-rs"))
        .env_clear()
        .envs(std::env::var_os("LLVM_PROFILE_FILE").map(|v| ("LLVM_PROFILE_FILE", v)))
        .args(["--env-file", file.path().to_str().unwrap(), "tools"])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tools: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tools.as_array().unwrap().len(), 1);
    assert_eq!(tools[0]["name"], "equidia_horse");
    let output = Command::new(env!("CARGO_BIN_EXE_mcp-equidia-rs"))
        .env_clear()
        .envs(std::env::var_os("LLVM_PROFILE_FILE").map(|v| ("LLVM_PROFILE_FILE", v)))
        .env("EQUIDIA_ENABLED_TOOLS", "equidia_races")
        .args([
            "--env-file",
            file.path().to_str().unwrap(),
            "--enabled-tools",
            "equidia_meetings",
            "tools",
        ])
        .output()
        .await
        .unwrap();
    let tools: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tools[0]["name"], "equidia_meetings");
    let output = binary()
        .args(["--enabled-tools", "unknown", "doctor"])
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    let output = binary().arg("doctor").output().await.unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["readOnly"], true);
    let output = binary()
        .args([
            "--enabled-tools",
            "equidia_races,equidia_api_catalog",
            "api-catalog",
            "--query",
            "races",
        ])
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let catalogue: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(catalogue["operations"].as_array().unwrap().len(), 1);
    assert_eq!(catalogue["operations"][0]["tool"], "equidia_races");
    let output = binary()
        .args([
            "doctor",
            "--tls-cert",
            "missing.pem",
            "--tls-key",
            "missing.key",
        ])
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ok"], false);
}

async fn rpc_write(stdin: &mut tokio::process::ChildStdin, value: Value) {
    stdin
        .write_all(format!("{value}\n").as_bytes())
        .await
        .unwrap();
}

#[tokio::test]
async fn stdio_initializes_lists_calls_and_returns_validation_errors() {
    let upstream = upstream("ok").await;
    let mut child = binary()
        .env("EQUIDIA_BASE_URL", &upstream.url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    rpc_write(&mut stdin, initialize()).await;
    let init: Value = serde_json::from_str(
        &tokio::time::timeout(Duration::from_secs(10), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "mcp-equidia-rs");
    rpc_write(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    for (id, method, params) in [
        (2, "tools/list", json!({})),
        (
            3,
            "tools/call",
            json!({"name":"equidia_races","arguments":{"date":"2026-11-11"}}),
        ),
        (
            4,
            "tools/call",
            json!({"name":"equidia_races","arguments":{"page":0}}),
        ),
        (
            5,
            "tools/call",
            json!({"name":"equidia_delete_v2_segments_by_id","arguments":{"id":"1"}}),
        ),
    ] {
        rpc_write(
            &mut stdin,
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        )
        .await;
        let response = loop {
            let line = tokio::time::timeout(Duration::from_secs(5), lines.next_line())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let value: Value = serde_json::from_str(&line).unwrap();
            if value["id"] == id {
                break value;
            }
        };
        match id {
            2 => assert_eq!(response["result"]["tools"].as_array().unwrap().len(), 72),
            3 => assert_eq!(
                response["result"]["structuredContent"]["data"][0]["uuid"],
                "race-123"
            ),
            4 => assert_eq!(response["result"]["isError"], true),
            _ => assert_eq!(response["error"]["code"], -32602),
        }
    }
    drop(stdin);
    assert!(
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

fn initialize() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"integration-test","version":"1"}}})
}

#[tokio::test]
async fn http_rate_limit_counts_authenticated_requests_only() {
    let mut child = binary()
        .env("MCP_HTTP_TOKEN", "local-token")
        .args([
            "serve",
            "http",
            "--listen",
            "127.0.0.1:0",
            "--rate-limit",
            "1",
        ])
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), stderr.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let url = line
        .strip_prefix("MCP Streamable HTTP listening on ")
        .unwrap();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    assert_eq!(
        http.post(url)
            .json(&initialize())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = http
        .post(url)
        .bearer_auth("local-token")
        .header("accept", "application/json, text/event-stream")
        .json(&initialize())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response = http
        .post(url)
        .bearer_auth("local-token")
        .header("accept", "application/json, text/event-stream")
        .json(&initialize())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 429);
    assert!(response.headers().contains_key("retry-after"));
    support::stop(&mut child).await;
}

#[tokio::test]
async fn http_auth_origin_host_health_metrics_and_mcp_session_work() {
    let mock = upstream("ok").await;
    let mut child = binary()
        .env("EQUIDIA_BASE_URL", &mock.url)
        .env("MCP_HTTP_TOKEN", "local-token")
        .args([
            "serve",
            "http",
            "--listen",
            "127.0.0.1:0",
            "--metrics",
            "true",
            "--rate-limit",
            "0",
        ])
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), stderr.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let url = line
        .strip_prefix("MCP Streamable HTTP listening on ")
        .unwrap();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    assert_eq!(
        http.get(url.replace("/mcp", "/health"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        http.post(url)
            .json(&initialize())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        http.get(url.replace("/mcp", "/metrics"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let metrics = http
        .get(url.replace("/mcp", "/metrics"))
        .bearer_auth("local-token")
        .send()
        .await
        .unwrap();
    assert!(
        metrics
            .text()
            .await
            .unwrap()
            .contains("mcp_equidia_tool_calls_total")
    );
    for (key, value) in [("origin", "https://evil.example"), ("host", "evil.example")] {
        let response = http
            .post(url)
            .bearer_auth("local-token")
            .header("accept", "application/json, text/event-stream")
            .header(key, value)
            .json(&initialize())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403, "{key}");
    }
    let response = http
        .post(url)
        .bearer_auth("local-token")
        .header("accept", "application/json, text/event-stream")
        .json(&initialize())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let session = response
        .headers()
        .get("mcp-session-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let initialized = response.text().await.unwrap();
    assert!(initialized.contains("mcp-equidia-rs"));
    let response = http
        .post(url)
        .bearer_auth("local-token")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session)
        .header("mcp-protocol-version", "2025-11-25")
        .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    let response = http.post(url).bearer_auth("local-token").header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session).header("mcp-protocol-version", "2025-11-25")
        .json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"equidia_api_catalog","arguments":{"query":"runners"}}})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        response
            .text()
            .await
            .unwrap()
            .contains("equidia_get_mobile_runners")
    );
    for (id, args) in [(3, json!({})), (4, json!({"page":0}))] {
        let response = http.post(url).bearer_auth("local-token").header("accept", "application/json, text/event-stream")
            .header("mcp-session-id", &session).header("mcp-protocol-version", "2025-11-25")
            .json(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"equidia_races","arguments":args}}))
            .send().await.unwrap();
        assert_eq!(response.status(), 200);
        let body = response.text().await.unwrap();
        assert!(body.contains(if id == 3 {
            "race-123"
        } else {
            "invalid arguments"
        }));
    }
    let metrics = http
        .get(url.replace("/mcp", "/metrics"))
        .bearer_auth("local-token")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(metrics.contains(
        "mcp_equidia_tool_calls_total{tool=\"equidia_api_catalog\",outcome=\"success\"} 1"
    ));
    assert!(metrics.contains("mcp_equidia_tool_duration_seconds_bucket{"));
    assert!(
        metrics
            .contains("mcp_equidia_tool_calls_total{tool=\"equidia_races\",outcome=\"error\"} 1")
    );
    assert!(
        metrics.contains("mcp_equidia_upstream_requests_total{api=\"equidia\",status=\"200\"} 1")
    );
    support::stop(&mut child).await;
    let output = binary()
        .args(["serve", "http", "--listen", "0.0.0.0:0"])
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MCP_HTTP_TOKEN"));
}

#[tokio::test]
async fn every_route_and_alias_dispatches_its_declared_path_and_all_query_parameters() {
    let mock = upstream("ok").await;
    let manifest: Value = serde_json::from_str(include_str!("../api/catalog.json")).unwrap();
    let mut cases: Vec<(String, &Value)> = manifest["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|op| (op["name"].as_str().unwrap().to_owned(), op))
        .collect();
    for (alias, target) in manifest["aliases"].as_object().unwrap() {
        let operation = manifest["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["name"] == *target)
            .unwrap();
        cases.push((alias.clone(), operation));
    }
    for (name, op) in &cases {
        let mut args = serde_json::Map::new();
        let mut expected_path = op["path"].as_str().unwrap().to_owned();
        for parameter in op["parameters"].as_array().unwrap() {
            let key = parameter["name"].as_str().unwrap();
            let schema = &op["input_schema"]["properties"][key];
            let value = if parameter["location"] == "path" {
                json!(if key == "date" {
                    "2026-11-11"
                } else {
                    "fixture-123"
                })
            } else if let Some(values) = schema["enum"].as_array() {
                values[0].clone()
            } else {
                match schema["type"].as_str().unwrap() {
                    "integer" => json!(2),
                    "boolean" => json!(true),
                    "array" => json!(["alpha & beta", "é"]),
                    "string" if schema["format"] == "date" => json!("2026-11-11"),
                    "string" => json!("sample & café"),
                    kind => panic!("missing fixture for {name}: {key} ({kind})"),
                }
            };
            if parameter["location"] == "path" {
                expected_path =
                    expected_path.replace(&format!("{{{key}}}"), value.as_str().unwrap());
            }
            args.insert(key.into(), value);
        }
        let output = binary()
            .env("EQUIDIA_BASE_URL", &mock.url)
            .args([
                "call",
                name,
                "--args",
                &Value::Object(args.clone()).to_string(),
            ])
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["data"][0]["uuid"], "race-123", "{name}");
        let (method, uri, auth) = mock.state.requests.lock().unwrap().last().unwrap().clone();
        assert_eq!(method, "GET", "{name}");
        assert!(auth.is_empty(), "{name}");
        let url = reqwest::Url::parse(&format!("{}{}", mock.url, uri)).unwrap();
        assert_eq!(url.path(), expected_path, "{name}");
        let query: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
        let expected_query_count = op["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["location"] == "query")
            .count();
        assert_eq!(query.len(), expected_query_count, "{name}");
        for parameter in op["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["location"] == "query")
        {
            let key = parameter["name"].as_str().unwrap();
            let expected = match &args[key] {
                Value::String(s) => s.clone(),
                Value::Array(a) => a
                    .iter()
                    .map(|s| s.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(","),
                v => v.to_string(),
            };
            assert_eq!(query[key], expected, "{name}: {key}");
        }
    }
    assert_eq!(cases.len(), 71);
    assert_eq!(mock.state.attempts.load(Ordering::Relaxed), 71);
}

#[tokio::test]
async fn stdin_output_schema_and_live_doctor_paths_work() {
    let mock = upstream("ok").await;
    let output_file = tempfile::NamedTempFile::new().unwrap();
    let mut child = binary()
        .env("EQUIDIA_BASE_URL", &mock.url)
        .args([
            "call",
            "races",
            "--args",
            "-",
            "--output",
            output_file.path().to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"date":"2026-11-11"}"#)
        .await
        .unwrap();
    let output = child.wait_with_output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&std::fs::read(output_file.path()).unwrap()).unwrap();
    assert_eq!(value["data"][0]["uuid"], "race-123");
    let output = binary().args(["schema", "races"]).output().await.unwrap();
    let schema: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(schema["properties"]["date"]["format"], "date");
    for (mode, success) in [("ok", true), ("forbidden", false)] {
        let mock = upstream(mode).await;
        let output = binary()
            .env("EQUIDIA_BASE_URL", &mock.url)
            .args(["--log-format", "json", "doctor", "--live"])
            .output()
            .await
            .unwrap();
        assert_eq!(output.status.success(), success);
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["ok"], success);
        assert_eq!(report["live"], true);
    }
    let output = binary()
        .args(["--enabled-tools", "equidia_races", "doctor", "--live"])
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires equidia_meetings"));
}

#[tokio::test]
async fn network_failures_and_empty_or_unexpected_responses_are_classified() {
    for (mode, success, expected) in [
        ("empty", true, Value::Null),
        ("no-content-type", true, json!({"value":42})),
        ("html", false, Value::Null),
        ("server-error", false, Value::Null),
        ("not-found", false, Value::Null),
    ] {
        let mock = upstream(mode).await;
        let output = binary()
            .env("EQUIDIA_BASE_URL", &mock.url)
            .args(["--max-retries", "1", "races"])
            .output()
            .await
            .unwrap();
        assert_eq!(output.status.success(), success, "{mode}");
        if success {
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stdout).unwrap()["data"],
                expected
            );
        }
        assert_eq!(
            mock.state.attempts.load(Ordering::Relaxed),
            if mode == "server-error" { 2 } else { 1 }
        );
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let output = binary()
        .env("EQUIDIA_BASE_URL", format!("http://{address}"))
        .args(["--max-retries", "1", "races"])
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("network request failed"));
}

#[tokio::test]
async fn invalid_configuration_and_dotenv_secrets_are_reported_safely() {
    for (key, value) in [
        ("EQUIDIA_TIMEOUT", "0"),
        ("EQUIDIA_MAX_RETRIES", "6"),
        ("EQUIDIA_TOOL_TIMEOUT", "0"),
        ("EQUIDIA_MAX_RESPONSE_BYTES", "0"),
        ("EQUIDIA_CONCURRENCY", "0"),
        ("MCP_HTTP_RATE_LIMIT", "-1"),
        ("MCP_METRICS_ENABLED", "yes"),
        ("MCP_LOG_FORMAT", "other"),
        ("MCP_HTTP_LISTEN", "not-an-address"),
        ("MCP_HTTP_TOKEN", " \n"),
        ("MCP_TLS_CERT", "cert.pem"),
        ("MCP_TLS_CLIENT_CA", "ca.pem"),
    ] {
        let output = binary()
            .env(key, value)
            .arg("doctor")
            .output()
            .await
            .unwrap();
        assert!(!output.status.success(), "accepted {key}");
    }
    for origin in [
        "bad-url",
        "http://example.org",
        "https://user:secret@example.org",
        "https://example.org/api",
        "https://example.org/?token=secret",
    ] {
        let output = binary()
            .env("EQUIDIA_BASE_URL", origin)
            .arg("doctor")
            .output()
            .await
            .unwrap();
        assert!(!output.status.success(), "accepted {origin}");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("secret"));
    }
    let output = binary()
        .env("EQUIDIA_BASE_URL", "http://[::1]")
        .arg("doctor")
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "MCP_HTTP_TOKEN='secret-do-not-print\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mcp-equidia-rs"))
        .env_clear()
        .envs(std::env::var_os("LLVM_PROFILE_FILE").map(|v| ("LLVM_PROFILE_FILE", v)))
        .args(["--env-file", file.path().to_str().unwrap(), "doctor"])
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot parse dotenv"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("secret-do-not-print"));
}
