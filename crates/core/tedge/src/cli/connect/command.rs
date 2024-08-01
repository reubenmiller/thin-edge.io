use crate::bridge::aws::BridgeConfigAwsParams;
use crate::bridge::azure::BridgeConfigAzureParams;
use crate::bridge::c8y::BridgeConfigC8yParams;
use crate::bridge::BridgeConfig;
use crate::bridge::BridgeLocation;
use crate::bridge::CommonMosquittoConfig;
use crate::cli::common::Cloud;
use crate::cli::connect::jwt_token::*;
use crate::cli::connect::*;
use crate::command::Command;
use crate::ConfigError;
use camino::Utf8PathBuf;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::QoS::AtLeastOnce;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tedge_config::system_services::*;
use tedge_config::TEdgeConfig;
use tedge_config::*;
use tedge_utils::paths::create_directories;
use tedge_utils::paths::ok_if_not_found;
use tedge_utils::paths::DraftFile;
use tracing::warn;
use which::which;

use crate::bridge::AWS_CONFIG_FILENAME;
use crate::bridge::AZURE_CONFIG_FILENAME;
use crate::bridge::C8Y_CONFIG_FILENAME;
use crate::bridge::TEDGE_BRIDGE_CONF_DIR_PATH;

const WAIT_FOR_CHECK_SECONDS: u64 = 2;
pub(crate) const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);
const MOSQUITTO_RESTART_TIMEOUT_SECONDS: u64 = 5;
const MQTT_TLS_PORT: u16 = 8883;

pub struct ConnectCommand {
    pub config_location: TEdgeConfigLocation,
    pub config: TEdgeConfig,
    pub cloud: Cloud,
    pub is_test_connection: bool,
    pub offline_mode: bool,
    pub service_manager: Arc<dyn SystemServiceManager>,
}

pub enum DeviceStatus {
    AlreadyExists,
    Unknown,
}

impl Command for ConnectCommand {
    fn description(&self) -> String {
        if self.is_test_connection {
            format!("test connection to {} cloud.", self.cloud)
        } else {
            format!("connect to {} cloud.", self.cloud)
        }
    }

    fn execute(&self) -> anyhow::Result<()> {
        let config = &self.config;
        let bridge_config = bridge_config(config, self.cloud)?;
        let updated_mosquitto_config = CommonMosquittoConfig::from_tedge_config(config);

        if self.is_test_connection {
            // If the bridge is part of the mapper, the bridge config file won't exist
            // TODO tidy me up once mosquitto is no longer required for bridge
            return if self.check_if_bridge_exists(&bridge_config) {
                match self.check_connection(config) {
                    Ok(DeviceStatus::AlreadyExists) => {
                        let cloud = bridge_config.cloud_name;
                        println!("Connection check to {} cloud is successful.\n", cloud);
                        Ok(())
                    }
                    Ok(DeviceStatus::Unknown) => Err(ConnectError::UnknownDeviceStatus.into()),
                    Err(err) => Err(err.into()),
                }
            } else {
                Err((ConnectError::DeviceNotConnected {
                    cloud: self.cloud.to_string(),
                })
                .into())
            };
        }

        let device_type = &config.device.ty;

        match new_bridge(
            &bridge_config,
            &updated_mosquitto_config,
            self.service_manager.as_ref(),
            &self.config_location,
            device_type,
            self.offline_mode,
        ) {
            Ok(()) => println!("Successfully created bridge connection!\n"),
            Err(ConnectError::SystemServiceError(
                SystemServiceError::ServiceManagerUnavailable { .. },
            )) => return Ok(()),
            Err(err) => return Err(err.into()),
        }

        if bridge_config.use_mapper && bridge_config.bridge_location == BridgeLocation::BuiltIn {
            // If the bridge is built in, the mapper needs to be running with the new configuration
            // to be connected
            self.start_mapper();
        }

        if self.offline_mode {
            println!("Offline mode. Skipping connection check.\n");
        } else {
            match self
                .check_connection_with_retries(config, bridge_config.connection_check_attempts)
            {
                Ok(DeviceStatus::AlreadyExists) => {
                    println!("Connection check is successful.\n");
                }
                _ => {
                    println!(
                        "Warning: Bridge has been configured, but {} connection check failed.\n",
                        self.cloud
                    );
                }
            }
        }

        if bridge_config.use_mapper && bridge_config.bridge_location == BridgeLocation::Mosquitto {
            // If the bridge is in mosquitto, the mapper should only start once the cloud connection
            // is verified
            self.start_mapper();
        }

        if let Cloud::C8y = self.cloud {
            if !self.offline_mode {
                check_connected_c8y_tenant_as_configured(
                    config,
                    &config
                        .c8y
                        .mqtt
                        .or_none()
                        .map(|u| u.host().to_string())
                        .unwrap_or_default(),
                );
            }
            enable_software_management(&bridge_config, self.service_manager.as_ref());
        }

        Ok(())
    }
}

impl ConnectCommand {
    fn check_connection_with_retries(
        &self,
        config: &TEdgeConfig,
        max_attempts: i32,
    ) -> Result<DeviceStatus, ConnectError> {
        for i in 1..max_attempts {
            let result = self.check_connection(config);
            if let Ok(DeviceStatus::AlreadyExists) = result {
                return result;
            }
            println!(
                "Connection test failed, attempt {} of {}\n",
                i, max_attempts,
            );
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
        self.check_connection(config)
    }
    fn check_connection(&self, config: &TEdgeConfig) -> Result<DeviceStatus, ConnectError> {
        println!(
            "Sending packets to check connection. This may take up to {} seconds.\n",
            WAIT_FOR_CHECK_SECONDS
        );
        match self.cloud {
            Cloud::Azure => check_device_status_azure(config),
            Cloud::Aws => check_device_status_aws(config),
            Cloud::C8y => check_device_status_c8y(config),
        }
    }

    fn check_if_bridge_exists(&self, br_config: &BridgeConfig) -> bool {
        let bridge_conf_path = self
            .config_location
            .tedge_config_root_path
            .join(TEDGE_BRIDGE_CONF_DIR_PATH)
            .join(br_config.config_file.clone());

        br_config.bridge_location == BridgeLocation::BuiltIn
            || Path::new(&bridge_conf_path).exists()
    }

    fn start_mapper(&self) {
        println!("Checking if tedge-mapper is installed.\n");

        if which("tedge-mapper").is_err() {
            println!("Warning: tedge-mapper is not installed.\n");
        } else {
            self.service_manager
                .as_ref()
                .start_and_enable_service(self.cloud.mapper_service(), std::io::stdout());
        }
    }
}

pub fn bridge_config(
    config: &TEdgeConfig,
    cloud: self::Cloud,
) -> Result<BridgeConfig, ConfigError> {
    let bridge_location = match config.mqtt.bridge.built_in {
        true => BridgeLocation::BuiltIn,
        false => BridgeLocation::Mosquitto,
    };
    match cloud {
        Cloud::Azure => {
            let params = BridgeConfigAzureParams {
                mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from(
                    config.az.url.or_config_not_set()?.as_str(),
                )
                .map_err(TEdgeConfigError::from)?,
                config_file: AZURE_CONFIG_FILENAME.into(),
                bridge_root_cert_path: config.az.root_cert_path.clone(),
                remote_clientid: config.device.id.try_read(config)?.clone(),
                bridge_certfile: config.device.cert_path.clone(),
                bridge_keyfile: config.device.key_path.clone(),
                bridge_location,
            };

            Ok(BridgeConfig::from(params))
        }
        Cloud::Aws => {
            let params = BridgeConfigAwsParams {
                mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from(
                    config.aws.url.or_config_not_set()?.as_str(),
                )
                .map_err(TEdgeConfigError::from)?,
                config_file: AWS_CONFIG_FILENAME.into(),
                bridge_root_cert_path: config.aws.root_cert_path.clone(),
                remote_clientid: config.device.id.try_read(config)?.clone(),
                bridge_certfile: config.device.cert_path.clone(),
                bridge_keyfile: config.device.key_path.clone(),
                bridge_location,
            };

            Ok(BridgeConfig::from(params))
        }
        Cloud::C8y => {
            let params = BridgeConfigC8yParams {
                mqtt_host: config.c8y.mqtt.or_config_not_set()?.clone(),
                config_file: C8Y_CONFIG_FILENAME.into(),
                bridge_root_cert_path: config.c8y.root_cert_path.clone(),
                remote_username: if config.c8y.username.is_empty() {
                    None
                } else {
                    Some(config.c8y.username.clone())
                },
                remote_password: if config.c8y.password.is_empty() {
                    None
                } else {
                    Some(config.c8y.password.clone())
                },
                remote_clientid: config.device.id.try_read(config)?.clone(),
                bridge_certfile: config.device.cert_path.clone(),
                bridge_keyfile: config.device.key_path.clone(),
                smartrest_templates: config.c8y.smartrest.templates.clone(),
                smartrest_one_templates: config.c8y.smartrest1.templates.clone(),
                include_local_clean_session: config.c8y.bridge.include.local_cleansession.clone(),
                bridge_location,
            };

            Ok(BridgeConfig::from(params))
        }
    }
}

// Check the connection by using the jwt token retrieval over the mqtt.
// If successful in getting the jwt token '71,xxxxx', the connection is established.
fn check_device_status_c8y(tedge_config: &TEdgeConfig) -> Result<DeviceStatus, ConnectError> {
    let prefix = &tedge_config.c8y.bridge.topic_prefix;
    let c8y_topic_builtin_jwt_token_downstream = format!("{prefix}/s/dat");
    let c8y_topic_builtin_jwt_token_upstream = format!("{prefix}/s/uat");
    const CLIENT_ID: &str = "check_connection_c8y";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .with_clean_session(true)
        .rumqttc_options()?;

    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (mut client, mut connection) = rumqttc::Client::new(mqtt_options, 10);
    connection
        .eventloop
        .network_options
        .set_connection_timeout(CONNECTION_TIMEOUT.as_secs());
    let mut acknowledged = false;
    let mut exists = false;

    client.subscribe(&c8y_topic_builtin_jwt_token_downstream, AtLeastOnce)?;

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    &c8y_topic_builtin_jwt_token_upstream,
                    rumqttc::QoS::AtMostOnce,
                    false,
                    "",
                )?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                // We got a response
                let token = String::from_utf8(response.payload.to_vec()).unwrap();
                // FIXME: what does this magic number mean?
                if token.contains("71") {
                    exists = true;
                    break;
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                eprintln!("ERROR: Local MQTT publish has timed out.");
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                eprintln!("ERROR: Disconnected");
                break;
            }
            Err(err) => {
                eprintln!("ERROR: {:?}", err);
                break;
            }
            _ => {}
        }
    }

    // Cleanly disconnect client
    client.disconnect()?;
    for event in connection.iter() {
        match event {
            Ok(Event::Outgoing(Outgoing::Disconnect)) | Err(_) => break,
            _ => {}
        }
    }

    if exists {
        return Ok(DeviceStatus::AlreadyExists);
    }

    if acknowledged {
        // The request has been sent but without a response
        Ok(DeviceStatus::Unknown)
    } else {
        // The request has not even been sent
        Err(ConnectError::TimeoutElapsedError)
    }
}

// Here We check the az device twin properties over mqtt to check if connection has been open.
// First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
// device twin property output.
// Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID.
// The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}.
// Here if the status is 200 then it's success.
fn check_device_status_azure(tedge_config: &TEdgeConfig) -> Result<DeviceStatus, ConnectError> {
    const AZURE_TOPIC_DEVICE_TWIN_DOWNSTREAM: &str = r##"az/twin/res/#"##;
    const AZURE_TOPIC_DEVICE_TWIN_UPSTREAM: &str = r#"az/twin/GET/?$rid=1"#;
    const CLIENT_ID: &str = "check_connection_az";
    const REGISTRATION_PAYLOAD: &[u8] = b"";
    const REGISTRATION_OK: &str = "200";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .rumqttc_options()?;

    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (mut client, mut connection) = rumqttc::Client::new(mqtt_options, 10);
    let mut acknowledged = false;
    let mut exists = false;

    client.subscribe(AZURE_TOPIC_DEVICE_TWIN_DOWNSTREAM, AtLeastOnce)?;

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    AZURE_TOPIC_DEVICE_TWIN_UPSTREAM,
                    AtLeastOnce,
                    false,
                    REGISTRATION_PAYLOAD,
                )?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                // We got a response
                if response.topic.contains(REGISTRATION_OK) {
                    println!("Received expected response message.");
                    exists = true;
                    break;
                } else {
                    break;
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                eprintln!("ERROR: Local MQTT publish has timed out.");
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                eprintln!("ERROR: Disconnected");
                break;
            }
            Err(err) => {
                eprintln!("ERROR: {:?}", err);
                break;
            }
            _ => {}
        }
    }

    // Cleanly disconnect client
    client.disconnect()?;
    for event in connection.iter() {
        match event {
            Ok(Event::Outgoing(Outgoing::Disconnect)) | Err(_) => break,
            _ => {}
        }
    }

    if exists {
        return Ok(DeviceStatus::AlreadyExists);
    }

    if acknowledged {
        // The request has been sent but without a response
        Ok(DeviceStatus::Unknown)
    } else {
        // The request has not even been sent
        Err(ConnectError::TimeoutElapsedError)
    }
}

fn check_device_status_aws(tedge_config: &TEdgeConfig) -> Result<DeviceStatus, ConnectError> {
    const AWS_TOPIC_PUB_CHECK_CONNECTION: &str = r#"aws/test-connection"#;
    const AWS_TOPIC_SUB_CHECK_CONNECTION: &str = r#"aws/connection-success"#;
    const CLIENT_ID: &str = "check_connection_aws";
    const REGISTRATION_PAYLOAD: &[u8] = b"";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .rumqttc_options()?;
    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (mut client, mut connection) = rumqttc::Client::new(mqtt_options, 10);
    let mut acknowledged = false;
    let mut exists = false;

    client.subscribe(AWS_TOPIC_SUB_CHECK_CONNECTION, AtLeastOnce)?;

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    AWS_TOPIC_PUB_CHECK_CONNECTION,
                    AtLeastOnce,
                    false,
                    REGISTRATION_PAYLOAD,
                )?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                // We got a response
                println!("Received expected response on topic {}.", response.topic);
                exists = true;
                break;
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                eprintln!("ERROR: Local MQTT publish has timed out.");
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                eprintln!("ERROR: Disconnected");
                break;
            }
            Err(err) => {
                eprintln!("ERROR: {:?}", err);
                break;
            }
            _ => {}
        }
    }

    // Cleanly disconnect client
    client.disconnect()?;
    for event in connection.iter() {
        match event {
            Ok(Event::Outgoing(Outgoing::Disconnect)) | Err(_) => break,
            _ => {}
        }
    }

    if exists {
        return Ok(DeviceStatus::AlreadyExists);
    }

    if acknowledged {
        // The request has been sent but without a response
        Ok(DeviceStatus::Unknown)
    } else {
        // The request has not even been sent
        Err(ConnectError::TimeoutElapsedError)
    }
}

fn new_bridge(
    bridge_config: &BridgeConfig,
    common_mosquitto_config: &CommonMosquittoConfig,
    service_manager: &dyn SystemServiceManager,
    config_location: &TEdgeConfigLocation,
    device_type: &str,
    offline_mode: bool,
) -> Result<(), ConnectError> {
    println!("Checking if {} is available.\n", service_manager.name());
    let service_manager_result = service_manager.check_operational();

    if let Err(SystemServiceError::ServiceManagerUnavailable { cmd: _, name }) =
        &service_manager_result
    {
        println!(
            "Warning: '{}' service manager is not available on the system.\n",
            name
        );
    }

    if bridge_config.bridge_location == BridgeLocation::Mosquitto {
        println!("Checking if configuration for requested bridge already exists.\n");
        bridge_config_exists(config_location, bridge_config)?;
    }

    println!("Validating the bridge certificates.\n");
    bridge_config.validate()?;

    if bridge_config.cloud_name.eq("c8y") {
        if offline_mode {
            println!("Offline mode. Skipping device creation in Cumulocity cloud.\n")
        } else {
            println!("Creating the device in Cumulocity cloud.\n");
            c8y_direct_connection::create_device_with_direct_connection(
                bridge_config,
                device_type,
            )?;
        }
    }

    if let Err(err) =
        write_generic_mosquitto_config_to_file(config_location, common_mosquitto_config)
    {
        // We want to preserve previous errors and therefore discard result of this function.
        let _ = clean_up(config_location, bridge_config);
        return Err(err);
    }

    if bridge_config.bridge_location == BridgeLocation::Mosquitto {
        println!("Saving configuration for requested bridge.\n");

        if let Err(err) = write_bridge_config_to_file(config_location, bridge_config) {
            // We want to preserve previous errors and therefore discard result of this function.
            let _ = clean_up(config_location, bridge_config);
            return Err(err);
        }
    } else {
        println!("Deleting mosquitto bridge configuration in favour of built-in bridge\n");
        use_built_in_bridge(config_location, bridge_config)?;
    }

    if let Err(err) = service_manager_result {
        println!("'tedge connect' configured the necessary tedge components, but you will have to start the required services on your own.");
        println!("Start/restart mosquitto and other thin edge components.");
        println!("thin-edge.io works seamlessly with 'systemd'.\n");
        return Err(err.into());
    }

    restart_mosquitto(bridge_config, service_manager, config_location)?;

    println!(
        "Awaiting mosquitto to start. This may take up to {} seconds.\n",
        MOSQUITTO_RESTART_TIMEOUT_SECONDS
    );
    std::thread::sleep(std::time::Duration::from_secs(
        MOSQUITTO_RESTART_TIMEOUT_SECONDS,
    ));

    println!("Enabling mosquitto service on reboots.\n");
    if let Err(err) = service_manager.enable_service(SystemService::Mosquitto) {
        clean_up(config_location, bridge_config)?;
        return Err(err.into());
    }

    Ok(())
}

pub fn chown_certificate_and_key(bridge_config: &BridgeConfig) {
    let (user, group) = match bridge_config.bridge_location {
        BridgeLocation::BuiltIn => ("tedge", "tedge"),
        BridgeLocation::Mosquitto => (crate::BROKER_USER, crate::BROKER_GROUP),
    };
    // Ignore errors - This was the behavior with the now deprecated user manager.
    // - When `tedge cert create` is not run as root, a certificate is created but owned by the user running the command.
    // - A better approach could be to remove this `chown` and run the command as mosquitto.
    for path in [
        &bridge_config.bridge_certfile,
        &bridge_config.bridge_keyfile,
    ] {
        if let Err(err) = tedge_utils::file::change_user_and_group(path.as_ref(), user, group) {
            warn!("Failed to change ownership of {path} to {user}:{group}: {err}");
        }
    }
}

fn restart_mosquitto(
    bridge_config: &BridgeConfig,
    service_manager: &dyn SystemServiceManager,
    config_location: &TEdgeConfigLocation,
) -> Result<(), ConnectError> {
    println!("Restarting mosquitto service.\n");

    if let Err(err) = service_manager.stop_service(SystemService::Mosquitto) {
        clean_up(config_location, bridge_config)?;
        return Err(err.into());
    }

    chown_certificate_and_key(bridge_config);

    if let Err(err) = service_manager.restart_service(SystemService::Mosquitto) {
        clean_up(config_location, bridge_config)?;
        return Err(err.into());
    }

    Ok(())
}

fn enable_software_management(
    bridge_config: &BridgeConfig,
    service_manager: &dyn SystemServiceManager,
) {
    println!("Enabling software management.\n");
    if bridge_config.use_agent {
        println!("Checking if tedge-agent is installed.\n");
        if which("tedge-agent").is_ok() {
            service_manager
                .start_and_enable_service(SystemService::TEdgeSMAgent, std::io::stdout());
        } else {
            println!("Info: Software management is not installed. So, skipping enabling related components.\n");
        }
    }
}

// To preserve error chain and not discard other errors we need to ignore error here
// (don't use '?' with the call to this function to preserve original error).
pub fn clean_up(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(config_location, bridge_config);
    std::fs::remove_file(path).or_else(ok_if_not_found)?;
    Ok(())
}

pub fn use_built_in_bridge(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(config_location, bridge_config);
    std::fs::write(
        path,
        "# This file is left empty as the built-in bridge is enabled",
    )
    .or_else(ok_if_not_found)?;
    Ok(())
}

fn bridge_config_exists(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(config_location, bridge_config);
    if path.exists() {
        return Err(ConnectError::ConfigurationExists {
            cloud: bridge_config.cloud_name.to_string(),
        });
    }
    Ok(())
}

fn write_generic_mosquitto_config_to_file(
    config_location: &TEdgeConfigLocation,
    common_mosquitto_config: &CommonMosquittoConfig,
) -> Result<(), ConnectError> {
    let dir_path = config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH);

    // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
    create_directories(dir_path)?;

    let common_config_path =
        get_common_mosquitto_config_file_path(config_location, common_mosquitto_config);
    let mut common_draft = DraftFile::new(common_config_path)?.with_mode(0o644);
    common_mosquitto_config.serialize(&mut common_draft)?;
    common_draft.persist()?;

    Ok(())
}

fn write_bridge_config_to_file(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let dir_path = config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH);

    // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
    create_directories(dir_path)?;

    let config_path = get_bridge_config_file_path(config_location, bridge_config);
    let mut config_draft = DraftFile::new(config_path)?.with_mode(0o644);
    bridge_config.serialize(&mut config_draft)?;
    config_draft.persist()?;

    Ok(())
}

fn get_bridge_config_file_path(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Utf8PathBuf {
    config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH)
        .join(&bridge_config.config_file)
}

fn get_common_mosquitto_config_file_path(
    config_location: &TEdgeConfigLocation,
    common_mosquitto_config: &CommonMosquittoConfig,
) -> Utf8PathBuf {
    config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH)
        .join(&common_mosquitto_config.config_file)
}

// To confirm the connected c8y tenant is the one that user configured.
fn check_connected_c8y_tenant_as_configured(tedge_config: &TEdgeConfig, configured_url: &str) {
    match get_connected_c8y_url(tedge_config) {
        Ok(url) if url == configured_url => {}
        Ok(url) => println!(
            "Warning: Connecting to {}, but the configured URL is {}.\n\
            The device certificate has to be removed from the former tenant.\n",
            url, configured_url
        ),
        Err(_) => println!("Failed to get the connected tenant URL from Cumulocity.\n"),
    }
}
