use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    fmt::Debug,
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use serde::{Deserialize, Serialize};

use super::NewRelicSinkError;
use crate::event::{Event, MetricKind, MetricValue, Value};

#[derive(Debug)]
pub enum NewRelicApiModel {
    Metrics(MetricsApiModel),
    Events(EventsApiModel),
    Logs(LogsApiModel),
}

type KeyValData = HashMap<String, Value>;
type DataStore = HashMap<String, Vec<KeyValData>>;

#[derive(Serialize, Deserialize, Debug)]
pub struct MetricsApiModel(pub Vec<DataStore>);

impl MetricsApiModel {
    pub fn new(metric_array: Vec<KeyValData>) -> Self {
        let mut metric_store = DataStore::new();
        metric_store.insert("metrics".to_owned(), metric_array);
        Self(vec![metric_store])
    }
}

impl TryFrom<Vec<Event>> for MetricsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut metric_array = vec![];

        for buf_event in buf_events {
            if let Event::Metric(metric) = buf_event {
                // Generate Value::Object() from BTreeMap<String, String>
                let (series, data, _) = metric.into_parts();
                let attr = series.tags.map(|tags| {
                    Value::from(tags
                        .into_iter()
                        .map(|(key, value)| (key, Value::from(value)))
                        .collect::<BTreeMap<_, _>())
                });

                let mut metric_data = KeyValData::new();

                if let MetricValue::Gauge { value } | MetricValue::Counter { value } = data.value {
                    metric_data.insert("name".to_owned(), Value::from(series.name.name));
                    metric_data.insert(
                        "value".to_owned(),
                        Value::from(
                            NotNan::new(value)
                                .map_err(|_| NewRelicSinkError::new("NaN value not supported"))?,
                        ),
                    );
                    metric_data.insert(
                        "timestamp".to_owned(),
                        if let Some(ts) = data.time.timestamp {
                            Value::from(ts.timestamp())
                        } else {
                            Value::from(DateTime::<Utc>::from(SystemTime::now()).timestamp())
                        },
                    );
                    if let Some(attr) = attr {
                        metric_data.insert("attributes".to_owned(), attr);
                    }
                }

                match (data.value, data.kind) {
                    (MetricValue::Counter { .. }, MetricKind::Incremental) => {
                        if let Some(interval_ms) = data.time.interval_ms {
                            metric_data.insert(
                                "interval.ms".to_owned(),
                                Value::from(interval_ms.get() as i64),
                            );
                        } else {
                            // Incremental counter without an interval is worthless, skip this metric
                            continue;
                        }
                        metric_data.insert("type".to_owned(), Value::from("count"));
                    }
                    (
                        MetricValue::Gauge { .. } | MetricValue::Counter { .. },
                        MetricKind::Absolute,
                    ) => {
                        metric_data.insert("type".to_owned(), Value::from("gauge"));
                    }
                    _ => {}
                }

                metric_array.push(metric_data);
            }
        }

        if !metric_array.is_empty() {
            Ok(Self::new(metric_array))
        } else {
            Err(NewRelicSinkError::new("No valid metrics to generate"))
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EventsApiModel(pub Vec<KeyValData>);

impl EventsApiModel {
    pub fn new(events_array: Vec<KeyValData>) -> Self {
        Self(events_array)
    }
}

impl TryFrom<Vec<Event>> for EventsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut events_array = vec![];
        for buf_event in buf_events {
            if let Event::Log(log) = buf_event {
                let mut event_model = KeyValData::new();
                for (k, v) in log.convert_to_fields() {
                    event_model.insert(k, v.clone());
                }

                if let Some(message) = log.get("message") {
                    let message = message.to_string_lossy().replace("\\\"", "\"");
                    // If message contains a JSON string, parse it and insert all fields into self
                    if let serde_json::Result::Ok(json_map) =
                        serde_json::from_str::<HashMap<String, serde_json::Value>>(&message)
                    {
                        for (k, v) in json_map {
                            match v {
                                serde_json::Value::String(s) => {
                                    event_model.insert(k, Value::from(s));
                                }
                                serde_json::Value::Number(n) => {
                                    if let Some(f) = n.as_f64() {
                                        event_model.insert(
                                            k,
                                            Value::from(NotNan::new(f).map_err(|_| {
                                                NewRelicSinkError::new("NaN value not supported")
                                            })?),
                                        );
                                    } else {
                                        event_model.insert(k, Value::from(n.as_i64()));
                                    }
                                }
                                serde_json::Value::Bool(b) => {
                                    event_model.insert(k, Value::from(b));
                                }
                                _ => {}
                            }
                        }
                        event_model.remove("message");
                    }
                }

                if event_model.get("eventType").is_none() {
                    event_model
                        .insert("eventType".to_owned(), Value::from("VectorSink".to_owned()));
                }

                events_array.push(event_model);
            }
        }

        if !events_array.is_empty() {
            Ok(Self::new(events_array))
        } else {
            Err(NewRelicSinkError::new("No valid events to generate"))
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LogsApiModel(pub Vec<DataStore>);

impl LogsApiModel {
    pub fn new(logs_array: Vec<KeyValData>) -> Self {
        let mut logs_store = DataStore::new();
        logs_store.insert("logs".to_owned(), logs_array);
        Self(vec![logs_store])
    }
}

impl TryFrom<Vec<Event>> for LogsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut logs_array = vec![];
        for buf_event in buf_events {
            if let Event::Log(log) = buf_event {
                let mut log_model = KeyValData::new();
                for (k, v) in log.convert_to_fields() {
                    log_model.insert(k, v.clone());
                }
                if log.get("message").is_none() {
                    log_model.insert(
                        "message".to_owned(),
                        Value::from("log from vector".to_owned()),
                    );
                }
                logs_array.push(log_model);
            }
        }

        if !logs_array.is_empty() {
            Ok(Self::new(logs_array))
        } else {
            Err(NewRelicSinkError::new("No valid logs to generate"))
        }
    }
}
