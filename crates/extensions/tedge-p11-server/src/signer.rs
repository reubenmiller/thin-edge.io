use std::sync::Arc;

use anyhow::Context;
use camino::Utf8PathBuf;
use rustls::sign::Signer;
use rustls::sign::SigningKey;
use tracing::error;
use tracing::instrument;

use crate::client::TedgeP11Client;
use crate::pkcs11::Cryptoki;
use crate::pkcs11::CryptokiConfigDirect;

#[derive(Debug, Clone)]
pub enum CryptokiConfig {
    Direct(CryptokiConfigDirect),
    SocketService {
        socket_path: Utf8PathBuf,
        uri: Option<Arc<str>>,
    },
}

/// Returns a rustls SigningKey that depending on the config, either connects to
/// tedge-p11-server or calls cryptoki module directly.
pub fn signing_key(config: CryptokiConfig) -> anyhow::Result<Arc<dyn SigningKey>> {
    let signing_key: Arc<dyn SigningKey> = match config {
        CryptokiConfig::Direct(config_direct) => {
            let uri = config_direct.uri.clone();
            let cryptoki =
                Cryptoki::new(config_direct).context("Failed to load cryptoki library")?;
            Arc::new(
                cryptoki
                    .signing_key(uri.as_deref())
                    .context("failed to create a TLS signer using PKCS#11 device")?,
            )
        }
        CryptokiConfig::SocketService { socket_path, uri } => Arc::new(TedgeP11ClientSigningKey {
            client: TedgeP11Client::with_ready_check(socket_path.into()),
            uri,
        }),
    };

    Ok(signing_key)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TedgeP11ClientSigningKey {
    pub client: TedgeP11Client,
    pub uri: Option<Arc<str>>,
}

impl SigningKey for TedgeP11ClientSigningKey {
    #[instrument(skip_all)]
    fn choose_scheme(
        &self,
        offered: &[rustls::SignatureScheme],
    ) -> Option<Box<dyn rustls::sign::Signer>> {
        let uri = self.uri.as_ref().map(|s| s.to_string());
        let response = match self.client.choose_scheme(offered, uri) {
            Ok(response) => response,
            Err(err) => {
                error!(?err, "Failed to choose scheme using cryptoki signer");
                return None;
            }
        };
        let scheme = response?;

        Some(Box::new(TedgeP11ClientSigner {
            client: self.client.clone(),
            scheme,
            uri: self.uri.clone(),
        }))
    }

    fn algorithm(&self) -> rustls::SignatureAlgorithm {
        // here we have no choice but to panic but this is only called by servers when verifying
        // client hello so it should never be called in our case
        self.client.algorithm().unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TedgeP11ClientSigner {
    pub client: TedgeP11Client,
    scheme: rustls::SignatureScheme,
    pub uri: Option<Arc<str>>,
}

impl Signer for TedgeP11ClientSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        let response = match self
            .client
            .sign(message, self.uri.as_ref().map(|s| s.to_string()))
        {
            Ok(response) => response,
            Err(err) => {
                return Err(rustls::Error::Other(rustls::OtherError(Arc::from(
                    Box::from(err),
                ))));
            }
        };
        Ok(response)
    }

    fn scheme(&self) -> rustls::SignatureScheme {
        self.scheme
    }
}
