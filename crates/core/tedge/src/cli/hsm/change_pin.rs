use anyhow::Context;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;
use tedge_p11::pkcs11::uri::Pkcs11Uri;
use tedge_p11::service::ChangePinRequest;
use tedge_p11::CryptokiConfig;
use tedge_p11::SecretString;

use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;

/// Arguments of the PIN-change command.
#[derive(Debug, clap::Args)]
pub struct ChangePinArgs {
    /// The new user PIN to set. If omitted, it is prompted for interactively (with confirmation).
    #[arg(long)]
    pub new_pin: Option<String>,

    /// The current user PIN.
    ///
    /// If omitted, the PIN configured for tedge-p11-server is used. Ignored when `--reset` is
    /// given.
    #[arg(long)]
    pub current_pin: Option<String>,

    /// Reset the user PIN using the Security Officer PIN instead of the current user PIN.
    ///
    /// Use this to recover a token when the current user PIN is unknown or the token is locked
    /// out. Requires the Security Officer PIN.
    #[arg(long, default_value_t = false)]
    pub reset: bool,

    /// The Security Officer PIN, used with `--reset`. If omitted, it is prompted for interactively.
    #[arg(long)]
    pub so_pin: Option<String>,

    /// A PKCS #11 URI selecting the token whose user PIN to change.
    ///
    /// If omitted, the single initialized token is selected automatically. If several initialized
    /// tokens exist, the command fails and lists their URIs so one can be selected.
    pub uri: Option<String>,
}

impl ChangePinArgs {
    pub fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        // Changing a token PIN is not scoped to a cloud; the token is selected by URI.
        let cryptoki_config = config
            .device
            .cryptoki_config(None::<&dyn CloudConfig>)?
            .context("Cryptoki config is not enabled")?;

        Ok(ChangePinCmd {
            cryptoki_config,
            uri: self.uri,
            new_pin: self.new_pin,
            current_pin: self.current_pin,
            reset: self.reset,
            so_pin: self.so_pin,
        }
        .into_boxed())
    }
}

pub struct ChangePinCmd {
    pub cryptoki_config: CryptokiConfig,
    pub uri: Option<String>,
    pub new_pin: Option<String>,
    pub current_pin: Option<String>,
    pub reset: bool,
    pub so_pin: Option<String>,
}

#[async_trait::async_trait]
impl Command for ChangePinCmd {
    fn description(&self) -> String {
        "Change the user PIN of a PKCS #11 token.".into()
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        // Resolve the new PIN, prompting (with confirmation) when it wasn't passed as a flag so it
        // doesn't end up in the shell history or process list.
        let new_pin = match self.new_pin.clone() {
            Some(pin) => pin,
            None => prompt_new_pin()?,
        };

        // For a reset we need the Security Officer PIN; prompt for it if not provided.
        let so_pin = if self.reset {
            let so_pin = match self.so_pin.clone() {
                Some(pin) => pin,
                None => rpassword::prompt_password("Enter Security Officer PIN: ")
                    .context("Failed to read the Security Officer PIN")?,
            };
            Some(SecretString::from(so_pin))
        } else {
            self.so_pin.clone().map(SecretString::from)
        };

        let cryptoki = tedge_p11::tedge_p11_service(self.cryptoki_config.clone())?;
        let response = cryptoki.change_pin(ChangePinRequest {
            uri: self.uri.clone(),
            new_pin: SecretString::from(new_pin.clone()),
            old_pin: self.current_pin.clone().map(SecretString::from),
            so_pin,
            reset: self.reset,
        })?;

        if self.reset {
            eprintln!("The user PIN of token '{}' was reset.", response.uri);
        } else {
            eprintln!("The user PIN of token '{}' was changed.", response.uri);
        }

        // Keep tedge-p11-server's configured PIN in sync so signing keeps working with the new PIN.
        // Without this the provider would keep using the old PIN and fail to log in to the token.
        // Only the PIN of the token the provider signs with belongs there: changing the PIN of
        // another token must not replace it, or the provider could no longer log in to its own.
        let socket_mode = matches!(self.cryptoki_config, CryptokiConfig::SocketService { .. });
        if !is_provider_token(&config, socket_mode, &response.uri) {
            eprintln!(
                "`device.cryptoki.pin` was left unchanged, as it is the PIN of the token tedge \
                 signs with, and that is not the token whose PIN was changed."
            );
            return Ok(());
        }
        match save_pin_to_config(config, &new_pin).await {
            Ok(()) => {
                eprintln!(
                    "The `device.cryptoki.pin` configuration setting was updated with the new PIN."
                );
                eprintln!(
                    "Restart tedge-p11-server for the change to take effect, e.g. \
                     `tedgectl restart tedge-p11-server` (or your service manager's equivalent)."
                );
            }
            Err(e) => {
                eprintln!(
                    "Warning: the token PIN was changed but `device.cryptoki.pin` could not be \
                     updated ({e:#}). Update it manually with \
                     `tedge config set device.cryptoki.pin <new-pin>` and restart tedge-p11-server, \
                     otherwise signing will fail."
                );
            }
        }

        Ok(())
    }
}

/// Prompts for a new PIN twice and returns it once both entries match.
fn prompt_new_pin() -> anyhow::Result<String> {
    let pin =
        rpassword::prompt_password("Enter new user PIN: ").context("Failed to read the new PIN")?;
    let confirm = rpassword::prompt_password("Confirm new user PIN: ")
        .context("Failed to read the new PIN confirmation")?;
    anyhow::ensure!(pin == confirm, "The entered PINs do not match.");
    anyhow::ensure!(!pin.is_empty(), "The new PIN must not be empty.");
    Ok(pin)
}

/// Whether `token_uri`, as reported by the PKCS #11 service, is the token tedge signs with.
///
/// Which token that is follows the same precedence the provider applies when signing. In `socket`
/// mode `device.cryptoki.uri` scopes tedge-p11-server, so when it names a token that token is the
/// one, whatever the `key_uri` settings say. Otherwise, and always in `module` mode where the
/// provider never reads `device.cryptoki.uri`, the `key_uri` settings select the signing key and
/// so its token. When nothing names a token, there is nothing to tell the changed token apart from
/// the provider's, so it is taken to be the same, which is the case on the usual single-token
/// device. Like the delete-key guard, this checks the common device/cloud settings and not
/// per-profile ones.
fn is_provider_token(config: &TEdgeConfig, socket_mode: bool, token_uri: &str) -> bool {
    // Reported URIs are generated by the service, so this only fails on a foreign implementation;
    // then keep the previous behaviour of updating the PIN rather than silently breaking signing.
    let Ok(changed) = Pkcs11Uri::parse(token_uri) else {
        return true;
    };

    // A configured URI names a token by label and/or serial; every attribute it sets must match.
    let names_token = |uri: &Pkcs11Uri| uri.token.is_some() || uri.serial.is_some();
    let matches = |uri: &Pkcs11Uri| {
        uri.token
            .as_deref()
            .is_none_or(|label| changed.token.as_deref() == Some(label))
            && uri
                .serial
                .as_deref()
                .is_none_or(|serial| changed.serial.as_deref() == Some(serial))
    };
    let configured = |setting: &str| {
        let key = setting.parse().ok()?;
        let uri = config.read_string(&key).ok()?;
        Pkcs11Uri::parse(&uri)
            .ok()
            .filter(names_token)
            .map(|uri| matches(&uri))
    };

    if socket_mode {
        if let Some(matches) = configured("device.cryptoki.uri") {
            return matches;
        }
    }

    let key_uris = [
        "device.key_uri",
        "c8y.device.key_uri",
        "az.device.key_uri",
        "aws.device.key_uri",
    ];
    let mut any_token_named = false;
    for setting in key_uris {
        if let Some(matches) = configured(setting) {
            any_token_named = true;
            if matches {
                return true;
            }
        }
    }
    !any_token_named
}

/// Updates `device.cryptoki.pin` in tedge config so the PKCS #11 provider uses the new PIN.
async fn save_pin_to_config(config: TEdgeConfig, new_pin: &str) -> anyhow::Result<()> {
    let key = "device.cryptoki.pin"
        .parse::<WritableKey>()
        .context("failed to parse 'device.cryptoki.pin' as a WritableKey")?;
    config
        .update_toml(&|dto, _reader| dto.try_update_str(&key, new_pin).map_err(|e| e.into()))
        .await
        .map_err(anyhow::Error::new)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    async fn config_with(toml: &str) -> TEdgeConfig {
        let tempdir = TempTedgeDir::new();
        tempdir.file("tedge.toml").with_raw_content(toml);
        TEdgeConfig::load(tempdir.path()).await.unwrap()
    }

    const SOCKET: bool = true;
    const MODULE: bool = false;

    // The URI shape reported back by the service for the token it acted on.
    const SPARE: &str = "pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM;serial=0123;token=spare";
    const TEDGE: &str = "pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM;serial=4567;token=tedge";

    // Both settings name a different token: the configuration this PR is about.
    const CONFLICT: &str = "[device]\nkey_uri = \"pkcs11:token=tedge;object=key\"\n\
        [device.cryptoki]\nuri = \"pkcs11:token=spare\"\n";

    #[tokio::test]
    async fn in_socket_mode_the_server_scope_decides_over_the_key_uri() {
        let config = config_with(CONFLICT).await;
        // tedge-p11-server is scoped to `spare`, so that is the token it logs in to...
        assert!(is_provider_token(&config, SOCKET, SPARE));
        // ...and the key_uri naming `tedge` does not make `tedge` the provider's token.
        assert!(!is_provider_token(&config, SOCKET, TEDGE));
    }

    #[tokio::test]
    async fn in_module_mode_the_cryptoki_uri_is_ignored() {
        let config = config_with(CONFLICT).await;
        // The provider never reads `device.cryptoki.uri` in module mode; key_uri selects the key.
        assert!(is_provider_token(&config, MODULE, TEDGE));
        assert!(!is_provider_token(&config, MODULE, SPARE));
    }

    #[tokio::test]
    async fn a_token_matching_the_configured_key_uri_is_the_providers() {
        let config = config_with("[device]\nkey_uri = \"pkcs11:token=spare;object=key\"\n").await;
        assert!(is_provider_token(&config, SOCKET, SPARE));
        assert!(is_provider_token(&config, MODULE, SPARE));
    }

    #[tokio::test]
    async fn a_token_other_than_the_configured_one_is_not_the_providers() {
        let config = config_with("[device.cryptoki]\nuri = \"pkcs11:token=tedge\"\n").await;
        assert!(!is_provider_token(&config, SOCKET, SPARE));
    }

    #[tokio::test]
    async fn a_configured_serial_must_match_too() {
        let config =
            config_with("[device]\nkey_uri = \"pkcs11:token=spare;serial=ffff;object=key\"\n")
                .await;
        assert!(!is_provider_token(&config, SOCKET, SPARE));
    }

    #[tokio::test]
    async fn any_token_is_the_providers_when_no_configured_uri_names_one() {
        let config = config_with("[device]\nkey_uri = \"pkcs11:object=key\"\n").await;
        assert!(is_provider_token(&config, SOCKET, SPARE));

        let config = config_with("").await;
        assert!(is_provider_token(&config, MODULE, SPARE));
    }

    #[tokio::test]
    async fn a_cloud_key_uri_counts_as_well() {
        let config =
            config_with("[c8y.device]\nkey_uri = \"pkcs11:token=spare;object=key\"\n").await;
        assert!(is_provider_token(&config, SOCKET, SPARE));
    }
}
