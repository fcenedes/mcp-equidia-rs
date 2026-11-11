use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use reqwest::{
    StatusCode, Url,
    header::{ACCEPT, HeaderMap, HeaderValue},
};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::{catalog::Operation, metrics::Metrics, settings::Settings};

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("{0}")]
    Input(String),
    #[error("Equidia returned HTTP {0}; {1}")]
    Upstream(u16, &'static str),
    #[error("upstream network request failed (check connectivity and TLS trust)")]
    Network,
    #[error("upstream response exceeds the configured byte limit")]
    TooLarge,
    #[error("upstream response was not valid JSON")]
    InvalidJson,
    #[error("tool deadline exceeded")]
    Deadline,
    #[error("internal JSON data error")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone)]
pub(crate) struct EquidiaClient {
    http: reqwest::Client,
    base: Url,
    retries: u32,
    max_bytes: usize,
    deadline: Duration,
    concurrency: Arc<Semaphore>,
    pub metrics: Arc<Metrics>,
}

impl EquidiaClient {
    pub fn new(settings: &Settings) -> Result<Self> {
        let base = Url::parse(&settings.base_url)
            .map_err(|_| Error::Input("EQUIDIA_BASE_URL must be an origin URL".into()))?;
        let loopback = base.host_str().is_some_and(|h| {
            h == "localhost"
                || h.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if !matches!(base.scheme(), "https" | "http")
            || (base.scheme() == "http" && !loopback)
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || !matches!(base.path(), "" | "/")
        {
            return Err(Error::Input(
                "EQUIDIA_BASE_URL must be an HTTPS origin (HTTP allowed only on loopback)".into(),
            ));
        }
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        let http = reqwest::Client::builder()
            .user_agent(concat!("mcp-equidia-rs/", env!("CARGO_PKG_VERSION")))
            .default_headers(headers)
            // No redirect may leave the configured origin or downgrade HTTPS.
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(settings.timeout.min(10)))
            .timeout(Duration::from_secs(settings.timeout))
            .build()
            .map_err(|_| Error::Network)?;
        Ok(Self {
            http,
            base,
            retries: settings.max_retries,
            max_bytes: settings.max_response_bytes,
            deadline: Duration::from_secs(settings.tool_timeout),
            concurrency: Arc::new(Semaphore::new(settings.concurrency)),
            metrics: Arc::default(),
        })
    }

    pub async fn call(&self, op: &Operation, args: &Value) -> Result<Value> {
        tokio::time::timeout(self.deadline, self.execute(op, args))
            .await
            .map_err(|_| Error::Deadline)?
    }

    async fn execute(&self, op: &Operation, args: &Value) -> Result<Value> {
        // Defense in depth: the client itself cannot dispatch writes.
        if op.method != "GET" {
            return Err(Error::Input("only GET operations are supported".into()));
        }
        let url = self.url(op, args)?;
        let _permit = self
            .concurrency
            .acquire()
            .await
            .map_err(|_| Error::Network)?;
        for attempt in 0..=self.retries {
            let response = match self.http.get(url.clone()).send().await {
                Ok(response) => response,
                Err(_) if attempt < self.retries => {
                    self.metrics.upstream("equidia", "network_error");
                    tokio::time::sleep(Duration::from_millis(250 * (1 << attempt))).await;
                    continue;
                }
                Err(_) => {
                    self.metrics.upstream("equidia", "network_error");
                    return Err(Error::Network);
                }
            };
            let status = response.status();
            self.metrics.upstream("equidia", status.as_str());
            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                && attempt < self.retries
            {
                let delay = retry_after(response.headers())
                    .unwrap_or_else(|| Duration::from_millis(250 * (1 << attempt)));
                drop(response);
                tokio::time::sleep(delay).await;
                continue;
            }
            if !status.is_success() {
                let guidance = match status.as_u16() {
                    401 | 403 => "anonymous access was denied by Equidia",
                    404 => "resource or route not found; check identifiers",
                    429 => "upstream rate limit reached; try later",
                    300..=399 => "redirect refused; check the configured API origin",
                    _ => "upstream request failed; check the documented parameters",
                };
                return Err(Error::Upstream(status.as_u16(), guidance));
            }
            if response
                .content_length()
                .is_some_and(|n| n > self.max_bytes as u64)
            {
                return Err(Error::TooLarge);
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|s| s.to_str().ok())
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_owned();
            let mut response = response;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| Error::Network)? {
                if bytes.len().saturating_add(chunk.len()) > self.max_bytes {
                    return Err(Error::TooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            let meta = json!({"source":"Equidia","url":url.as_str(),"status":status.as_u16(),"contentType":content_type,"bytes":bytes.len()});
            let data = if bytes.is_empty() {
                Value::Null
            } else if content_type == "application/json"
                || content_type.ends_with("+json")
                || matches!(bytes.first(), Some(b'[' | b'{'))
            {
                serde_json::from_slice(&bytes).map_err(|_| Error::InvalidJson)?
            } else {
                return Err(Error::InvalidJson);
            };
            return Ok(json!({"data":data,"meta":meta}));
        }
        Err(Error::Network)
    }

    fn url(&self, op: &Operation, args: &Value) -> Result<Url> {
        let mut url = self.base.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| Error::Input("invalid API origin".into()))?;
            path.clear();
            for segment in op.path.trim_start_matches('/').split('/') {
                if let Some(key) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    let value = args[key]
                        .as_str()
                        .ok_or_else(|| Error::Input(format!("missing path parameter: {key}")))?;
                    if value.is_empty()
                        || value == "."
                        || value == ".."
                        || !value
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
                    {
                        return Err(Error::Input(format!("invalid path parameter: {key}")));
                    }
                    path.push(value);
                } else {
                    path.push(segment);
                }
            }
        }
        let mut query = Vec::new();
        for parameter in &op.parameters {
            if parameter.location != "query" {
                continue;
            }
            let Some(value) = args.get(&parameter.name) else {
                continue;
            };
            if let Some(values) = value.as_array() {
                if parameter.explode {
                    for item in values {
                        query.push((parameter.name.clone(), scalar(item)?));
                    }
                } else {
                    let separator = match parameter.style.as_str() {
                        "spaceDelimited" => " ",
                        "pipeDelimited" => "|",
                        _ => ",",
                    };
                    query.push((
                        parameter.name.clone(),
                        values
                            .iter()
                            .map(scalar)
                            .collect::<Result<Vec<_>>>()?
                            .join(separator),
                    ));
                }
            } else {
                query.push((parameter.name.clone(), scalar(value)?));
            }
        }
        // Most Equidia collections default to 10 or 50. Never request unbounded
        // pages implicitly, and don't assume every endpoint honors pagination.
        if op.parameters.iter().any(|p| p.name == "items-per-page")
            && !args
                .as_object()
                .is_some_and(|a| a.contains_key("items-per-page"))
        {
            query.push(("items-per-page".into(), "10".into()));
        }
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        Ok(url)
    }
}

fn scalar(value: &Value) -> Result<String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Bool(_) | Value::Number(_) => Ok(value.to_string()),
        _ => Err(Error::Input(
            "query parameters must be scalar values or scalar arrays".into(),
        )),
    }
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    value
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
        .or_else(|| {
            httpdate::parse_http_date(value)
                .ok()
                .map(|t| t.duration_since(SystemTime::now()).unwrap_or_default())
        })
}
