//! Detection of the domain names under which the Cumulocity tenant is reachable.
//!
//! A tenant can be reachable under several domain names, e.g. while a dedicated instance is
//! being renamed, both the old and the new domain point to the same server.
//! URLs received from Cumulocity can use any of these names,
//! and all of them have to be routed through the local auth proxy.
//!
//! These names are listed by the TLS certificate served by Cumulocity,
//! so they are detected without any extra configuration.

use anyhow::Context;
use certificate::CloudHttpConfig;
use reqwest::tls::TlsInfo;
use reqwest::Url;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;
use tracing::info;
use tracing::warn;
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::FromDer;
use x509_parser::prelude::X509Certificate;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MIN_RETRY_DELAY: Duration = Duration::from_secs(30);
const REFRESH_INTERVAL: Duration = Duration::from_secs(3600);

/// The domain names, possibly wildcards, under which the tenant is reachable
///
/// Clones share the same names, so names detected in the background
/// are seen by all the clones.
#[derive(Clone, Debug, Default)]
pub struct TenantAliases {
    names: Arc<RwLock<Arc<[String]>>>,
}

impl TenantAliases {
    pub fn set(&self, names: Vec<String>) {
        *self.names.write().unwrap() = names.into();
    }

    pub fn names(&self) -> Arc<[String]> {
        self.names.read().unwrap().clone()
    }

    /// Returns true if the host is one of the tenant aliases
    pub fn matches(&self, host: &str) -> bool {
        self.names
            .read()
            .unwrap()
            .iter()
            .any(|name| dns_name_matches(name, host))
    }
}

/// Matches a host against a certificate DNS name, which can be a wildcard (RFC 6125)
///
/// A wildcard only matches a single, left-most, label: `*.example.com` matches `a.example.com`
/// but neither `example.com` nor `a.b.example.com`.
fn dns_name_matches(name: &str, host: &str) -> bool {
    let host = host.trim_end_matches('.');
    match name.strip_prefix("*.") {
        Some(parent) => host
            .split_once('.')
            .is_some_and(|(label, rest)| !label.is_empty() && rest.eq_ignore_ascii_case(parent)),
        None => name.eq_ignore_ascii_case(host),
    }
}

/// Extracts the DNS names listed by the Subject Alternative Name extension of a DER certificate
pub fn certificate_dns_names(der: &[u8]) -> anyhow::Result<Vec<String>> {
    let (_, certificate) =
        X509Certificate::from_der(der).context("Failed to parse the server certificate")?;
    let Some(san) = certificate
        .subject_alternative_name()
        .context("Invalid subject alternative name in the server certificate")?
    else {
        return Ok(vec![]);
    };
    Ok(san
        .value
        .general_names
        .iter()
        .filter_map(|name| match name {
            GeneralName::DNSName(name) => Some(name.to_string()),
            _ => None,
        })
        .collect())
}

/// Detects in the background the aliases of the tenant reachable at `c8y_host` (i.e. `c8y.http`)
pub fn spawn_detection(c8y_host: &str, http_config: &CloudHttpConfig, aliases: TenantAliases) {
    match TenantAliasDetector::new(c8y_host, http_config, aliases) {
        Ok(detector) => {
            tokio::spawn(detector.run());
        }
        Err(err) => warn!("The Cumulocity tenant domain names will not be detected: {err:#}"),
    }
}

/// Periodically detects the tenant aliases from the certificate served by Cumulocity
pub struct TenantAliasDetector {
    client: reqwest::Client,
    url: Url,
    aliases: TenantAliases,
}

impl TenantAliasDetector {
    /// Builds a detector for the tenant reachable at `c8y_host` (i.e. `c8y.http`)
    pub fn new(
        c8y_host: &str,
        http_config: &CloudHttpConfig,
        aliases: TenantAliases,
    ) -> anyhow::Result<Self> {
        let url = format!("https://{c8y_host}/")
            .parse()
            .with_context(|| format!("Invalid Cumulocity host: {c8y_host}"))?;
        let client = http_config
            .client_builder()
            .tls_info(true)
            .build()
            .context("Failed to build an HTTP client")?;
        Ok(TenantAliasDetector {
            client,
            url,
            aliases,
        })
    }

    /// Returns the DNS names listed by the certificate currently served by Cumulocity
    pub async fn detect(&self) -> anyhow::Result<Vec<String>> {
        // Only the TLS handshake matters: the response status is ignored
        let response = self
            .client
            .head(self.url.clone())
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .with_context(|| format!("Failed to connect to {}", self.url))?;
        let der = response
            .extensions()
            .get::<TlsInfo>()
            .and_then(TlsInfo::peer_certificate)
            .context("No server certificate received")?;
        certificate_dns_names(der)
    }

    /// Detects the tenant aliases, retrying on failure and refreshing them periodically
    ///
    /// On failure, the previously detected aliases are kept.
    pub async fn run(self) {
        let mut retry_delay = MIN_RETRY_DELAY;
        loop {
            match self.detect().await {
                Ok(names) => {
                    if *self.aliases.names() != *names {
                        info!("Cumulocity tenant reachable as: {}", names.join(", "));
                        self.aliases.set(names);
                    }
                    retry_delay = MIN_RETRY_DELAY;
                    tokio::time::sleep(REFRESH_INTERVAL).await;
                }
                Err(err) => {
                    warn!(
                        "Failed to detect the Cumulocity tenant domain names, retrying in {}s: {err:#}",
                        retry_delay.as_secs()
                    );
                    tokio::time::sleep(retry_delay).await;
                    retry_delay = (retry_delay * 2).min(REFRESH_INTERVAL);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::pki_types::PrivateKeyDer;
    use rustls::pki_types::PrivatePkcs8KeyDer;
    use test_case::test_case;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

    #[test_case("new.example.com", "new.example.com", true; "exact")]
    #[test_case("New.Example.com", "new.example.COM", true; "exact ignoring case")]
    #[test_case("new.example.com", "new.example.com.", true; "exact with trailing dot")]
    #[test_case("new.example.com", "other.example.com", false; "other host")]
    #[test_case("*.example.com", "tenant.example.com", true; "wildcard")]
    #[test_case("*.example.com", "example.com", false; "wildcard requires a label")]
    #[test_case("*.example.com", "a.tenant.example.com", false; "wildcard matches a single label")]
    #[test_case("*.example.com", "tenant.example.org", false; "wildcard other domain")]
    fn matching_dns_names(name: &str, host: &str, expected: bool) {
        assert_eq!(dns_name_matches(name, host), expected);
    }

    #[test]
    fn aliases_are_shared_by_clones() {
        let aliases = TenantAliases::default();
        let clone = aliases.clone();
        assert!(!clone.matches("iot.new-domain.com"));

        aliases.set(vec![
            "iot.old-domain.com".to_string(),
            "iot.new-domain.com".to_string(),
        ]);

        assert!(clone.matches("iot.new-domain.com"));
        assert!(clone.matches("iot.old-domain.com"));
        assert!(!clone.matches("iot.other-domain.com"));
    }

    #[test]
    fn dns_names_are_extracted_from_certificate() {
        let names = vec![
            "iot.old-domain.com".to_string(),
            "*.new-domain.com".to_string(),
        ];
        let certificate = rcgen::generate_simple_self_signed(names.clone()).unwrap();

        let detected = certificate_dns_names(certificate.cert.der()).unwrap();

        assert_eq!(detected, names);
    }

    #[tokio::test]
    async fn aliases_are_detected_from_the_certificate_served_by_cumulocity() {
        let names = vec![
            "localhost".to_string(),
            "iot.old-domain.com".to_string(),
            "*.new-domain.com".to_string(),
        ];
        let certificate = rcgen::generate_simple_self_signed(names.clone()).unwrap();
        let c8y_host = spawn_tls_server(&certificate).await;
        let trusted = reqwest::Certificate::from_der(certificate.cert.der()).unwrap();
        let http_config = CloudHttpConfig::new([trusted], None);

        let detector =
            TenantAliasDetector::new(&c8y_host, &http_config, TenantAliases::default()).unwrap();

        assert_eq!(detector.detect().await.unwrap(), names);
    }

    #[tokio::test]
    async fn detection_fails_when_cumulocity_is_unreachable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let http_config = CloudHttpConfig::test_value();

        let detector = TenantAliasDetector::new(
            &format!("127.0.0.1:{port}"),
            &http_config,
            TenantAliases::default(),
        )
        .unwrap();

        assert!(detector.detect().await.is_err());
    }

    // Spawns an HTTPS server, returning its `host:port`
    async fn spawn_tls_server(certificate: &rcgen::CertifiedKey<rcgen::KeyPair>) -> String {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            certificate.signing_key.serialize_der(),
        ));
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate.cert.der().clone()], key)
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let Ok(mut stream) = acceptor.accept(stream).await else {
                    continue;
                };
                let mut request = [0; 1024];
                let _ = stream.read(&mut request).await;
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                    .await;
                let _ = stream.shutdown().await;
            }
        });
        format!("localhost:{port}")
    }
}
