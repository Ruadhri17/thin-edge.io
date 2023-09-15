use crate::software_manager::actor::SoftwareRequest;
use crate::software_manager::actor::SoftwareResponse;
use crate::tedge_operation_converter::builder::TedgeOperationConverterBuilder;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynError;
use tedge_actors::MessageReceiver;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::messages::CommandStatus;
use tedge_api::messages::RestartCommandPayload;
use tedge_api::messages::SoftwareModuleAction;
use tedge_api::messages::SoftwareModuleItem;
use tedge_api::messages::SoftwareRequestResponseSoftwareList;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::RestartCommand;
use tedge_api::SoftwareListRequest;
use tedge_api::SoftwareListResponse;
use tedge_api::SoftwareUpdateRequest;
use tedge_api::SoftwareUpdateResponse;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

#[tokio::test]
async fn convert_incoming_software_list_request() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let (mut software_box, _restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/main//").await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/commands/req/software/list"),
        r#"{"id": "random"}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListRequest
    software_box
        .assert_received([SoftwareListRequest {
            id: "random".to_string(),
        }])
        .await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_software_update_request() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let (mut software_box, _restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/main//").await?;

    // Simulate SoftwareUpdate MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/commands/req/software/update"),
        r#"{"id":"1234","updateList":[{"type":"debian","modules":[{"name":"debian1","version":"0.0.1","action":"install"}]}]}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Create expected request
    let debian_module1 = SoftwareModuleItem {
        name: "debian1".into(),
        version: Some("0.0.1".into()),
        action: Some(SoftwareModuleAction::Install),
        url: None,
        reason: None,
    };
    let debian_list = SoftwareRequestResponseSoftwareList {
        plugin_type: "debian".into(),
        modules: vec![debian_module1],
    };

    // The output of converter => SoftwareUpdateRequest
    software_box
        .assert_received([SoftwareUpdateRequest {
            id: "1234".to_string(),
            update_list: vec![debian_list],
        }])
        .await;

    Ok(())
}

#[tokio::test]
async fn convert_incoming_restart_request() -> Result<(), DynError> {
    let target_device = "device/child-foo//";

    // Spawn incoming mqtt message converter
    let (_software_box, mut restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter(target_device).await?;

    // Simulate Restart MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked(&format!("te/{target_device}/cmd/restart/random")),
        r#"{"status": "init"}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Assert RestartOperationRequest
    restart_box
        .assert_received([RestartCommand {
            target: target_device.parse()?,
            cmd_id: "random".to_string(),
            payload: RestartCommandPayload {
                status: CommandStatus::Init,
                reason: "".to_string(),
            },
        }])
        .await;

    Ok(())
}

#[tokio::test]
async fn convert_outgoing_software_list_response() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let (mut software_box, _restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/main//").await?;

    // Skip capabilities messages
    mqtt_box.skip(1).await;

    // Simulate SoftwareList response message received.
    let software_list_request = SoftwareListRequest::new_with_id("1234");
    let software_list_response = SoftwareListResponse::new(&software_list_request);
    software_box.send(software_list_response.into()).await?;

    mqtt_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked("tedge/commands/res/software/list"),
            r#"{"id":"1234","status":"executing"}"#,
        )])
        .await;

    Ok(())
}

#[tokio::test]
async fn publish_capabilities_on_start() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let (_software_box, _restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/child//").await?;

    mqtt_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked("te/device/child///cmd/restart"),
            "{}",
        )
        .with_retain()])
        .await;

    Ok(())
}

#[tokio::test]
async fn convert_outgoing_software_update_response() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let (mut software_box, _restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/main//").await?;

    // Skip capabilities messages
    mqtt_box.skip(1).await;

    // Simulate SoftwareUpdate response message received.
    let software_update_request = SoftwareUpdateRequest::new_with_id("1234");
    let software_update_response = SoftwareUpdateResponse::new(&software_update_request);
    software_box.send(software_update_response.into()).await?;

    mqtt_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked("tedge/commands/res/software/update"),
            r#"{"id":"1234","status":"executing"}"#,
        )])
        .await;

    Ok(())
}

#[tokio::test]
async fn convert_outgoing_restart_response() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let (_software_box, mut restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/main//").await?;

    // Skip capabilities messages
    mqtt_box.skip(1).await;

    // Simulate Restart response message received.
    let executing_response = RestartCommand {
        target: EntityTopicId::default_main_device(),
        cmd_id: "abc".to_string(),
        payload: RestartCommandPayload {
            status: CommandStatus::Executing,
            reason: "".to_string(),
        },
    };
    restart_box.send(executing_response).await?;

    let (topic, payload) = mqtt_box
        .recv()
        .await
        .map(|msg| (msg.topic, msg.payload))
        .expect("MqttMessage");
    assert_eq!(topic.name, "te/device/main///cmd/restart/abc");
    assert!(format!("{:?}", payload).contains(r#"status":"executing"#));

    Ok(())
}

async fn spawn_mqtt_operation_converter(
    device_topic_id: &str,
) -> Result<
    (
        TimedMessageBox<SimpleMessageBox<SoftwareRequest, SoftwareResponse>>,
        TimedMessageBox<SimpleMessageBox<RestartCommand, RestartCommand>>,
        TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>,
    ),
    DynError,
> {
    let mut software_builder: SimpleMessageBoxBuilder<SoftwareRequest, SoftwareResponse> =
        SimpleMessageBoxBuilder::new("Software", 5);
    let mut restart_builder: SimpleMessageBoxBuilder<RestartCommand, RestartCommand> =
        SimpleMessageBoxBuilder::new("Restart", 5);
    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);

    let converter_actor_builder = TedgeOperationConverterBuilder::new(
        "te",
        device_topic_id.parse().expect("Invalid topic id"),
        &mut software_builder,
        &mut restart_builder,
        &mut mqtt_builder,
    );

    let software_box = software_builder.build().with_timeout(TEST_TIMEOUT_MS);
    let restart_box = restart_builder.build().with_timeout(TEST_TIMEOUT_MS);
    let mqtt_message_box = mqtt_builder.build().with_timeout(TEST_TIMEOUT_MS);

    let mut converter_actor = converter_actor_builder.build();
    tokio::spawn(async move { converter_actor.run().await });

    Ok((software_box, restart_box, mqtt_message_box))
}
