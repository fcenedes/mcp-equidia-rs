//! Exercise real TLS handshakes and diagnostics through the installed CLI surface.

mod support;

use std::{path::Path, process::Stdio, time::Duration};

use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};

fn binary(directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mcp-equidia-rs"));
    command
        .env_clear()
        .current_dir(directory)
        .arg("--no-env-file")
        .kill_on_drop(true);
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    command
}

struct Certificates {
    ca: String,
    server: String,
    server_key: String,
    client: String,
    client_key: String,
}

fn certificates() -> Certificates {
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca_key = KeyPair::generate().unwrap();
    let ca = ca_params.self_signed(&ca_key).unwrap().pem();
    let issuer = Issuer::new(ca_params, ca_key);
    let mut params = CertificateParams::new(vec!["127.0.0.1".into(), "localhost".into()]).unwrap();
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let server_key = KeyPair::generate().unwrap();
    let server = params.signed_by(&server_key, &issuer).unwrap().pem();
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    let client_key = KeyPair::generate().unwrap();
    let client = params.signed_by(&client_key, &issuer).unwrap().pem();
    Certificates {
        ca,
        server,
        server_key: server_key.serialize_pem(),
        client,
        client_key: client_key.serialize_pem(),
    }
}

fn write_certificates(directory: &Path, certs: &Certificates) {
    for (name, contents) in [
        ("server.pem", &certs.server),
        ("server-key.pem", &certs.server_key),
        ("ca.pem", &certs.ca),
    ] {
        std::fs::write(directory.join(name), contents).unwrap();
    }
}

fn client(certs: &Certificates, identity: bool) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .pool_max_idle_per_host(0)
        .add_root_certificate(reqwest::Certificate::from_pem(certs.ca.as_bytes()).unwrap());
    if identity {
        builder = builder.identity(
            reqwest::Identity::from_pem(format!("{}{}", certs.client, certs.client_key).as_bytes())
                .unwrap(),
        );
    }
    builder.build().unwrap()
}

async fn server(
    directory: &Path,
    mtls: bool,
) -> (
    Child,
    String,
    tokio::io::Lines<BufReader<tokio::process::ChildStderr>>,
) {
    let mut command = binary(directory);
    command
        .env("MCP_HTTP_TOKEN", "fixture-http-token")
        .env("RUST_LOG", "info")
        .args([
            "serve",
            "http",
            "--listen",
            "127.0.0.1:0",
            "--tls-cert",
            "server.pem",
            "--tls-key",
            "server-key.pem",
            "--metrics",
            "true",
            "--rate-limit",
            "2",
        ]);
    if mtls {
        command.args(["--tls-client-ca", "ca.pem"]);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let url = line
        .strip_prefix("MCP Streamable HTTP listening on ")
        .unwrap()
        .to_owned();
    assert!(url.starts_with("https://"));
    (child, url, lines)
}

#[tokio::test]
async fn tls_serves_mcp_and_metrics_and_rejects_untrusted_certificates() {
    let directory = tempfile::tempdir().unwrap();
    let certs = certificates();
    write_certificates(directory.path(), &certs);
    let doctor = binary(directory.path())
        .args([
            "doctor",
            "--tls-cert",
            "server.pem",
            "--tls-key",
            "server-key.pem",
        ])
        .output()
        .await
        .unwrap();
    assert!(doctor.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&doctor.stdout).unwrap()["tls"],
        "valid"
    );
    let (mut child, url, _logs) = server(directory.path(), false).await;
    let http = client(&certs, false);
    let health = url.replace("/mcp", "/health");
    assert_eq!(
        http.get(&health)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "ok"
    );
    assert!(
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(&health)
            .send()
            .await
            .is_err()
    );
    let metrics = url.replace("/mcp", "/metrics");
    assert_eq!(http.get(&metrics).send().await.unwrap().status(), 401);
    assert_eq!(
        http.get(&metrics)
            .bearer_auth("fixture-http-token")
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"tls-test","version":"1"}}});
    let response = http
        .post(&url)
        .bearer_auth("fixture-http-token")
        .header("accept", "application/json, text/event-stream")
        .json(&initialize)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(response.headers().contains_key("mcp-session-id"));
    support::stop(&mut child).await;
}

#[cfg(unix)]
#[tokio::test]
async fn sighup_rotates_certificates_and_a_failed_reload_retains_the_last_valid_pair() {
    let directory = tempfile::tempdir().unwrap();
    let original = certificates();
    write_certificates(directory.path(), &original);
    let (mut child, url, mut logs) = server(directory.path(), false).await;
    let health = url.replace("/mcp", "/health");
    assert_eq!(
        client(&original, false)
            .get(&health)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let replacement = certificates();
    write_certificates(directory.path(), &replacement);
    for expected in ["TLS configuration reloaded", "TLS reload failed"] {
        if expected.contains("failed") {
            std::fs::write(
                directory.path().join("server-key.pem"),
                "invalid private key",
            )
            .unwrap();
        }
        assert!(
            Command::new("kill")
                .args(["-HUP", &child.id().unwrap().to_string()])
                .status()
                .await
                .unwrap()
                .success()
        );
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let line = logs
                    .next_line()
                    .await
                    .unwrap()
                    .expect("server log stream ended");
                if line.contains(expected) {
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(
            client(&replacement, false)
                .get(&health)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert!(client(&original, false).get(&health).send().await.is_err());
    }
    support::stop(&mut child).await;
}

#[tokio::test]
async fn mtls_requires_a_client_certificate_and_keeps_bearer_authentication() {
    let directory = tempfile::tempdir().unwrap();
    let certs = certificates();
    write_certificates(directory.path(), &certs);
    let (mut child, url, _logs) = server(directory.path(), true).await;
    let health = url.replace("/mcp", "/health");
    assert!(client(&certs, false).get(&health).send().await.is_err());
    let wrong = certificates();
    let wrong_identity = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .add_root_certificate(reqwest::Certificate::from_pem(certs.ca.as_bytes()).unwrap())
        .identity(
            reqwest::Identity::from_pem(format!("{}{}", wrong.client, wrong.client_key).as_bytes())
                .unwrap(),
        )
        .build()
        .unwrap();
    assert!(wrong_identity.get(&health).send().await.is_err());
    let http = client(&certs, true);
    assert_eq!(http.get(&health).send().await.unwrap().status(), 200);
    assert_eq!(
        http.get(url.replace("/mcp", "/metrics"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    support::stop(&mut child).await;
}
