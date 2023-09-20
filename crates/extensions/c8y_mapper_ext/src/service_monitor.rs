use c8y_api::smartrest::message::sanitize_for_smartrest;
use c8y_api::smartrest::message::MAX_PAYLOAD_LIMIT_IN_BYTES;
use c8y_api::smartrest::topic::SMARTREST_PUBLISH_TOPIC;
use serde::Deserialize;
use serde::Serialize;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::Topic;

const DEFAULT_SERVICE_TYPE: &str = "service";

#[derive(Deserialize, Serialize, Debug)]
pub struct HealthStatus {
    #[serde(rename = "type", default = "default_type")]
    pub service_type: String,

    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "unknown".to_string()
}

fn default_type() -> String {
    "".to_string()
}

pub fn convert_health_status_message(
    entity: &EntityTopicId,
    message: &Message,
    device_name: String,
    default_service_type: String,
) -> Vec<Message> {
    let mut mqtt_messages: Vec<Message> = Vec::new();

    let service_name = entity
        .default_service_name()
        .expect("EntityTopicId should be in default scheme");

    let parent = entity
        .default_parent_identifier()
        .expect("EntityTopicId should be in default scheme");

    let child_id = if parent.is_default_child_device() {
        Some(
            parent
                .default_device_name()
                .expect("EntityTopicId should be in default scheme")
                .to_string(),
        )
    } else {
        None
    };

    let default_health_status = format!("\"type\":{default_service_type},\"status\":\"unknown\"");

    // If not Bridge health status
    if !service_name.contains("bridge") {
        let payload_str = message.payload_str().unwrap_or(&default_health_status);

        let mut health_status =
            serde_json::from_str(payload_str).unwrap_or_else(|_| HealthStatus {
                service_type: default_service_type.clone(),
                status: "unknown".to_string(),
            });

        if health_status.status.is_empty() {
            health_status.status = "unknown".into();
        }

        if health_status.service_type.is_empty() {
            health_status.service_type = if default_service_type.is_empty() {
                DEFAULT_SERVICE_TYPE.to_string()
            } else {
                default_service_type
            };
        }

        let status_message = service_monitor_status_message(
            &device_name,
            service_name,
            &health_status.status,
            &health_status.service_type,
            child_id,
        );

        mqtt_messages.push(status_message);
    }

    mqtt_messages
}

pub fn service_monitor_status_message(
    device_name: &str,
    daemon_name: &str,
    status: &str,
    service_type: &str,
    child_id: Option<String>,
) -> Message {
    let sanitized_status = sanitize_for_smartrest(status.into(), MAX_PAYLOAD_LIMIT_IN_BYTES);
    let sanitized_type = sanitize_for_smartrest(service_type.into(), MAX_PAYLOAD_LIMIT_IN_BYTES);
    match child_id {
        Some(cid) => Message {
            topic: Topic::new_unchecked(&format!("{SMARTREST_PUBLISH_TOPIC}/{cid}")),
            payload: format!(
                "102,{device_name}_{cid}_{daemon_name},\"{sanitized_type}\",{daemon_name},\"{sanitized_status}\""
            )
            .into(),
            qos: tedge_mqtt_ext::QoS::AtLeastOnce,
            retain: false,
        },
        None => Message {
            topic: Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC),
            payload: format!(
                "102,{device_name}_{daemon_name},\"{sanitized_type}\",{daemon_name},\"{sanitized_status}\""
            )
            .into(),
            qos: tedge_mqtt_ext::QoS::AtLeastOnce,
            retain: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_api::mqtt_topics::MqttSchema;
    use test_case::test_case;
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"pid":"1234","type":"systemd","status":"up"}"#,
        "c8y/s/us",
        r#"102,test_device_tedge-mapper-c8y,"systemd",tedge-mapper-c8y,"up""#;  
        "service-monitoring-thin-edge-device"
    )]
    #[test_case(
        "test_device",
        "te/device/child/service/tedge-mapper-c8y/status/health",
        r#"{"pid":"1234","type":"systemd","status":"up"}"#,
        "c8y/s/us/child",
        r#"102,test_device_child_tedge-mapper-c8y,"systemd",tedge-mapper-c8y,"up""#;
        "service-monitoring-thin-edge-child-device"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"pid":"123456","type":"systemd"}"#,
        "c8y/s/us",
        r#"102,test_device_tedge-mapper-c8y,"systemd",tedge-mapper-c8y,"unknown""#;
        "service-monitoring-thin-edge-no-status"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"type":"systemd"}"#,
        "c8y/s/us",
        r#"102,test_device_tedge-mapper-c8y,"systemd",tedge-mapper-c8y,"unknown""#;
        "service-monitoring-thin-edge-no-status-no-pid"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"type":"", "status":""}"#,
        "c8y/s/us",
        r#"102,test_device_tedge-mapper-c8y,"service",tedge-mapper-c8y,"unknown""#;
        "service-monitoring-empty-status-and-type"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        "{}",
        "c8y/s/us",
        r#"102,test_device_tedge-mapper-c8y,"service",tedge-mapper-c8y,"unknown""#;
        "service-monitoring-empty-health-message"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"type":"thin,edge","status":"up,down"}"#,
        "c8y/s/us",
        r#"102,test_device_tedge-mapper-c8y,"thin,edge",tedge-mapper-c8y,"up,down""#;
        "service-monitoring-type-with-comma-health-message"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"type":"thin\"\"edge","status":"up\"down"}"#,
        "c8y/s/us",
        r#"102,test_device_tedge-mapper-c8y,"thin""""edge",tedge-mapper-c8y,"up""down""#;
        "service-monitoring-double-quotes-health-message"
    )]
    fn translate_health_status_to_c8y_service_monitoring_message(
        device_name: &str,
        health_topic: &str,
        health_payload: &str,
        c8y_monitor_topic: &str,
        c8y_monitor_payload: &str,
    ) {
        let topic = Topic::new_unchecked(health_topic);

        let mqtt_schema = MqttSchema::new();
        let (entity, _) = mqtt_schema.entity_channel_of(&topic).unwrap();

        let health_message = Message::new(&topic, health_payload.as_bytes().to_owned());
        let expected_message = Message::new(
            &Topic::new_unchecked(c8y_monitor_topic),
            c8y_monitor_payload.as_bytes(),
        );

        let msg = convert_health_status_message(
            &entity,
            &health_message,
            device_name.into(),
            "service".into(),
        );
        assert_eq!(msg[0], expected_message);
    }
}
