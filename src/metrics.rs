//! Process-local Prometheus metrics with bounded, non-sensitive labels.

use std::{
    collections::BTreeMap,
    fmt::Write,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

const BUCKETS: [f64; 8] = [0.01, 0.05, 0.1, 0.5, 1., 5., 30., 120.];

#[derive(Default)]
struct Observation {
    count: u64,
    seconds: f64,
    buckets: [u64; 8],
}

#[derive(Default)]
pub(crate) struct Metrics {
    tools: Mutex<BTreeMap<(String, &'static str), Observation>>,
    upstream: Mutex<BTreeMap<(String, String), u64>>,
    pub rate_limited: AtomicU64,
}

impl Metrics {
    pub fn tool(&self, name: &str, success: bool, elapsed: Duration) {
        let mut tools = self.tools.lock().unwrap_or_else(|error| error.into_inner());
        // Callers validate names against the catalog before recording them.
        let observation = tools
            .entry((name.into(), if success { "success" } else { "error" }))
            .or_default();
        observation.count += 1;
        let seconds = elapsed.as_secs_f64();
        observation.seconds += seconds;
        for (bound, count) in BUCKETS.iter().zip(&mut observation.buckets) {
            if seconds <= *bound {
                *count += 1;
            }
        }
    }

    pub fn upstream(&self, api: &str, status: &str) {
        *self
            .upstream
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry((api.into(), status.into()))
            .or_default() += 1;
    }

    pub fn render(&self) -> String {
        let mut output = String::from(
            "# HELP mcp_equidia_tool_calls_total Completed tool calls, including validation failures.\n# TYPE mcp_equidia_tool_calls_total counter\n",
        );
        let tools = self.tools.lock().unwrap_or_else(|error| error.into_inner());
        for ((tool, outcome), observation) in tools.iter() {
            let _ = writeln!(
                output,
                "mcp_equidia_tool_calls_total{{tool=\"{tool}\",outcome=\"{outcome}\"}} {}",
                observation.count
            );
        }
        output.push_str("# HELP mcp_equidia_tool_duration_seconds Tool wall time, including queueing and retries.\n# TYPE mcp_equidia_tool_duration_seconds histogram\n");
        for ((tool, outcome), observation) in tools.iter() {
            for (bound, count) in BUCKETS.iter().zip(observation.buckets) {
                let _ = writeln!(
                    output,
                    "mcp_equidia_tool_duration_seconds_bucket{{tool=\"{tool}\",outcome=\"{outcome}\",le=\"{bound}\"}} {count}"
                );
            }
            let _ = writeln!(
                output,
                "mcp_equidia_tool_duration_seconds_bucket{{tool=\"{tool}\",outcome=\"{outcome}\",le=\"+Inf\"}} {}",
                observation.count
            );
            let _ = writeln!(
                output,
                "mcp_equidia_tool_duration_seconds_count{{tool=\"{tool}\",outcome=\"{outcome}\"}} {}",
                observation.count
            );
            let _ = writeln!(
                output,
                "mcp_equidia_tool_duration_seconds_sum{{tool=\"{tool}\",outcome=\"{outcome}\"}} {}",
                observation.seconds
            );
        }
        drop(tools);
        output.push_str("# HELP mcp_equidia_upstream_requests_total Equidia HTTP attempts, including retries.\n# TYPE mcp_equidia_upstream_requests_total counter\n");
        for ((api, status), count) in self
            .upstream
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
        {
            let _ = writeln!(
                output,
                "mcp_equidia_upstream_requests_total{{api=\"{api}\",status=\"{status}\"}} {count}"
            );
        }
        output.push_str("# HELP mcp_equidia_http_rate_limited_total HTTP requests rejected by the process rate limit.\n# TYPE mcp_equidia_http_rate_limited_total counter\n");
        let _ = writeln!(
            output,
            "mcp_equidia_http_rate_limited_total {}",
            self.rate_limited.load(Ordering::Relaxed)
        );
        output
    }
}
