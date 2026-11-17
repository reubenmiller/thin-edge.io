use std::sync::Arc;

use anyhow::Context;
use rustls::sign::SigningKey;

// These are only used by the unix-only proxy client signing path.
#[cfg(unix)]
use rustls::sign::Signer;
#[cfg(unix)]
use tracing::error;
#[cfg(unix)]
use tracing::instrument;

use crate::pkcs11::Cryptoki;
use crate::pkcs11::Pkcs11Signer;
use crate::pkcs11::SessionParams;
use crate::pkcs11::SigScheme;
use crate::CryptokiConfig;

#[cfg(unix)]
use crate::proxy::client::TedgeP11Client;

/// A signer using a private key object located on the PKCS11 token.
///
/// Is backed by either direct cryptoki library usage or by tedge-p11-server client.
///
/// Contains a handle to Pkcs11-backed private key that will be used for signing, selected at construction time.
pub trait TedgeP11Signer: SigningKey {
    /// Signs the message using the selected private key.
    fn sign(&self, msg: &[u8]) -> anyhow::Result<Vec<u8>>;

    /// Signs the message using the selected private key and signature scheme.
    ///
    /// Useful when a key can be used with multiple schemes, eg. RSA key using PKCS 1.5 or PSS.
    fn sign2(&self, msg: &[u8], sigscheme: SigScheme) -> anyhow::Result<Vec<u8>>;

    fn to_rustls_signing_key(self: Arc<Self>) -> Arc<dyn rustls::sign::SigningKey>;
}

impl TedgeP11Signer for Pkcs11Signer {
    fn sign(&self, msg: &[u8]) -> anyhow::Result<Vec<u8>> {
        Pkcs11Signer::sign(self, msg, None)
    }

    fn sign2(&self, msg: &[u8], sigscheme: SigScheme) -> anyhow::Result<Vec<u8>> {
        Pkcs11Signer::sign(self, msg, Some(sigscheme))
    }

    fn to_rustls_signing_key(self: Arc<Self>) -> Arc<dyn rustls::sign::SigningKey> {
        self
    }
}

/// Returns a rustls SigningKey that depending on the config, either connects to
/// tedge-p11-server or calls cryptoki module directly.
pub fn signing_key(config: CryptokiConfig) -> anyhow::Result<Arc<dyn TedgeP11Signer>> {
    let signing_key: Arc<dyn TedgeP11Signer> = match config {
        CryptokiConfig::Direct(config_direct) => {
            let uri = config_direct.uri.as_ref().map(|u| u.to_string());
            let pin = Some(config_direct.pin.clone());
            let cryptoki =
                Cryptoki::new(config_direct).context("Failed to load cryptoki library")?;
            Arc::new(
                cryptoki
                    .signing_key_retry(SessionParams { uri, pin })
                    .context("failed to create a TLS signer using PKCS#11 device")?,
            )
        }
        CryptokiConfig::SocketService {
            socket_path,
            uri,
            pin,
        } => {
            #[cfg(unix)]
            {
                let mut client = TedgeP11Client::with_ready_check(socket_path.into());
                client.pin = pin;
                Arc::new(TedgeP11ClientSigningKey { client, uri })
            }
            #[cfg(not(unix))]
            {
                let _ = (socket_path, uri, pin);
                anyhow::bail!(
                    "PKCS#11 socket proxy is not supported on Windows. \
                     Use CryptokiConfig::Direct instead."
                );
            }
        }
    };

    Ok(signing_key)
}

#[cfg(unix)]
#[derive(Debug, Clone)]
pub struct TedgeP11ClientSigningKey {
    pub client: TedgeP11Client,
    pub uri: Option<Arc<str>>,
}

#[cfg(unix)]
impl TedgeP11Signer for TedgeP11ClientSigningKey {
    fn sign(&self, msg: &[u8]) -> anyhow::Result<Vec<u8>> {
        self.client
            .sign(msg, self.uri.as_ref().map(|s| s.to_string()))
    }

    fn sign2(&self, msg: &[u8], sigscheme: SigScheme) -> anyhow::Result<Vec<u8>> {
        self.client
            .sign2(msg, self.uri.as_ref().map(|s| s.to_string()), sigscheme)
    }

    fn to_rustls_signing_key(self: Arc<Self>) -> Arc<dyn rustls::sign::SigningKey> {
        self
    }
}

#[cfg(unix)]
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
        let scheme = response.scheme?.0;

        Some(Box::new(TedgeP11ClientSigner {
            client: self.client.clone(),
            scheme,
            uri: self.uri.clone(),
        }))
    }

    fn algorithm(&self) -> rustls::SignatureAlgorithm {
        self.client.algorithm().unwrap()
    }
}

#[cfg(unix)]
#[derive(Debug, Clone)]
pub struct TedgeP11ClientSigner {
    pub client: TedgeP11Client,
    scheme: rustls::SignatureScheme,
    pub uri: Option<Arc<str>>,
}

#[cfg(unix)]
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
