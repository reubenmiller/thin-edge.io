use super::ConnectError;
use crate::bridge::BridgeConfig;
use crate::cli::connect::CONNECTION_TIMEOUT;
use certificate::parse_root_certificate::create_tls_config;
use certificate::parse_root_certificate::create_tls_config_without_client_cert;
use rumqttc::tokio_rustls::rustls::AlertDescription;
use rumqttc::tokio_rustls::rustls::CertificateError;
use rumqttc::tokio_rustls::rustls::Error;
use rumqttc::Client;
use rumqttc::ConnectionError;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::MqttOptions;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::QoS;
use rumqttc::TlsError;
use rumqttc::Transport;

// Connect directly to the c8y cloud over mqtt and publish device create message.
pub fn create_device_with_direct_connection(
    bridge_config: &BridgeConfig,
    device_type: &str,
) -> Result<(), ConnectError> {
    const DEVICE_ALREADY_EXISTS: &[u8] = b"41,100,Device already existing";
    const DEVICE_CREATE_ERROR_TOPIC: &str = "s/e";

    let address = bridge_config.address.clone();

    let mut mqtt_options = MqttOptions::new(
        bridge_config.remote_clientid.clone(),
        address.host().to_string(),
        address.port().into(),
    );
    mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

    let use_legacy_auth =
        bridge_config.remote_username.is_some() && bridge_config.remote_password.is_some();
    if use_legacy_auth {
        mqtt_options.set_credentials(
            bridge_config.remote_username.clone().unwrap_or_default(),
            bridge_config.remote_password.clone().unwrap_or_default(),
        );
    }

    let tls_config = if use_legacy_auth {
        create_tls_config_without_client_cert(&bridge_config.bridge_root_cert_path)?
    } else {
        create_tls_config(
            &bridge_config.bridge_root_cert_path,
            &bridge_config.bridge_keyfile,
            &bridge_config.bridge_certfile,
        )?
    };

    mqtt_options.set_transport(Transport::tls_with_config(tls_config.into()));

    let (mut client, mut connection) = Client::new(mqtt_options, 10);
    connection
        .eventloop
        .network_options
        .set_connection_timeout(CONNECTION_TIMEOUT.as_secs());

    client.subscribe(DEVICE_CREATE_ERROR_TOPIC, QoS::AtLeastOnce)?;

    let mut device_create_try: usize = 0;
    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_) | Packet::PubAck(_) | Packet::PubComp(_))) => {
                publish_device_create_message(
                    &mut client,
                    &bridge_config.remote_clientid.clone(),
                    device_type,
                )?;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                // We got a response
                if response.payload == DEVICE_ALREADY_EXISTS {
                    return Ok(());
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // If not received any response then resend the device create request again.
                // else timeout.
                if device_create_try < 1 {
                    publish_device_create_message(
                        &mut client,
                        &bridge_config.remote_clientid.clone(),
                        device_type,
                    )?;
                    device_create_try += 1;
                } else {
                    // No messages have been received for a while
                    break;
                }
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                eprintln!("ERROR: Disconnected");
                break;
            }
            Err(ConnectionError::Io(err)) if err.kind() == std::io::ErrorKind::InvalidData => {
                if let Some(Error::AlertReceived(alert_description)) = err
                    .get_ref()
                    .and_then(|custom_err| custom_err.downcast_ref::<Error>())
                {
                    if let AlertDescription::CertificateUnknown = alert_description {
                        // Either the device cert is not uploaded to c8y or
                        // another cert is set in device.cert_path
                        eprintln!("The device certificate is not trusted by Cumulocity.");
                        return Err(ConnectError::ConnectionCheckError);
                    } else if let AlertDescription::HandshakeFailure = alert_description {
                        // Non-paired private key is set in device.key_path
                        eprintln!(
                            "The private key is not paired with the certificate. Check your 'device.key_path'."
                        );
                        return Err(ConnectError::ConnectionCheckError);
                    }
                }
                eprintln!("ERROR: {:?}", err);
                return Err(ConnectError::ConnectionCheckError);
            }
            Err(ConnectionError::Tls(TlsError::Io(err)))
                if err.kind() == std::io::ErrorKind::InvalidData =>
            {
                match err
                    .get_ref()
                    .and_then(|custom_err| custom_err.downcast_ref::<Error>())
                {
                    Some(Error::InvalidCertificate(CertificateError::UnknownIssuer)) => {
                        eprintln!("Cumulocity certificate is not trusted by the device. Check your 'c8y.root_cert_path'.");
                    }
                    _ => {
                        eprintln!("ERROR: {:?}", err);
                    }
                }
                return Err(ConnectError::ConnectionCheckError);
            }
            Err(err) => {
                eprintln!("ERROR: {:?}", err);
                return Err(ConnectError::ConnectionCheckError);
            }
            _ => {}
        }
    }

    // The request has not even been sent
    println!("No response from Cumulocity");
    Err(ConnectError::TimeoutElapsedError)
}

fn publish_device_create_message(
    client: &mut Client,
    device_id: &str,
    device_type: &str,
) -> Result<(), ConnectError> {
    const DEVICE_CREATE_PUBLISH_TOPIC: &str = "s/us";
    client.publish(
        DEVICE_CREATE_PUBLISH_TOPIC,
        QoS::ExactlyOnce,
        false,
        format!("100,{},{}", device_id, device_type).as_bytes(),
    )?;
    Ok(())
}
