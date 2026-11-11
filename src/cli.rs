use std::io::{self, Read, Write};

use anyhow::Context;
use clap::{Arg, ArgAction, Command};
use serde_json::{Value, json};
use tracing_subscriber::EnvFilter;

use crate::{
    catalog::Catalog, client::EquidiaClient, server, settings::Settings, tools::EngineState,
};

pub(crate) async fn run() -> anyhow::Result<()> {
    let all = Catalog::new(None)?;
    let args = command(&all).get_matches();
    let settings = Settings::load(&args)?;
    let logging = tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(
            EnvFilter::try_new(&settings.log_filter).unwrap_or_else(|_| EnvFilter::new("warn")),
        );
    if settings.log_format == "json" {
        logging.json().try_init().ok();
    } else {
        logging.try_init().ok();
    }
    let catalog = Catalog::new(Some(&settings.enabled_tools))?;
    let client = EquidiaClient::new(&settings)?;
    let engine = EngineState { catalog, client };
    match args.subcommand() {
        None => server::serve_stdio(engine).await,
        Some(("serve", args)) => match args.subcommand() {
            Some(("http", _)) => server::serve_http(engine, settings.http).await,
            _ => server::serve_stdio(engine).await,
        },
        Some(("tools", _)) => print_json(&json!(
            engine
                .catalog
                .iter()
                .map(|d| d.tool.clone())
                .collect::<Vec<_>>()
        )),
        Some(("schema", args)) => {
            let tool = normalize(args.get_one::<String>("tool").context("missing tool")?);
            print_json(&json!(engine.catalog.get(&tool)?.tool.input_schema))
        }
        Some(("call", args)) => {
            let tool = normalize(args.get_one::<String>("tool").context("missing tool")?);
            let input = read_args(args.get_one::<String>("args").context("missing args")?)?;
            let output = engine.call(&tool, input).await?;
            write_output(&output, args.get_one::<String>("output"))
        }
        Some(("doctor", args)) => {
            let mut report = json!({"ok":true,"version":env!("CARGO_PKG_VERSION"),"readOnly":true,"upstreamAuthentication":"none","enabledTools":engine.catalog.iter().count(),"httpTokenConfigured":settings.http.token.is_some(),"live":false});
            if !settings.http.listen.ip().is_loopback() && settings.http.token.is_none() {
                report["ok"] = json!(false);
                report["httpError"] =
                    json!("MCP_HTTP_TOKEN is required for non-loopback listeners");
            }
            if let Some(tls) = &settings.http.tls {
                match tls.load().await {
                    Ok(_) => {
                        report["tls"] = json!("valid");
                    }
                    Err(error) => {
                        report["ok"] = json!(false);
                        report["tlsError"] = json!(error.to_string());
                    }
                }
            }
            if args.get_flag("live") {
                report["live"] = json!(true);
                let op = engine.catalog.iter().find(|d| d.operation.as_ref().is_some_and(|op| op.path == "/api/mobile/meetings"))
                    .context("doctor --live requires equidia_meetings or equidia_get_mobile_meetings to be enabled")?;
                match engine
                    .call(&op.tool.name, json!({"items-per-page":1}))
                    .await
                {
                    Ok(output) => {
                        report["probe"] = output["meta"].clone();
                    }
                    Err(error) => {
                        report["ok"] = json!(false);
                        report["error"] = json!(error.to_string());
                    }
                }
            }
            print_json(&report)?;
            if report["ok"] == false {
                anyhow::bail!("doctor found a failed check");
            }
            Ok(())
        }
        Some((name, args)) => {
            let tool = normalize(name);
            let definition = engine.catalog.get(&tool)?;
            let mut input = serde_json::Map::new();
            if let Some(properties) = definition.tool.input_schema["properties"].as_object() {
                for (key, schema) in properties {
                    if let Some(value) = args.get_one::<String>(key) {
                        let value = if schema["type"] == "string" {
                            Value::String(value.clone())
                        } else {
                            serde_json::from_str(value)
                                .with_context(|| format!("--{} requires a JSON value", flag(key)))?
                        };
                        input.insert(key.clone(), value);
                    }
                }
            }
            let output = engine.call(&tool, Value::Object(input)).await?;
            write_output(&output, args.get_one::<String>("output"))
        }
    }
}

pub(crate) fn command(catalog: &Catalog) -> Command {
    let mut command = Command::new("mcp-equidia-rs").version(env!("CARGO_PKG_VERSION"))
        .about("Read-only Equidia API CLI and MCP server")
        .after_help("No command starts MCP stdio. Dates use YYYY-MM-DD. Arrays use JSON. Example: mcp-equidia-rs races --date 2026-11-11 --items-per-page 5")
        .arg(Arg::new("env-file").long("env-file").global(true).conflicts_with("no-env-file").help("Dotenv file; defaults to optional .env or MCP_ENV_FILE"))
        .arg(Arg::new("no-env-file").long("no-env-file").global(true).action(ArgAction::SetTrue).help("Disable dotenv loading"));
    for (name, help) in [
        (
            "base-url",
            "Override EQUIDIA_BASE_URL; HTTPS origin (default https://apiv2.equidia.fr)",
        ),
        (
            "timeout",
            "Override EQUIDIA_TIMEOUT; request timeout in seconds (30)",
        ),
        (
            "max-retries",
            "Override EQUIDIA_MAX_RETRIES; transient GET retries (2)",
        ),
        (
            "tool-timeout",
            "Override EQUIDIA_TOOL_TIMEOUT; total deadline in seconds (120)",
        ),
        (
            "max-response-bytes",
            "Override EQUIDIA_MAX_RESPONSE_BYTES (8388608)",
        ),
        (
            "concurrency",
            "Override EQUIDIA_CONCURRENCY; concurrent upstream requests (4)",
        ),
        (
            "enabled-tools",
            "Override EQUIDIA_ENABLED_TOOLS; comma-separated MCP names (all)",
        ),
        ("log-format", "MCP_LOG_FORMAT: text or json (text)"),
        ("listen", "MCP_HTTP_LISTEN: IP:port (127.0.0.1:3000)"),
        (
            "allowed-hosts",
            "MCP_HTTP_ALLOWED_HOSTS: comma-separated HTTP host allowlist",
        ),
        (
            "allowed-origins",
            "MCP_HTTP_ALLOWED_ORIGINS: comma-separated browser origin allowlist",
        ),
        (
            "rate-limit",
            "MCP_HTTP_RATE_LIMIT: authenticated requests/minute, 0 disables (120)",
        ),
        (
            "metrics",
            "MCP_METRICS_ENABLED: true or false; enable protected /metrics (false)",
        ),
        ("tls-cert", "MCP_TLS_CERT: PEM server certificate"),
        ("tls-key", "MCP_TLS_KEY: PEM private key"),
        (
            "tls-client-ca",
            "MCP_TLS_CLIENT_CA: require client certificates from this PEM CA",
        ),
    ] {
        command = command.arg(Arg::new(name).long(name).global(true).help(help));
    }
    command = command
        .subcommand(
            Command::new("serve")
                .about("Start MCP transport")
                .subcommand(Command::new("stdio"))
                .subcommand(Command::new("http")),
        )
        .subcommand(Command::new("tools").about("List enabled MCP tools and schemas offline"))
        .subcommand(
            Command::new("schema")
                .about("Print a tool input schema")
                .arg(Arg::new("tool").required(true)),
        )
        .subcommand(
            Command::new("doctor")
                .about("Check configuration; --live probes meetings")
                .arg(Arg::new("live").long("live").action(ArgAction::SetTrue)),
        )
        .subcommand(
            Command::new("call")
                .about("Call any enabled tool using JSON")
                .arg(Arg::new("tool").required(true))
                .arg(Arg::new("args").long("args").required(true))
                .arg(output_arg()),
        );
    for definition in catalog.iter() {
        let tool = &definition.tool;
        let mut sub = Command::new(cli_name(&tool.name))
            .about(tool.description.as_deref().unwrap_or_default().to_owned())
            .arg(output_arg());
        if let Some(properties) = tool.input_schema["properties"].as_object() {
            for (key, schema) in properties {
                let required = tool.input_schema["required"]
                    .as_array()
                    .is_some_and(|r| r.iter().any(|r| r == key));
                let mut arg = Arg::new(key.clone())
                    .long(flag(key))
                    .required(required)
                    .value_name(if schema["type"] == "string" {
                        "TEXT"
                    } else {
                        "JSON"
                    });
                if let Some(description) = schema["description"].as_str() {
                    arg = arg.help(description.to_owned());
                }
                if schema["type"] == "boolean" {
                    arg = arg
                        .num_args(0..=1)
                        .default_missing_value("true")
                        .value_parser(["true", "false"]);
                }
                sub = sub.arg(arg);
            }
        }
        command = command.subcommand(sub);
    }
    command
}

fn output_arg() -> Arg {
    Arg::new("output")
        .long("output")
        .short('o')
        .help("Write the JSON response envelope to this file")
}
fn cli_name(name: &str) -> String {
    name.strip_prefix("equidia_")
        .unwrap_or(name)
        .replace('_', "-")
}
fn normalize(name: &str) -> String {
    if name.starts_with("equidia_") {
        name.into()
    } else {
        format!("equidia_{}", name.replace('-', "_"))
    }
}
fn flag(name: &str) -> String {
    name.replace('_', "-")
}

fn read_args(input: &str) -> anyhow::Result<Value> {
    if input != "-" {
        return serde_json::from_str(input).context("invalid JSON arguments");
    }
    let mut buffer = String::new();
    io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_string(&mut buffer)?;
    if buffer.len() > 1024 * 1024 {
        anyhow::bail!("JSON arguments exceed 1 MiB");
    }
    serde_json::from_str(&buffer).context("invalid JSON arguments on stdin")
}

fn write_output(value: &Value, path: Option<&String>) -> anyhow::Result<()> {
    if let Some(path) = path {
        std::fs::write(path, serde_json::to_vec_pretty(value)?)
            .with_context(|| format!("cannot write {path}"))?;
        Ok(())
    } else {
        print_json(value)
    }
}

fn print_json(value: &Value) -> anyhow::Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, value)?;
    writeln!(stdout)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn cli_contract_is_valid() {
        super::command(&crate::catalog::Catalog::new(None).unwrap()).debug_assert();
    }
}
