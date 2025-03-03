use download::DownloadInfo;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use tedge_api::alarm::ThinEdgeAlarm;
use tedge_api::alarm::ThinEdgeAlarmData;
use tedge_api::commands::SoftwareListCommand;
use tedge_api::entity::EntityExternalId;
use tedge_api::entity::EntityType;
use tedge_api::event::ThinEdgeEvent;
use tedge_api::Jsonify;
use tedge_api::SoftwareModule;
use time::OffsetDateTime;

const EMPTY_STRING: &str = "";
const DEFAULT_ALARM_SEVERITY: AlarmSeverity = AlarmSeverity::Minor;
const DEFAULT_ALARM_TYPE: &str = "ThinEdgeAlarm";

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct C8yCreateEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<C8yManagedObject>,

    #[serde(rename = "type")]
    pub event_type: String,

    #[serde(with = "time::serde::rfc3339")]
    pub time: OffsetDateTime,

    pub text: String,

    #[serde(flatten)]
    pub extras: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
/// used to retrieve the id of a log event
pub struct C8yEventResponse {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct C8yManagedObject {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalIdResponse {
    managed_object: C8yManagedObject,
    external_id: String,
}

impl InternalIdResponse {
    pub fn new(id: &str, external_id: &str) -> Self {
        InternalIdResponse {
            managed_object: C8yManagedObject { id: id.to_string() },
            external_id: external_id.to_string(),
        }
    }

    pub fn id(&self) -> String {
        self.managed_object.id.clone()
    }
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct C8ySoftwareModuleItem {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub software_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(flatten)]
    pub url: Option<DownloadInfo>,
}

impl Jsonify for C8ySoftwareModuleItem {}

impl From<SoftwareModule> for C8ySoftwareModuleItem {
    fn from(module: SoftwareModule) -> Self {
        let url = if module.url.is_none() {
            Some(EMPTY_STRING.into())
        } else {
            module.url
        };

        Self {
            name: module.name,
            version: module.version,
            software_type: module.module_type.unwrap_or(SoftwareModule::default_type()),
            url,
        }
    }
}

#[derive(Debug, Serialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct C8yUpdateSoftwareListResponse {
    #[serde(rename = "c8y_SoftwareList")]
    c8y_software_list: Option<Vec<C8ySoftwareModuleItem>>,
}

impl Jsonify for C8yUpdateSoftwareListResponse {}

impl From<&SoftwareListCommand> for C8yUpdateSoftwareListResponse {
    fn from(list: &SoftwareListCommand) -> Self {
        let mut new_list: Vec<C8ySoftwareModuleItem> = Vec::new();
        list.modules().into_iter().for_each(|software_module| {
            let c8y_software_module: C8ySoftwareModuleItem = software_module.into();
            new_list.push(c8y_software_module);
        });

        Self {
            c8y_software_list: Some(new_list),
        }
    }
}

impl From<ThinEdgeEvent> for C8yCreateEvent {
    fn from(event: ThinEdgeEvent) -> Self {
        let mut extras = HashMap::new();
        if let Some(source) = event.source {
            update_the_external_source_event(&mut extras, &source);
        }

        match event.data {
            None => Self {
                source: None,
                event_type: event.name.clone(),
                time: OffsetDateTime::now_utc(),
                text: event.name,
                extras,
            },
            Some(event_data) => {
                extras.extend(event_data.extras);

                // If payload contains type, use the value as the event type unless it's empty
                let event_type = match extras.remove("type") {
                    Some(type_from_payload) => match type_from_payload.as_str() {
                        Some(new_type) if !new_type.is_empty() => new_type.to_string(),
                        _ => event.name,
                    },
                    None => event.name,
                };

                Self {
                    source: None,
                    event_type: event_type.clone(),
                    time: event_data.time.unwrap_or_else(OffsetDateTime::now_utc),
                    text: event_data.text.unwrap_or(event_type),
                    extras,
                }
            }
        }
    }
}

impl Jsonify for C8yCreateEvent {}

fn update_the_external_source_event(extras: &mut HashMap<String, Value>, source: &str) {
    let mut value = serde_json::Map::new();
    value.insert("externalId".to_string(), source.into());
    value.insert("type".to_string(), "c8y_Serial".into());
    extras.insert("externalSource".into(), value.into());
}

fn make_c8y_source_fragment(source_name: &str) -> SourceInfo {
    SourceInfo::new(source_name.into(), "c8y_Serial".into())
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    #[serde(rename = "externalId")]
    pub id: String,
    #[serde(rename = "type")]
    pub source_type: String,
}

impl SourceInfo {
    pub fn new(id: String, source_type: String) -> Self {
        Self { id, source_type }
    }
}

/// Internal representation of c8y's alarm model.
#[derive(Debug, PartialEq, Eq)]
pub enum C8yAlarm {
    Create(C8yCreateAlarm),
    Clear(C8yClearAlarm),
}

/// Internal representation of creating an alarm in c8y.
/// Note: text and time are optional for SmartREST, however,
/// mandatory for JSON over MQTT. Hence, here they are mandatory.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct C8yCreateAlarm {
    /// Alarm type, default is "ThinEdgeAlarm".
    #[serde(rename = "type")]
    pub alarm_type: String,

    /// None for main device, Some for child device.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "externalSource")]
    pub source: Option<SourceInfo>,

    pub severity: AlarmSeverity,

    pub text: String,

    #[serde(with = "time::serde::rfc3339")]
    pub time: OffsetDateTime,

    #[serde(flatten)]
    pub fragments: HashMap<String, Value>,
}

/// Internal representation of clearing an alarm in c8y.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct C8yClearAlarm {
    /// Alarm type, default is "ThinEdgeAlarm".
    #[serde(rename = "type")]
    pub alarm_type: String,

    /// None for main device, Some for child device.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "externalSource")]
    pub source: Option<SourceInfo>,
}

impl C8yAlarm {
    pub fn from(
        alarm: &ThinEdgeAlarm,
        external_id: &EntityExternalId,
        entity_type: &EntityType,
    ) -> Self {
        let source = Self::convert_source(external_id, entity_type);
        let alarm_type = Self::convert_alarm_type(&alarm.alarm_type);

        let c8y_alarm = match alarm.data.as_ref() {
            None => C8yAlarm::Clear(C8yClearAlarm { alarm_type, source }),
            Some(tedge_alarm_data) => C8yAlarm::Create(C8yCreateAlarm {
                alarm_type: alarm_type.clone(),
                source,
                severity: C8yCreateAlarm::convert_severity(tedge_alarm_data),
                text: C8yCreateAlarm::convert_text(tedge_alarm_data, &alarm_type),
                time: C8yCreateAlarm::convert_time(tedge_alarm_data),
                fragments: C8yCreateAlarm::convert_extras(tedge_alarm_data),
            }),
        };
        c8y_alarm
    }

    fn convert_source(
        external_id: &EntityExternalId,
        entity_type: &EntityType,
    ) -> Option<SourceInfo> {
        match entity_type {
            EntityType::MainDevice => None,
            EntityType::ChildDevice => Some(make_c8y_source_fragment(external_id.as_ref())),
            EntityType::Service => Some(make_c8y_source_fragment(external_id.as_ref())),
        }
    }

    fn convert_alarm_type(alarm_type: &str) -> String {
        if alarm_type.is_empty() {
            DEFAULT_ALARM_TYPE.to_string()
        } else {
            alarm_type.to_string()
        }
    }
}

impl C8yCreateAlarm {
    fn convert_severity(alarm_data: &ThinEdgeAlarmData) -> AlarmSeverity {
        match alarm_data.severity.clone() {
            Some(severity) => match AlarmSeverity::try_from(severity.as_str()) {
                Ok(c8y_severity) => c8y_severity,
                Err(_) => DEFAULT_ALARM_SEVERITY,
            },
            None => DEFAULT_ALARM_SEVERITY,
        }
    }

    fn convert_text(alarm_data: &ThinEdgeAlarmData, alarm_type: &str) -> String {
        alarm_data.text.clone().unwrap_or(alarm_type.to_string())
    }

    fn convert_time(alarm_data: &ThinEdgeAlarmData) -> OffsetDateTime {
        alarm_data.time.unwrap_or_else(OffsetDateTime::now_utc)
    }

    /// Remove reserved keywords from extras.
    /// "type", "time", "text", "severity" are ensured that they are not
    /// in the hashmap of ThinEdgeAlarm because they are already members of the struct itself.
    fn convert_extras(alarm_data: &ThinEdgeAlarmData) -> HashMap<String, Value> {
        let mut map = alarm_data.extras.clone();
        map.remove("externalSource");
        map
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all(serialize = "UPPERCASE"))]
pub enum AlarmSeverity {
    Critical,
    Major,
    Minor,
    Warning,
}

impl TryFrom<&str> for AlarmSeverity {
    type Error = C8yAlarmError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "critical" => Ok(AlarmSeverity::Critical),
            "major" => Ok(AlarmSeverity::Major),
            "minor" => Ok(AlarmSeverity::Minor),
            "warning" => Ok(AlarmSeverity::Warning),
            invalid => Err(C8yAlarmError::UnsupportedAlarmSeverity(invalid.into())),
        }
    }
}

impl fmt::Display for AlarmSeverity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AlarmSeverity::Critical => write!(f, "CRITICAL"),
            AlarmSeverity::Major => write!(f, "MAJOR"),
            AlarmSeverity::Minor => write!(f, "MINOR"),
            AlarmSeverity::Warning => write!(f, "WARNING"),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum C8yAlarmError {
    #[error("Unsupported alarm severity in topic: {0}")]
    UnsupportedAlarmSeverity(String),

    #[error("Unsupported device topic ID in topic: {0}")]
    UnsupportedDeviceTopicId(String),
}

#[cfg(test)]
mod tests {
    use crate::json_c8y::AlarmSeverity;
    use anyhow::Result;
    use assert_matches::assert_matches;
    use serde_json::json;
    use tedge_api::alarm::ThinEdgeAlarm;
    use tedge_api::alarm::ThinEdgeAlarmData;
    use tedge_api::commands::SoftwareListCommandPayload;
    use tedge_api::event::ThinEdgeEventData;
    use tedge_api::mqtt_topics::EntityTopicId;
    use test_case::test_case;
    use time::macros::datetime;

    use super::*;

    #[test]
    fn from_software_module_to_c8y_software_module_item() {
        let software_module = SoftwareModule {
            module_type: Some("a".into()),
            name: "b".into(),
            version: Some("c".into()),
            url: Some("".into()),
            file_path: None,
        };

        let expected_c8y_item = C8ySoftwareModuleItem {
            name: "b".into(),
            version: Some("c".into()),
            software_type: "a".to_string(),
            url: Some("".into()),
        };

        let converted: C8ySoftwareModuleItem = software_module.into();

        assert_eq!(converted, expected_c8y_item);
    }

    #[test]
    fn from_thin_edge_json_to_c8y_set_software_list() {
        let input_json = r#"{
            "id":"1",
            "status":"successful",
            "currentSoftwareList":[ 
                {"type":"debian", "modules":[
                    {"name":"a"},
                    {"name":"b","version":"1.0"},
                    {"name":"c","url":"https://foobar.io/c.deb"},
                    {"name":"d","version":"beta","url":"https://foobar.io/d.deb"}
                ]},
                {"type":"apama","modules":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]}"#;

        let command = SoftwareListCommand {
            target: EntityTopicId::default_main_device(),
            cmd_id: "1".to_string(),
            payload: SoftwareListCommandPayload::from_json(input_json).unwrap(),
        };

        let c8y_software_list: C8yUpdateSoftwareListResponse = (&command).into();

        let expected_struct = C8yUpdateSoftwareListResponse {
            c8y_software_list: Some(vec![
                C8ySoftwareModuleItem {
                    name: "a".into(),
                    version: None,
                    software_type: "debian".to_string(),
                    url: Some("".into()),
                },
                C8ySoftwareModuleItem {
                    name: "b".into(),
                    version: Some("1.0".into()),
                    software_type: "debian".to_string(),
                    url: Some("".into()),
                },
                C8ySoftwareModuleItem {
                    name: "c".into(),
                    version: None,
                    software_type: "debian".to_string(),
                    url: Some("https://foobar.io/c.deb".into()),
                },
                C8ySoftwareModuleItem {
                    name: "d".into(),
                    version: Some("beta".into()),
                    software_type: "debian".to_string(),
                    url: Some("https://foobar.io/d.deb".into()),
                },
                C8ySoftwareModuleItem {
                    name: "m".into(),
                    version: None,
                    software_type: "apama".to_string(),
                    url: Some("https://foobar.io/m.epl".into()),
                },
            ]),
        };

        let expected_json = r#"{"c8y_SoftwareList":[{"name":"a","softwareType":"debian","url":""},{"name":"b","version":"1.0","softwareType":"debian","url":""},{"name":"c","softwareType":"debian","url":"https://foobar.io/c.deb"},{"name":"d","version":"beta","softwareType":"debian","url":"https://foobar.io/d.deb"},{"name":"m","softwareType":"apama","url":"https://foobar.io/m.epl"}]}"#;

        assert_eq!(c8y_software_list, expected_struct);
        assert_eq!(c8y_software_list.to_json(), expected_json);
    }

    #[test]
    fn empty_to_c8y_set_software_list() {
        let input_json = r#"{
            "id":"1",
            "status":"successful",
            "currentSoftwareList":[]
            }"#;

        let command = &SoftwareListCommand {
            target: EntityTopicId::default_main_device(),
            cmd_id: "1".to_string(),
            payload: SoftwareListCommandPayload::from_json(input_json).unwrap(),
        };

        let c8y_software_list: C8yUpdateSoftwareListResponse = command.into();

        let expected_struct = C8yUpdateSoftwareListResponse {
            c8y_software_list: Some(vec![]),
        };
        let expected_json = r#"{"c8y_SoftwareList":[]}"#;

        assert_eq!(c8y_software_list, expected_struct);
        assert_eq!(c8y_software_list.to_json(), expected_json);
    }

    #[test]
    fn get_id_from_c8y_response() {
        let managed_object = C8yManagedObject { id: "12345".into() };
        let response = InternalIdResponse {
            managed_object,
            external_id: "test".into(),
        };

        assert_eq!(response.id(), "12345".to_string());
    }

    #[test_case(
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: Some("Someone clicked".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
            source: None,
        },
        C8yCreateEvent {
            source: None,
            event_type: "click_event".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            text: "Someone clicked".into(),
            extras: HashMap::new(),
        }
        ;"event translation"
    )]
    #[test_case(
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: None,
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
            source: None,
        },
        C8yCreateEvent {
            source: None,
            event_type: "click_event".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            text: "click_event".into(),
            extras: HashMap::new(),
        }
        ;"event translation without text"
    )]
    #[test_case(
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: Some("Someone, clicked, it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
            source: None,
        },
        C8yCreateEvent {
            source: None,
            event_type: "click_event".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            text: "Someone, clicked, it".into(),
            extras: HashMap::new(),
        }
        ;"event translation with commas in text"
    )]
    fn check_event_translation(
        tedge_event: ThinEdgeEvent,
        expected_c8y_event: C8yCreateEvent,
    ) -> Result<()> {
        let actual_c8y_event = C8yCreateEvent::from(tedge_event);

        assert_eq!(expected_c8y_event, actual_c8y_event);

        Ok(())
    }

    #[test]
    fn event_translation_empty_json_payload_generates_timestamp() -> Result<()> {
        let tedge_event = ThinEdgeEvent {
            name: "empty_event".into(),
            data: Some(ThinEdgeEventData {
                text: None,
                time: None,
                extras: HashMap::new(),
            }),
            source: None,
        };

        let actual_c8y_event = C8yCreateEvent::from(tedge_event);

        assert_eq!(actual_c8y_event.event_type, "empty_event".to_string());
        assert_eq!(actual_c8y_event.text, "empty_event".to_string());
        assert_matches!(actual_c8y_event.time, _);
        assert_matches!(actual_c8y_event.source, None);
        assert!(actual_c8y_event.extras.is_empty());

        Ok(())
    }

    #[test]
    fn event_translation_empty_payload() -> Result<()> {
        let tedge_event = ThinEdgeEvent {
            name: "empty_event".into(),
            data: None,
            source: None,
        };

        let actual_c8y_event = C8yCreateEvent::from(tedge_event);

        assert_eq!(actual_c8y_event.event_type, "empty_event".to_string());
        assert_eq!(actual_c8y_event.text, "empty_event".to_string());
        assert!(actual_c8y_event.time <= OffsetDateTime::now_utc());
        assert_matches!(actual_c8y_event.source, None);
        assert!(actual_c8y_event.extras.is_empty());

        Ok(())
    }

    #[test_case(
        ThinEdgeAlarm {
            alarm_type: "temperature alarm".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("critical".into()),
                text: Some("Temperature went high".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
        },
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "temperature alarm".into(),
            source: None,
            severity: AlarmSeverity::Critical,
            text: "Temperature went high".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: HashMap::new(),
        })
        ;"critical alarm translation"
    )]
    #[test_case(
        ThinEdgeAlarm {
            alarm_type: "temperature alarm".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("critical".into()),
                text: Some("Temperature went high".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
            }),
        },
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "temperature alarm".into(),
            source: None,
            severity: AlarmSeverity::Critical,
            text: "Temperature went high".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
        })
        ;"critical alarm translation with custom fragment"
    )]
    #[test_case(
        ThinEdgeAlarm {
            alarm_type: "temperature alarm".into(),
            source: EntityTopicId::default_child_device("external_source").unwrap(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("critical".into()),
                text: Some("Temperature went high".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
            }),
        },
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "temperature alarm".into(),
            source: Some(SourceInfo::new("external_source".to_string(),"c8y_Serial".to_string())),
            severity: AlarmSeverity::Critical,
            text: "Temperature went high".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
        })
        ;"critical alarm translation of child device with custom fragment"
    )]
    #[test_case(
        ThinEdgeAlarm {
            alarm_type: "".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("invalid".into()),
                text: None,
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
        },
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "ThinEdgeAlarm".into(),
            source: None,
            severity: AlarmSeverity::Minor,
            text: "ThinEdgeAlarm".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: HashMap::new(),
        })
        ;"using default values of alarm"
    )]
    #[test_case(
        ThinEdgeAlarm {
            alarm_type: "".into(),
            source: EntityTopicId::default_main_device(),
            data: None,
        },
        C8yAlarm::Clear(C8yClearAlarm {
            alarm_type: "ThinEdgeAlarm".into(),
            source: None,
        })
        ;"convert to clear alarm"
    )]
    fn check_alarm_translation(tedge_alarm: ThinEdgeAlarm, expected_c8y_alarm: C8yAlarm) {
        let (external_id, entity_type) = if tedge_alarm.source.is_default_main_device() {
            ("main_device".into(), EntityType::MainDevice)
        } else {
            ("external_source".into(), EntityType::ChildDevice)
        };

        let actual_c8y_alarm = C8yAlarm::from(&tedge_alarm, &external_id, &entity_type);
        assert_eq!(actual_c8y_alarm, expected_c8y_alarm);
    }

    #[test]
    fn alarm_translation_generates_timestamp_if_not_given() {
        let tedge_alarm = ThinEdgeAlarm {
            alarm_type: "".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("critical".into()),
                text: None,
                time: None,
                extras: HashMap::new(),
            }),
        };
        let external_id = "main".into();

        match C8yAlarm::from(&tedge_alarm, &external_id, &EntityType::MainDevice) {
            C8yAlarm::Create(value) => {
                assert!(value.time.millisecond() > 0);
            }
            C8yAlarm::Clear(_) => panic!("Must be C8yAlarm::Create"),
        };
    }
}
