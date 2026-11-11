use std::{
    collections::HashMap, fs::File, io::ErrorKind, net::SocketAddr, path::PathBuf, str::FromStr,
};

use anyhow::{Context, bail};
use clap::ArgMatches;

use crate::{server::HttpOptions, tls::TlsFiles};

pub(crate) struct Settings {
    pub base_url: String,
    pub timeout: u64,
    pub max_retries: u32,
    pub tool_timeout: u64,
    pub max_response_bytes: usize,
    pub concurrency: usize,
    pub enabled_tools: String,
    pub log_filter: String,
    pub log_format: String,
    pub http: HttpOptions,
}

impl Settings {
    pub fn load(args: &ArgMatches) -> anyhow::Result<Self> {
        let file = dotenv_values(args)?;
        let value = |arg: &str, env: &str, default: &str| -> String {
            args.try_get_one::<String>(arg)
                .ok()
                .flatten()
                .cloned()
                .or_else(|| std::env::var(env).ok())
                .or_else(|| file.get(env).cloned())
                .unwrap_or_else(|| default.into())
        };
        let http_token = value("http-token", "MCP_HTTP_TOKEN", "");
        if !http_token.is_empty()
            && (http_token.trim().is_empty() || http_token.contains(['\r', '\n']))
        {
            bail!("MCP_HTTP_TOKEN must be a nonempty single-line token");
        }
        let cert = value("tls-cert", "MCP_TLS_CERT", "");
        let key = value("tls-key", "MCP_TLS_KEY", "");
        let ca = value("tls-client-ca", "MCP_TLS_CLIENT_CA", "");
        let tls = match (cert.is_empty(), key.is_empty(), ca.is_empty()) {
            (true, true, true) => None,
            (false, false, _) => Some(TlsFiles {
                cert: cert.into(),
                key: key.into(),
                client_ca: (!ca.is_empty()).then(|| ca.into()),
            }),
            _ => {
                bail!("TLS certificate and key must be supplied together; client CA requires both")
            }
        };
        let log_format = value("log-format", "MCP_LOG_FORMAT", "text");
        if !matches!(log_format.as_str(), "text" | "json") {
            bail!("MCP_LOG_FORMAT must be text or json");
        }
        let listen: SocketAddr = value("listen", "MCP_HTTP_LISTEN", "127.0.0.1:3000")
            .parse()
            .context("MCP_HTTP_LISTEN must be an IP:port")?;
        let metrics = value("metrics", "MCP_METRICS_ENABLED", "false")
            .parse::<bool>()
            .context("MCP_METRICS_ENABLED must be true or false")?;
        Ok(Self {
            base_url: value("base-url", "EQUIDIA_BASE_URL", "https://apiv2.equidia.fr"),
            timeout: number(
                &value("timeout", "EQUIDIA_TIMEOUT", "30"),
                "EQUIDIA_TIMEOUT",
                1,
                600,
            )?,
            max_retries: number(
                &value("max-retries", "EQUIDIA_MAX_RETRIES", "2"),
                "EQUIDIA_MAX_RETRIES",
                0,
                5,
            )?,
            tool_timeout: number(
                &value("tool-timeout", "EQUIDIA_TOOL_TIMEOUT", "120"),
                "EQUIDIA_TOOL_TIMEOUT",
                1,
                3600,
            )?,
            max_response_bytes: number(
                &value(
                    "max-response-bytes",
                    "EQUIDIA_MAX_RESPONSE_BYTES",
                    "8388608",
                ),
                "EQUIDIA_MAX_RESPONSE_BYTES",
                1,
                104857600,
            )?,
            concurrency: number(
                &value("concurrency", "EQUIDIA_CONCURRENCY", "4"),
                "EQUIDIA_CONCURRENCY",
                1,
                64,
            )?,
            enabled_tools: value("enabled-tools", "EQUIDIA_ENABLED_TOOLS", "*"),
            log_filter: value("log-filter", "RUST_LOG", "warn"),
            log_format,
            http: HttpOptions {
                listen,
                token: (!http_token.is_empty()).then_some(http_token),
                allowed_hosts: comma_list(&value("allowed-hosts", "MCP_HTTP_ALLOWED_HOSTS", "")),
                allowed_origins: comma_list(&value(
                    "allowed-origins",
                    "MCP_HTTP_ALLOWED_ORIGINS",
                    "",
                )),
                rate_limit: number(
                    &value("rate-limit", "MCP_HTTP_RATE_LIMIT", "120"),
                    "MCP_HTTP_RATE_LIMIT",
                    0,
                    1000000,
                )?,
                metrics,
                tls,
            },
        })
    }
}

fn number<T: FromStr + PartialOrd + TryFrom<u64>>(
    value: &str,
    name: &str,
    min: u64,
    max: u64,
) -> anyhow::Result<T> {
    let n = value
        .parse::<u64>()
        .ok()
        .filter(|n| (min..=max).contains(n))
        .with_context(|| format!("{name} must be an integer between {min} and {max}"))?;
    T::try_from(n).map_err(|_| anyhow::anyhow!("{name} is out of range"))
}

fn comma_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

fn dotenv_values(args: &ArgMatches) -> anyhow::Result<HashMap<String, String>> {
    if args.get_flag("no-env-file") {
        return Ok(HashMap::new());
    }
    let explicit = args
        .get_one::<String>("env-file")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("MCP_ENV_FILE").map(PathBuf::from));
    let path = explicit
        .as_deref()
        .unwrap_or_else(|| std::path::Path::new(".env"));
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound && explicit.is_none() => {
            return Ok(HashMap::new());
        }
        Err(_) => bail!("cannot read dotenv file {}", path.display()),
    };
    dotenvy::from_read_iter(file)
        .collect::<Result<_, _>>()
        .map_err(|_| {
            anyhow::anyhow!(
                "cannot parse dotenv file {}; check KEY=value syntax",
                path.display()
            )
        })
}
