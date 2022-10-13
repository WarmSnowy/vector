use std::{io, sync::Arc};

use bytes::Bytes;
use codecs::JsonSerializer;
use lookup::lookup_v2::OwnedSegment;
use vector_common::metadata::{MetaDescriptive, RequestMetadata};
use vector_core::ByteSizeOf;

use crate::{
    codecs::{Encoder, TimestampFormat, Transformer},
    event::{Event, EventFinalizers, Finalizable},
    sinks::util::{
        metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression, ElementCount,
        RequestBuilder,
    },
};

#[derive(Clone)]
pub struct DatadogEventsRequest {
    pub body: Bytes,
    pub metadata: Metadata,
    request_metadata: RequestMetadata,
}

impl Finalizable for DatadogEventsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

impl ByteSizeOf for DatadogEventsRequest {
    fn allocated_bytes(&self) -> usize {
        self.body.allocated_bytes() + self.metadata.finalizers.allocated_bytes()
    }
}

impl ElementCount for DatadogEventsRequest {
    fn element_count(&self) -> usize {
        // Datadog Events api only accepts a single event per request
        1
    }
}

impl MetaDescriptive for DatadogEventsRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }
}

#[derive(Clone)]
pub struct Metadata {
    pub finalizers: EventFinalizers,
    pub api_key: Option<Arc<str>>,
}

pub struct DatadogEventsRequestBuilder {
    encoder: (Transformer, Encoder<()>),
}

impl Default for DatadogEventsRequestBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DatadogEventsRequestBuilder {
    pub fn new() -> DatadogEventsRequestBuilder {
        DatadogEventsRequestBuilder { encoder: encoder() }
    }
}

impl RequestBuilder<Event> for DatadogEventsRequestBuilder {
    type Metadata = (Metadata, RequestMetadataBuilder);
    type Events = Event;
    type Encoder = (Transformer, Encoder<()>);
    type Payload = Bytes;
    type Request = DatadogEventsRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, event: Event) -> (Self::Metadata, Self::Events) {
        let builder = RequestMetadataBuilder::from_events(&event);

        let mut log = event.into_log();
        let metadata = Metadata {
            finalizers: log.take_finalizers(),
            api_key: log.metadata_mut().datadog_api_key(),
        };

        ((metadata, builder), Event::from(log))
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (metadata, builder) = metadata;
        let request_metadata = builder.build(&payload);
        DatadogEventsRequest {
            body: payload.into_payload(),
            metadata,
            request_metadata,
        }
    }
}

fn encoder() -> (Transformer, Encoder<()>) {
    // DataDog Event API allows only some fields, and refuses
    // to accept event if it contains any other field.
    let only_fields = Some(
        [
            "aggregation_key",
            "alert_type",
            "date_happened",
            "device_name",
            "host",
            "priority",
            "related_event_id",
            "source_type_name",
            "tags",
            "text",
            "title",
        ]
        .iter()
        .map(|field| vec![OwnedSegment::Field((*field).into())].into())
        .collect(),
    );
    // DataDog Event API requires unix timestamp.
    let timestamp_format = Some(TimestampFormat::Unix);

    (
        Transformer::new(only_fields, None, timestamp_format)
            .expect("transformer configuration must be valid"),
        Encoder::<()>::new(JsonSerializer::new().into()),
    )
}
