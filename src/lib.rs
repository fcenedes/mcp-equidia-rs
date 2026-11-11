#![doc = include_str!("../docs/crate.md")]
#![doc = include_str!("../docs/tools.md")]
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]

mod catalog;
mod cli;
mod client;
mod metrics;
mod policy;
mod server;
mod settings;
mod tls;
mod tools;

/// Run the CLI or MCP server using process arguments and environment.
///
/// With no command, serves MCP on stdin/stdout. Diagnostics go to stderr.
/// Tools and schema inspection are offline. Network requests use GET only.
///
/// # Errors
/// Returns an error for invalid configuration, API failures, or transport errors.
///
/// # Example
/// ```no_run
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     mcp_equidia_rs::run().await
/// }
/// ```
pub async fn run() -> anyhow::Result<()> {
    cli::run().await
}
