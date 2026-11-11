//! Native TLS configuration. Reloads replace the entire configuration atomically.

use std::{path::PathBuf, sync::Arc};

use axum_server::tls_rustls::RustlsConfig;
use rustls::{RootCertStore, ServerConfig, server::WebPkiClientVerifier};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(crate) struct TlsFiles {
    pub cert: PathBuf,
    pub key: PathBuf,
    pub client_ca: Option<PathBuf>,
}

impl TlsFiles {
    pub async fn load(&self) -> anyhow::Result<Arc<ServerConfig>> {
        let files = self.clone();
        // PEM parsers and key matching perform filesystem/CPU work off the async executor.
        tokio::task::spawn_blocking(move || files.load_blocking()).await
            .ok().and_then(Result::ok)
            .ok_or_else(|| anyhow::anyhow!("cannot load TLS certificate, key or client CA; check PEM format, matching key and file permissions"))
    }

    fn load_blocking(&self) -> Result<Arc<ServerConfig>, Box<dyn std::error::Error + Send + Sync>> {
        let certificates =
            CertificateDer::pem_file_iter(&self.cert)?.collect::<Result<Vec<_>, _>>()?;
        let key = PrivateKeyDer::from_pem_file(&self.key)?;
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let builder = ServerConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])?;
        let builder = if let Some(ca) = &self.client_ca {
            let mut roots = RootCertStore::empty();
            for cert in CertificateDer::pem_file_iter(ca)? {
                roots.add(cert?)?;
            }
            builder.with_client_cert_verifier(
                WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider).build()?,
            )
        } else {
            builder.with_no_client_auth()
        };
        let mut config = builder.with_single_cert(certificates, key)?;
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Ok(Arc::new(config))
    }
}

pub(crate) fn spawn_reload(
    files: TlsFiles,
    config: RustlsConfig,
    cancellation: CancellationToken,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    #[cfg(unix)]
    {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;
        Ok(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancellation.cancelled() => break,
                    event = signal.recv() => {
                        if event.is_none() { break; }
                        match files.load().await {
                            Ok(replacement) => {
                                config.reload_from_config(replacement);
                                tracing::info!("TLS configuration reloaded");
                            }
                            Err(_) => tracing::error!("TLS reload failed; previous configuration retained"),
                        }
                    }
                }
            }
        }))
    }
    #[cfg(not(unix))]
    {
        let _ = (files, config);
        Ok(tokio::spawn(async move { cancellation.cancelled().await }))
    }
}
