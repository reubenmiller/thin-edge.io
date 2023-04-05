use async_trait::async_trait;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::smartrest_deserializer::SmartRestFirmwareRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::handle::C8YHttpProxy;
use mqtt_channel::Topic;
use nanoid::nanoid;
use sha256::digest;
use sha256::try_digest;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::Sender;
use tedge_actors::WrappedInput;
use tedge_api::OperationStatus;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_mqtt_ext::MqttMessage;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;
use tedge_utils::file::move_file;
use tedge_utils::file::PermissionEntry;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::config::FirmwareManagerConfig;
use crate::error::FirmwareManagementError;
use crate::message::get_child_id_from_child_topic;
use crate::message::DownloadFirmwareStatusMessage;
use crate::message::FirmwareOperationRequest;
use crate::message::FirmwareOperationResponse;
use crate::operation::ActiveOperationState;
use crate::operation::FirmwareOperationEntry;
use crate::operation::OperationKey;

pub type OperationSetTimeout = SetTimeout<OperationKey>;
pub type OperationTimeout = Timeout<OperationKey>;

pub type IdDownloadResult = (String, DownloadResult);
pub type IdDownloadRequest = (String, DownloadRequest);

fan_in_message_type!(FirmwareInput[MqttMessage, OperationTimeout, IdDownloadResult] : Debug);
fan_in_message_type!(FirmwareOutput[MqttMessage, OperationSetTimeout, IdDownloadRequest] : Debug);

pub struct FirmwareManagerActor {
    config: FirmwareManagerConfig,
    active_child_ops: HashMap<OperationKey, ActiveOperationState>,
    reqs_pending_download: HashMap<String, SmartRestFirmwareRequest>,
    message_box: FirmwareManagerMessageBox,
}

impl FirmwareManagerActor {
    pub fn new(config: FirmwareManagerConfig, message_box: FirmwareManagerMessageBox) -> Self {
        Self {
            config,
            active_child_ops: HashMap::new(),
            reqs_pending_download: HashMap::new(),
            message_box,
        }
    }

    pub async fn process_mqtt_message(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), FirmwareManagementError> {
        if self.config.c8y_request_topics.accept(&message) {
            self.handle_firmware_update_smartrest_request(message)
                .await?;
        } else if self.config.firmware_update_response_topics.accept(&message) {
            self.handle_child_device_firmware_operation_response(message.clone())
                .await?;
        } else {
            error!(
                "Received unexpected message on topic: {}",
                message.topic.name
            );
        }
        Ok(())
    }

    pub async fn handle_firmware_update_smartrest_request(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), FirmwareManagementError> {
        for smartrest_message in collect_smartrest_messages(message.payload_str()?) {
            let result = match get_smartrest_template_id(&smartrest_message).as_str() {
                "515" => match SmartRestFirmwareRequest::from_smartrest(&smartrest_message) {
                    Ok(firmware_request) => {
                        self.handle_firmware_download_request(firmware_request)
                            .await
                    }
                    Err(_) => {
                        error!("Incorrect c8y_Firmware SmartREST payload: {smartrest_message}");
                        Ok(())
                    }
                },
                _ => {
                    // Ignore operation messages not meant for this plugin
                    Ok(())
                }
            };

            if let Err(err) = result {
                error!("Handling of operation: '{smartrest_message}' failed with {err}");
            }
        }
        Ok(())
    }

    async fn handle_firmware_download_request(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> Result<(), FirmwareManagementError> {
        info!(
            "Handling c8y_Firmware operation: device={}, name={}, version={}, url={}",
            smartrest_request.device,
            smartrest_request.name,
            smartrest_request.version,
            smartrest_request.url,
        );

        if smartrest_request.device == self.config.tedge_device_id {
            warn!("c8y-firmware-plugin does not support firmware operation for the main tedge device. \
            Please define a custom operation handler for the c8y_Firmware operation.");
            return Ok(());
        }

        let child_id = smartrest_request.device.as_str();

        if let Err(err) = self
            .validate_same_request_in_progress(smartrest_request.clone())
            .await
        {
            return match err {
                FirmwareManagementError::RequestAlreadyAddressed => {
                    warn!("Skip the received c8y_Firmware operation as the same operation is already in progress.");
                    Ok(())
                }
                _ => {
                    self.fail_operation_in_cloud(child_id, None, &err.to_string())
                        .await?;
                    Err(err)
                }
            };
        }

        let op_id = nanoid!();
        if let Err(err) = self
            .handle_firmware_download_request_child_device(
                smartrest_request.clone(),
                op_id.as_str(),
            )
            .await
        {
            self.fail_operation_in_cloud(child_id, Some(&op_id), &err.to_string())
                .await?;
        }

        Ok(())
    }

    async fn handle_firmware_download_request_child_device(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
        operation_id: &str,
    ) -> Result<(), FirmwareManagementError> {
        let firmware_url = smartrest_request.url.as_str();
        let file_cache_key = digest(firmware_url);
        let cache_file_path = self
            .config
            .validate_and_get_cache_dir_path()?
            .join(&file_cache_key);

        if cache_file_path.is_file() {
            info!(
                "Hit the file cache={}. File download is skipped.",
                cache_file_path.display()
            );
            self.handle_firmware_update_request_with_downloaded_file(
                smartrest_request,
                operation_id,
                &cache_file_path,
            )
            .await?;
        } else {
            info!(
                "Awaiting firmware download for op_id: {} from url: {}",
                operation_id, firmware_url
            );

            // TODO: JWT token
            // If url_is_in_my_tenant_domain
            // let auth = if false {
            //     let client_message_box = self.message_box.c8y_http_proxy.get_client_message_box();
            //     let jwt_token = client_message_box.await_response(C8YR).await?;
            //     Some(jwt_token)
            // } else {
            //     None
            // };

            // Send a request to the DownloadManager to download the file asynchronously
            let download_request = DownloadRequest::new(firmware_url, &cache_file_path, None);

            self.message_box
                .download_sender
                .send((operation_id.to_string(), download_request))
                .await?;
            self.reqs_pending_download
                .insert(operation_id.to_string(), smartrest_request);
        }
        Ok(())
    }

    async fn process_after_download(
        &mut self,
        operation_id: &str,
        download_result: DownloadResult,
    ) -> Result<(), FirmwareManagementError> {
        if let Some(smartrest_request) = self.reqs_pending_download.remove(operation_id) {
            let child_id = smartrest_request.device.clone();
            match download_result {
                Ok(response) => {
                    if let Err(err) = self
                        .handle_firmware_update_request_with_downloaded_file(
                            smartrest_request,
                            operation_id,
                            &response.file_path,
                        )
                        .await
                    {
                        self.fail_operation_in_cloud(
                            &child_id,
                            Some(operation_id),
                            &err.to_string(),
                        )
                        .await?;
                    }
                }
                Err(err) => {
                    let firmware_url = smartrest_request.url;
                    let failure_reason = format!("Download from {firmware_url} failed with {err}");
                    self.fail_operation_in_cloud(&child_id, Some(operation_id), &failure_reason)
                        .await?;
                }
            }
        } else {
            error!("Unexpected: Download completed for unknown operation: {operation_id}");
        }
        Ok(())
    }

    async fn handle_firmware_update_request_with_downloaded_file(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
        operation_id: &str,
        downloaded_firmware: &Path,
    ) -> Result<(), FirmwareManagementError> {
        let child_id = smartrest_request.device.as_str();
        let firmware_url = smartrest_request.url.as_str();
        let file_cache_key = digest(firmware_url);
        let cache_dir_path = self.config.validate_and_get_cache_dir_path()?;
        let cache_file_path = cache_dir_path.join(&file_cache_key);

        // If the downloaded firmware is not already in the cache, move it there
        if !downloaded_firmware.starts_with(&cache_dir_path) {
            move_file(
                &downloaded_firmware,
                &cache_file_path,
                PermissionEntry::new(None, None, None),
            )
            .await?;
        }

        let symlink_path =
            self.create_file_transfer_symlink(child_id, &file_cache_key, &cache_file_path)?;
        let file_transfer_url = format!(
            "http://{}/tedge/file-transfer/{child_id}/firmware_update/{file_cache_key}",
            &self.config.local_http_host
        );
        let file_sha256 = try_digest(symlink_path.as_path())?;

        let operation_entry = FirmwareOperationEntry {
            operation_id: operation_id.to_string(),
            child_id: child_id.to_string(),
            name: smartrest_request.name.to_string(),
            version: smartrest_request.version.to_string(),
            server_url: firmware_url.to_string(),
            file_transfer_url: file_transfer_url.clone(),
            sha256: file_sha256.to_string(),
            attempt: 1,
        };

        operation_entry.create_status_file(&self.config.firmware_dir)?;

        self.publish_firmware_update_request(operation_entry)
            .await?;

        let operation_key = OperationKey::new(child_id, operation_id);
        self.active_child_ops
            .insert(operation_key.clone(), ActiveOperationState::Pending);

        // Start timer
        self.message_box
            .send(SetTimeout::new(self.config.timeout_sec, operation_key).into())
            .await?;

        Ok(())
    }

    async fn handle_child_device_firmware_operation_response(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), FirmwareManagementError> {
        let child_id = get_child_id_from_child_topic(&message.topic.name)?;

        match FirmwareOperationResponse::try_from(&message) {
            Ok(response) => {
                if let Err(err) = self
                    .handle_child_device_firmware_update_response(&response)
                    .await
                {
                    self.fail_operation_in_cloud(
                        &child_id,
                        Some(response.get_payload().operation_id.as_str()),
                        &err.to_string(),
                    )
                    .await?;
                }
            }
            Err(err) => {
                // Ignore bad responses. Eventually, timeout will fail an operation.
                error!("Received a firmware update response with invalid payload for child {child_id}. Error: {err}");
            }
        }
        Ok(())
    }

    async fn handle_child_device_firmware_update_response(
        &mut self,
        response: &FirmwareOperationResponse,
    ) -> Result<(), FirmwareManagementError> {
        let child_device_payload = response.get_payload();
        let child_id = response.get_child_id();
        let operation_id = child_device_payload.operation_id.as_str();
        let received_status = child_device_payload.status;
        info!("Firmware update response received. Details: id={operation_id}, child={child_id}, status={received_status:?}");

        let operation_key = OperationKey::new(&child_id, operation_id);
        let current_operation_state = self.active_child_ops.get(&operation_key);

        match current_operation_state {
            Some(&ActiveOperationState::Executing) => {}
            Some(&ActiveOperationState::Pending) => {
                self.publish_c8y_executing_message(&child_id).await?;
                self.active_child_ops
                    .insert(operation_key.clone(), ActiveOperationState::Executing);
            }
            None => {
                info!("Received a response from {child_id} for unknown request {operation_id}.");
                return Ok(());
            }
        }

        match received_status {
            OperationStatus::Successful => {
                let status_file_path = self.config.firmware_dir.join(operation_id);
                let operation_entry =
                    FirmwareOperationEntry::read_from_file(status_file_path.as_path())?;

                self.publish_c8y_installed_firmware_message(&operation_entry)
                    .await?;
                self.publish_c8y_successful_message(&child_id).await?;

                self.remove_status_file(operation_id)?;
                self.remove_entry_from_active_operations(&operation_key);
            }
            OperationStatus::Failed => {
                self.publish_c8y_failed_message(
                    &child_id,
                    "No failure reason provided by child device.",
                )
                .await?;
                self.remove_status_file(operation_id)?;
                self.remove_entry_from_active_operations(&operation_key);
            }
            OperationStatus::Executing => {
                // Starting timer again means extending the timer.
                self.message_box
                    .send(SetTimeout::new(self.config.timeout_sec, operation_key).into())
                    .await?;
            }
        }

        Ok(())
    }

    async fn process_operation_timeout(
        &mut self,
        timeout: OperationTimeout,
    ) -> Result<(), FirmwareManagementError> {
        let child_id = timeout.event.child_id;
        let operation_id = timeout.event.operation_id;

        if let Some(_operation_state) = self
            .active_child_ops
            .get(&OperationKey::new(&child_id, &operation_id))
        {
            self.fail_operation_in_cloud(
                &child_id,
                Some(&operation_id),
                &format!("Child device {child_id} did not respond within the timeout interval of {}sec. Operation ID={operation_id}", self.config.timeout_sec.as_secs()),
            ).await
        } else {
            // Ignore the timeout as the operation has already completed.
            Ok(())
        }
    }

    // This function can be removed once we start using operation ID from c8y.
    async fn validate_same_request_in_progress(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> Result<(), FirmwareManagementError> {
        let firmware_dir_path = self.config.validate_and_get_firmware_dir_path()?;

        for entry in fs::read_dir(firmware_dir_path.clone())? {
            match entry {
                Ok(file_path) => match FirmwareOperationEntry::read_from_file(&file_path.path()) {
                    Ok(recorded_entry) => {
                        if recorded_entry.child_id == smartrest_request.device
                            && recorded_entry.name == smartrest_request.name
                            && recorded_entry.version == smartrest_request.version
                            && recorded_entry.server_url == smartrest_request.url
                        {
                            info!("The same operation as the received c8y_Firmware operation is already in progress.");

                            // Resend a firmware request with incremented attempt.
                            let new_operation_entry = recorded_entry.increment_attempt();
                            let operation_key = OperationKey::new(
                                &new_operation_entry.child_id,
                                &new_operation_entry.operation_id,
                            );

                            new_operation_entry.overwrite_file(&firmware_dir_path)?;
                            self.publish_firmware_update_request(new_operation_entry)
                                .await?;
                            // Add operation to hashmap
                            self.active_child_ops
                                .insert(operation_key.clone(), ActiveOperationState::Pending);
                            // Start timer
                            self.message_box
                                .send(
                                    SetTimeout::new(self.config.timeout_sec, operation_key).into(),
                                )
                                .await?;

                            return Err(FirmwareManagementError::RequestAlreadyAddressed);
                        }
                    }
                    Err(err) => {
                        warn!("Error: {err} while reading the contents of persistent store directory {}",
                            firmware_dir_path.display());
                        continue;
                    }
                },
                Err(err) => {
                    warn!(
                        "Error: {err} while reading the contents of persistent store directory {}",
                        firmware_dir_path.display()
                    );
                    continue;
                }
            }
        }
        Ok(())
    }

    async fn fail_operation_in_cloud(
        &mut self,
        child_id: &str,
        op_id: Option<&str>,
        failure_reason: &str,
    ) -> Result<(), FirmwareManagementError> {
        error!(failure_reason);
        let op_state = if let Some(operation_id) = op_id {
            self.remove_status_file(operation_id)?;
            self.remove_entry_from_active_operations(&OperationKey::new(child_id, operation_id))
        } else {
            ActiveOperationState::Pending
        };

        if op_state == ActiveOperationState::Pending {
            self.publish_c8y_executing_message(child_id).await?;
        }
        self.publish_c8y_failed_message(child_id, failure_reason)
            .await?;

        Ok(())
    }

    async fn resend_operations_to_child_device(&mut self) -> Result<(), FirmwareManagementError> {
        let firmware_dir_path = self.config.firmware_dir.clone();
        if !firmware_dir_path.is_dir() {
            // Do nothing if the persistent store directory does not exist yet.
            return Ok(());
        }

        for entry in fs::read_dir(&firmware_dir_path)? {
            let file_path = entry?.path();
            if file_path.is_file() {
                let operation_entry =
                    FirmwareOperationEntry::read_from_file(&file_path)?.increment_attempt();
                let operation_key =
                    OperationKey::new(&operation_entry.child_id, &operation_entry.operation_id);

                operation_entry.overwrite_file(&firmware_dir_path)?;
                self.publish_firmware_update_request(operation_entry)
                    .await?;
                // Add operation to hashmap
                self.active_child_ops
                    .insert(operation_key.clone(), ActiveOperationState::Pending);
                // Start timer
                self.message_box
                    .send(SetTimeout::new(self.config.timeout_sec, operation_key).into())
                    .await?;
            }
        }
        Ok(())
    }

    fn remove_status_file(&mut self, operation_id: &str) -> Result<(), FirmwareManagementError> {
        let status_file_path = self
            .config
            .validate_and_get_firmware_dir_path()?
            .join(operation_id);
        if status_file_path.exists() {
            fs::remove_file(status_file_path)?;
        }
        Ok(())
    }

    async fn publish_firmware_update_request(
        &mut self,
        operation_entry: FirmwareOperationEntry,
    ) -> Result<(), FirmwareManagementError> {
        let mqtt_message: MqttMessage =
            FirmwareOperationRequest::from(operation_entry.clone()).try_into()?;
        self.message_box.send(mqtt_message.into()).await?;
        info!(
            "Firmware update request is sent. operation_id={}, child={}",
            operation_entry.operation_id, operation_entry.child_id
        );
        Ok(())
    }

    async fn publish_c8y_executing_message(
        &mut self,
        child_id: &str,
    ) -> Result<(), FirmwareManagementError> {
        let c8y_child_topic = Topic::new_unchecked(
            &C8yTopic::ChildSmartRestResponse(child_id.to_string()).to_string(),
        );
        let executing_msg = MqttMessage::new(
            &c8y_child_topic,
            DownloadFirmwareStatusMessage::status_executing()?,
        );
        self.message_box.send(executing_msg.into()).await?;
        Ok(())
    }

    async fn publish_c8y_successful_message(
        &mut self,
        child_id: &str,
    ) -> Result<(), FirmwareManagementError> {
        let c8y_child_topic = Topic::new_unchecked(
            &C8yTopic::ChildSmartRestResponse(child_id.to_string()).to_string(),
        );
        let successful_msg = MqttMessage::new(
            &c8y_child_topic,
            DownloadFirmwareStatusMessage::status_successful(None)?,
        );
        self.message_box.send(successful_msg.into()).await?;
        Ok(())
    }

    async fn publish_c8y_failed_message(
        &mut self,
        child_id: &str,
        failure_reason: &str,
    ) -> Result<(), FirmwareManagementError> {
        let c8y_child_topic = Topic::new_unchecked(
            &C8yTopic::ChildSmartRestResponse(child_id.to_string()).to_string(),
        );
        let failed_msg = MqttMessage::new(
            &c8y_child_topic,
            DownloadFirmwareStatusMessage::status_failed(failure_reason.to_string())?,
        );
        self.message_box.send(failed_msg.into()).await?;
        Ok(())
    }

    async fn publish_c8y_installed_firmware_message(
        &mut self,
        operation_entry: &FirmwareOperationEntry,
    ) -> Result<(), FirmwareManagementError> {
        let c8y_child_topic = Topic::new_unchecked(
            &C8yTopic::ChildSmartRestResponse(operation_entry.child_id.clone()).to_string(),
        );
        let installed_firmware_payload = format!(
            "115,{},{},{}",
            operation_entry.name, operation_entry.version, operation_entry.server_url
        );
        let installed_firmware_message =
            MqttMessage::new(&c8y_child_topic, installed_firmware_payload);
        self.message_box
            .send(installed_firmware_message.into())
            .await?;
        Ok(())
    }

    // TODO: What "key not found" mean?
    fn remove_entry_from_active_operations(
        &mut self,
        operation_key: &OperationKey,
    ) -> ActiveOperationState {
        if let Some(operation_state) = self.active_child_ops.remove(operation_key) {
            operation_state
        } else {
            ActiveOperationState::Pending
        }
    }

    /// The symlink path should be <tedge-data-dir>/file-transfer/<child-id>/firmware_update/<file_cache_key>
    fn create_file_transfer_symlink(
        &self,
        child_id: &str,
        file_cache_key: &str,
        original_file_path: &Path,
    ) -> Result<PathBuf, FirmwareManagementError> {
        let file_transfer_dir_path = self.config.validate_and_get_file_transfer_dir_path()?;

        let symlink_dir_path = file_transfer_dir_path
            .join(child_id)
            .join("firmware_update");
        let symlink_path = symlink_dir_path.join(file_cache_key);

        if !symlink_path.is_symlink() {
            fs::create_dir_all(symlink_dir_path)?;
            unix_fs::symlink(original_file_path, &symlink_path)?;
        }
        Ok(symlink_path)
    }

    // Candidate to be removed since another actor should be in charge of this.
    async fn get_pending_operations_from_cloud(&mut self) -> Result<(), FirmwareManagementError> {
        let message = MqttMessage::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
        self.message_box.send(message.into()).await?;
        Ok(())
    }
}

#[async_trait]
impl Actor for FirmwareManagerActor {
    fn name(&self) -> &str {
        "FirmwareManager"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        self.resend_operations_to_child_device().await?;
        // TODO: We need a dedicated actor to publish 500 later.
        self.get_pending_operations_from_cloud().await?;

        info!("Ready to serve the firmware request.");
        while let Some(event) = self.message_box.recv().await {
            match event {
                FirmwareInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await?;
                }
                FirmwareInput::OperationTimeout(timeout) => {
                    self.process_operation_timeout(timeout).await?;
                }
                FirmwareInput::IdDownloadResult((id, result)) => {
                    self.process_after_download(&id, result).await?
                }
            }
        }
        Ok(())
    }
}

pub struct FirmwareManagerMessageBox {
    input_receiver: LoggingReceiver<FirmwareInput>,
    mqtt_publisher: DynSender<MqttMessage>,
    c8y_http_proxy: C8YHttpProxy,
    timer_sender: DynSender<SetTimeout<OperationKey>>,
    download_sender: DynSender<IdDownloadRequest>,
}

impl FirmwareManagerMessageBox {
    pub fn new(
        input_receiver: LoggingReceiver<FirmwareInput>,
        mqtt_publisher: DynSender<MqttMessage>,
        c8y_http_proxy: C8YHttpProxy,
        timer_sender: DynSender<SetTimeout<OperationKey>>,
        download_sender: DynSender<IdDownloadRequest>,
    ) -> Self {
        Self {
            input_receiver,
            mqtt_publisher,
            c8y_http_proxy,
            timer_sender,
            download_sender,
        }
    }

    async fn send(&mut self, message: FirmwareOutput) -> Result<(), ChannelError> {
        match message {
            FirmwareOutput::MqttMessage(message) => self.mqtt_publisher.send(message).await,
            FirmwareOutput::OperationSetTimeout(message) => self.timer_sender.send(message).await,
            FirmwareOutput::IdDownloadRequest(message) => self.download_sender.send(message).await,
        }
    }
}

#[async_trait]
impl MessageReceiver<FirmwareInput> for FirmwareManagerMessageBox {
    async fn try_recv(&mut self) -> Result<Option<FirmwareInput>, RuntimeRequest> {
        self.input_receiver.try_recv().await
    }

    async fn recv_message(&mut self) -> Option<WrappedInput<FirmwareInput>> {
        self.input_receiver.recv_message().await
    }

    async fn recv(&mut self) -> Option<FirmwareInput> {
        self.input_receiver.recv().await
    }
}
