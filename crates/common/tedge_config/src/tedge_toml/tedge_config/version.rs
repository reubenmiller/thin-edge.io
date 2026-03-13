//! Versioning and migrations of the main config file.

use std::borrow::Cow;

use toml::Table;

use super::WritableKey;

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(into = "&'static str", try_from = "String")]
/// A version of tedge.toml, used to manage migrations (see [Self::migrations])
pub enum TEdgeTomlVersion {
    #[default]
    One,
    Two,
    Three,
}

impl TryFrom<String> for TEdgeTomlVersion {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "1" => Ok(Self::One),
            "2" => Ok(Self::Two),
            "3" => Ok(Self::Three),
            _ => todo!(),
        }
    }
}

impl From<TEdgeTomlVersion> for &'static str {
    fn from(value: TEdgeTomlVersion) -> Self {
        match value {
            TEdgeTomlVersion::One => "1",
            TEdgeTomlVersion::Two => "2",
            TEdgeTomlVersion::Three => "3",
        }
    }
}

impl From<TEdgeTomlVersion> for toml::Value {
    fn from(value: TEdgeTomlVersion) -> Self {
        let str: &str = value.into();
        toml::Value::String(str.to_owned())
    }
}

pub enum TomlMigrationStep {
    UpdateFieldValue {
        key: &'static str,
        value: toml::Value,
    },

    MoveKey {
        original: &'static str,
        target: Cow<'static, str>,
    },

    RemoveTableIfEmpty {
        key: &'static str,
    },
}

impl TomlMigrationStep {
    pub fn apply_to(self, mut toml: toml::Value) -> toml::Value {
        match self {
            TomlMigrationStep::MoveKey { original, target } => {
                let mut doc = &mut toml;
                let (tables, field) = original.rsplit_once('.').unwrap();
                for key in tables.split('.') {
                    if doc.as_table().map(|table| table.contains_key(key)) == Some(true) {
                        doc = &mut doc[key];
                    } else {
                        return toml;
                    }
                }
                let value = doc.as_table_mut().unwrap().remove(field);

                if let Some(value) = value {
                    let mut doc = &mut toml;
                    let (tables, field) = target.rsplit_once('.').unwrap();
                    for key in tables.split('.') {
                        let table = doc.as_table_mut().unwrap();
                        if !table.contains_key(key) {
                            table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                        }
                        doc = &mut doc[key];
                    }
                    let table = doc.as_table_mut().unwrap();
                    // TODO if this returns Some, something is going wrong? Maybe this could be an error, or maybe it doesn't matter
                    table.insert(field.to_owned(), value);
                }
            }
            TomlMigrationStep::UpdateFieldValue { key, value } => {
                let mut doc = &mut toml;
                let (tables, field) = key.rsplit_once('.').unwrap();
                for key in tables.split('.') {
                    let table = doc.as_table_mut().unwrap();
                    if !table.contains_key(key) {
                        table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                    }
                    doc = &mut doc[key];
                }
                let table = doc.as_table_mut().unwrap();
                // TODO if this returns Some, something is going wrong? Maybe this could be an error, or maybe it doesn't matter
                table.insert(field.to_owned(), value);
            }
            TomlMigrationStep::RemoveTableIfEmpty { key } => {
                let mut doc = &mut toml;
                let (parents, target) = key.rsplit_once('.').unwrap();
                for key in parents.split('.') {
                    let table = doc.as_table_mut().unwrap();
                    if !table.contains_key(key) {
                        table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                    }
                    doc = &mut doc[key];
                }
                let table = doc.as_table_mut().unwrap();
                if let Some(table) = table.get(target) {
                    let table = table.as_table().unwrap();
                    // TODO make sure this is covered in toml migration test
                    if !table.is_empty() {
                        return toml;
                    }
                }
                table.remove(target);
            }
        }

        toml
    }
}

impl TEdgeTomlVersion {
    fn next(self) -> Self {
        match self {
            Self::One => Self::Two,
            Self::Two => Self::Three,
            Self::Three => Self::Three,
        }
    }

    /// The migrations to upgrade `tedge.toml` from its current version to the
    /// next version.
    ///
    /// If this returns `None`, the version of `tedge.toml` is the latest
    /// version, and no migrations need to be applied.
    pub fn migrations(self) -> Option<Vec<TomlMigrationStep>> {
        use WritableKey::*;
        let mv = |original, target: WritableKey| TomlMigrationStep::MoveKey {
            original,
            target: target.to_cow_str(),
        };
        let update_version_field = || TomlMigrationStep::UpdateFieldValue {
            key: "config.version",
            value: self.next().into(),
        };
        let rm = |key| TomlMigrationStep::RemoveTableIfEmpty { key };

        match self {
            Self::One => Some(vec![
                mv("mqtt.port", MqttBindPort),
                mv("mqtt.bind_address", MqttBindAddress),
                mv("mqtt.client_host", MqttClientHost),
                mv("mqtt.client_port", MqttClientPort),
                mv("mqtt.client_ca_file", MqttClientAuthCaFile),
                mv("mqtt.client_ca_path", MqttClientAuthCaDir),
                mv("mqtt.client_auth.cert_file", MqttClientAuthCertFile),
                mv("mqtt.client_auth.key_file", MqttClientAuthKeyFile),
                rm("mqtt.client_auth"),
                mv("mqtt.external_port", MqttExternalBindPort),
                mv("mqtt.external_bind_address", MqttExternalBindAddress),
                mv("mqtt.external_bind_interface", MqttExternalBindInterface),
                mv("mqtt.external_capath", MqttExternalCaPath),
                mv("mqtt.external_certfile", MqttExternalCertFile),
                mv("mqtt.external_keyfile", MqttExternalKeyFile),
                mv("az.mapper_timestamp", AzMapperTimestamp(None)),
                mv("aws.mapper_timestamp", AwsMapperTimestamp(None)),
                mv("http.port", HttpBindPort),
                mv("http.bind_address", HttpBindAddress),
                mv("software.default_plugin_type", SoftwarePluginDefault),
                mv("run.lock_files", RunLockFiles),
                mv("firmware.child_update_timeout", FirmwareChildUpdateTimeout),
                mv("c8y.smartrest_templates", C8ySmartrestTemplates(None)),
                update_version_field(),
            ]),
            Self::Two => Some(vec![
                TomlMigrationStep::MoveKey {
                    original: "apt.dpk",
                    target: Cow::Borrowed("apt.dpkg"),
                },
                update_version_field(),
            ]),
            Self::Three => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_version_has_no_migrations() {
        assert!(TEdgeTomlVersion::Three.migrations().is_none());
    }

    #[test]
    fn move_key_relocates_an_existing_value() {
        let input = toml::toml!(
            [apt.dpk.options]
            config = "keepold"
        );
        let output = TomlMigrationStep::MoveKey {
            original: "apt.dpk",
            target: Cow::Borrowed("apt.dpkg"),
        }
        .apply_to(toml::Value::Table(input));
        assert_eq!(output["apt"]["dpkg"]["options"]["config"].as_str(), Some("keepold"));
        assert!(output["apt"].as_table().unwrap().get("dpk").is_none());
    }

    #[test]
    fn move_key_is_a_noop_when_source_key_is_absent() {
        let input = toml::toml!(
            [apt.name]
            filter = "tedge.*"
        );
        let expected = input.clone();
        let output = TomlMigrationStep::MoveKey {
            original: "apt.dpk",
            target: Cow::Borrowed("apt.dpkg"),
        }
        .apply_to(toml::Value::Table(input));
        assert_eq!(output, toml::Value::Table(expected));
    }

    #[test]
    fn remove_table_if_empty_removes_an_empty_table() {
        let input = toml::toml!(
            [mqtt.client_auth]
        );
        let output = TomlMigrationStep::RemoveTableIfEmpty { key: "mqtt.client_auth" }
            .apply_to(toml::Value::Table(input));
        assert!(output["mqtt"].as_table().unwrap().get("client_auth").is_none());
    }

    #[test]
    fn remove_table_if_empty_preserves_a_non_empty_table() {
        let input = toml::toml!(
            [mqtt.client_auth]
            cert_file = "/path/to/cert.pem"
        );
        let expected = input.clone();
        let output = TomlMigrationStep::RemoveTableIfEmpty { key: "mqtt.client_auth" }
            .apply_to(toml::Value::Table(input));
        assert_eq!(output, toml::Value::Table(expected));
    }

    #[test]
    fn v2_migration_renames_apt_dpk_to_apt_dpkg() {
        let input = toml::toml!(
            [config]
            version = "2"

            [apt.dpk.options]
            config = "keepold"
        );
        let migrated = TEdgeTomlVersion::Two
            .migrations()
            .unwrap()
            .into_iter()
            .fold(toml::Value::Table(input), |toml, step| step.apply_to(toml));
        assert_eq!(migrated["config"]["version"].as_str(), Some("3"));
        assert_eq!(migrated["apt"]["dpkg"]["options"]["config"].as_str(), Some("keepold"));
        assert!(migrated["apt"].as_table().unwrap().get("dpk").is_none());
    }
}
