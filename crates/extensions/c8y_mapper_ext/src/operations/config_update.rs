use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use crate::error::CumulocityMapperError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::fail_operation;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_no_payload;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use std::sync::Arc;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::messages::CommandStatus;
use tedge_api::messages::ConfigUpdateCmdPayload;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::Jsonify;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::TopicFilter;
use tracing::log::warn;

pub fn topic_filter(mqtt_schema: &MqttSchema) -> TopicFilter {
    [
        mqtt_schema.topics(
            EntityFilter::AnyEntity,
            ChannelFilter::Command(OperationType::ConfigUpdate),
        ),
        mqtt_schema.topics(
            EntityFilter::AnyEntity,
            ChannelFilter::CommandMetadata(OperationType::ConfigUpdate),
        ),
    ]
    .into_iter()
    .collect()
}

impl CumulocityConverter {
    /// Address a received ThinEdge config_update command. If its status is
    /// - "executing", it converts the message to SmartREST "Executing".
    /// - "successful", it converts the message to SmartREST "Successful".
    /// - "failed", it converts the message to SmartREST "Failed".
    pub async fn handle_config_update_state_change(
        &mut self,
        topic_id: &EntityTopicId,
        _cmd_id: &str,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        if !self.config.capabilities.config_update {
            warn!("Received a config_update command, however, config_update feature is disabled");
            return Ok(vec![]);
        }

        let sm_topic = self.smartrest_publish_topic_for_entity(topic_id)?;
        let payload = message.payload_str()?;
        let response = &ConfigUpdateCmdPayload::from_json(payload)?;

        let messages = match &response.status {
            CommandStatus::Executing => {
                let smartrest_operation_status =
                    set_operation_executing(CumulocitySupportedOperations::C8yDownloadConfigFile);

                vec![Message::new(&sm_topic, smartrest_operation_status)]
            }
            CommandStatus::Successful => {
                let smartrest_operation_status = succeed_operation_no_payload(
                    CumulocitySupportedOperations::C8yDownloadConfigFile,
                );
                let c8y_notification = Message::new(&sm_topic, smartrest_operation_status);
                let clear_local_cmd = Message::new(&message.topic, "")
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);

                vec![c8y_notification, clear_local_cmd]
            }
            CommandStatus::Failed { reason } => {
                let smartrest_operation_status =
                    fail_operation(CumulocitySupportedOperations::C8yDownloadConfigFile, reason);
                let c8y_notification = Message::new(&sm_topic, smartrest_operation_status);
                let clear_local_cmd = Message::new(&message.topic, "")
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);

                vec![c8y_notification, clear_local_cmd]
            }
            _ => {
                vec![] // Do nothing as other components might handle those states
            }
        };

        Ok(messages)
    }

    /// Upon receiving a SmartREST c8y_DownloadConfigFile request, convert it to a message on the
    /// command channel.
    pub async fn convert_config_update_request(
        &mut self,
        smartrest: &str,
    ) -> Result<Vec<Message>, CumulocityMapperError> {
        let smartrest = SmartRestConfigDownloadRequest::from_smartrest(smartrest)?;
        let target = self
            .entity_store
            .try_get_by_external_id(&smartrest.device.clone().into())?;

        let cmd_id = self.command_id.new_id();

        let message = self.create_config_update_cmd(cmd_id.into(), &smartrest, target);
        Ok(message)
    }

    /// Converts a config_update metadata message to
    /// - supported operation "c8y_DownloadConfigFile"
    /// - supported config types
    pub fn convert_config_update_metadata(
        &mut self,
        topic_id: &EntityTopicId,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        if !self.config.capabilities.config_update {
            warn!("Received config_update metadata, however, config_update feature is disabled");
            return Ok(vec![]);
        }
        self.convert_config_metadata(topic_id, message, "c8y_DownloadConfigFile")
    }

    fn create_config_update_cmd(
        &self,
        cmd_id: Arc<str>,
        smartrest: &SmartRestConfigDownloadRequest,
        target: &EntityMetadata,
    ) -> Vec<Message> {
        let channel = Channel::Command {
            operation: OperationType::ConfigUpdate,
            cmd_id: cmd_id.to_string(),
        };
        let topic = self.mqtt_schema.topic_for(&target.topic_id, &channel);

        let proxy_url = self
            .c8y_endpoint
            .maybe_tenant_url(&smartrest.url)
            .map(|cumulocity_url| self.auth_proxy.proxy_url(cumulocity_url).into());

        let remote_url = proxy_url.unwrap_or(smartrest.url.to_string());

        let request = ConfigUpdateCmdPayload {
            status: CommandStatus::Init,
            tedge_url: None,
            remote_url,
            config_type: smartrest.config_type.clone(),
            path: None,
        };

        // Command messages must be retained
        vec![Message::new(&topic, request.to_json()).with_retain()]
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor;
    use c8y_api::smartrest::topic::C8yTopic;
    use serde_json::json;
    use sha256::digest;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_api::mqtt_topics::Channel;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_api::mqtt_topics::OperationType;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

    #[tokio::test]
    async fn mapper_converts_smartrest_config_download_req_with_new_download_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_DownloadConfigFile SmartREST request
        mqtt.send(MqttMessage::new(
            &C8yTopic::downstream_topic(),
            "524,test-device,http://www.my.url,path/config/A",
        ))
        .await
        .expect("Send failed");

        // New config_update command should be published
        let (topic, received_json) = mqtt
            .recv()
            .await
            .map(|msg| {
                (
                    msg.topic,
                    serde_json::from_str::<serde_json::Value>(msg.payload.as_str().expect("UTF8"))
                        .expect("JSON"),
                )
            })
            .unwrap();

        let mqtt_schema = MqttSchema::default();
        let (entity, channel) = mqtt_schema.entity_channel_of(&topic).unwrap();
        assert_eq!(entity, "device/main//");

        let Channel::Command {
            operation: OperationType::ConfigUpdate,
            ..
        } = channel
        else {
            panic!("Unexpected response on channel: {:?}", topic)
        };

        // Validate the payload JSON
        let expected_json = json!({
            "status": "init",
            "remoteUrl": "http://www.my.url",
            "type": "path/config/A",
        });
        assert_json_diff::assert_json_include!(actual: received_json, expected: expected_json);
    }

    #[tokio::test]
    async fn mapper_converts_smartrest_config_download_req_for_child_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(2).await; // Skip child device registration messages

        // Cache is already available
        ttd.dir("cache").file(&digest("http://www.my.url"));

        // Simulate c8y_DownloadConfigFile SmartREST request
        mqtt.send(MqttMessage::new(
            &C8yTopic::downstream_topic(),
            "524,child1,http://www.my.url,configA",
        ))
        .await
        .expect("Send failed");

        // New config_update command should be published
        let (topic, received_json) = mqtt
            .recv()
            .await
            .map(|msg| {
                (
                    msg.topic,
                    serde_json::from_str::<serde_json::Value>(msg.payload.as_str().expect("UTF8"))
                        .expect("JSON"),
                )
            })
            .unwrap();

        let mqtt_schema = MqttSchema::default();
        let (entity, channel) = mqtt_schema.entity_channel_of(&topic).unwrap();
        assert_eq!(entity, "device/child1//");

        let Channel::Command {
            operation: OperationType::ConfigUpdate,
            ..
        } = channel
        else {
            panic!("Unexpected response on channel: {:?}", topic)
        };

        // Validate the payload JSON
        let expected_json = json!({
            "status": "init",
            "remoteUrl": "http://www.my.url",
            "type": "configA",
        });
        assert_json_diff::assert_json_include!(actual: received_json, expected: expected_json);
    }

    #[tokio::test]
    async fn handle_config_update_executing_and_failed_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_snapshot command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "executing",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_update/typeA-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "typeA",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `501` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_DownloadConfigFile")]).await;

        // Simulate config_update command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "failed",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_update/typeA-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "typeA",
            "reason": "Something went wrong"
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `502` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us",
                "502,c8y_DownloadConfigFile,Something went wrong",
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_config_update_executing_and_failed_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(2).await; // Skip child device registration messages

        // Simulate config_snapshot command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "executing",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/child1/config_update/typeA-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "typeA",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `501` smartrest message on child topic.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us/child1", "501,c8y_DownloadConfigFile")],
        )
        .await;

        // Simulate config_update command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "failed",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/child1/config_update/typeA-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "typeA",
            "reason": "Something went wrong"
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `502` smartrest message on child topic.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/child1",
                "502,c8y_DownloadConfigFile,Something went wrong",
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_config_update_successful_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_update command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_update/path:type:A-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "path/type/A",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `503` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "503,c8y_DownloadConfigFile")]).await;
    }

    #[tokio::test]
    async fn handle_config_update_successful_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(2).await; // Skip child device registration messages

        // Simulate config_update command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/child1/config_update/typeA-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "typeA",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `503` smartrest message on child topic.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us/child1", "503,c8y_DownloadConfigFile")],
        )
        .await;
    }
}
