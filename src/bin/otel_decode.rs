use aiki::editors::codex::otel::{ExportLogsServiceRequest, KeyValue, Resource};
use aiki::editors::codex::otel::{InstrumentationScope, LogRecord};
use prost::Message;
use std::collections::BTreeSet;
use std::env;
use std::fs;

#[derive(Clone, PartialEq, Message)]
struct ExportTraceServiceRequest {
    #[prost(message, repeated, tag = "1")]
    pub resource_spans: Vec<ResourceSpans>,
}

#[derive(Clone, PartialEq, Message)]
struct ResourceSpans {
    #[prost(message, optional, tag = "1")]
    pub resource: Option<Resource>,
    #[prost(message, repeated, tag = "2")]
    pub scope_spans: Vec<ScopeSpans>,
}

#[derive(Clone, PartialEq, Message)]
struct ScopeSpans {
    #[prost(message, optional, tag = "1")]
    pub scope: Option<InstrumentationScope>,
    #[prost(message, repeated, tag = "2")]
    pub spans: Vec<Span>,
}

#[derive(Clone, PartialEq, Message)]
struct Span {
    #[prost(string, tag = "5")]
    pub name: String,
    #[prost(message, repeated, tag = "9")]
    pub attributes: Vec<KeyValue>,
    #[prost(message, repeated, tag = "11")]
    pub events: Vec<Event>,
}

#[derive(Clone, PartialEq, Message)]
struct Event {
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(message, repeated, tag = "3")]
    pub attributes: Vec<KeyValue>,
}

#[derive(Clone, PartialEq, Message)]
struct ExportMetricsServiceRequest {
    #[prost(message, repeated, tag = "1")]
    pub resource_metrics: Vec<ResourceMetrics>,
}

#[derive(Clone, PartialEq, Message)]
struct ResourceMetrics {
    #[prost(message, optional, tag = "1")]
    pub resource: Option<Resource>,
    #[prost(message, repeated, tag = "2")]
    pub scope_metrics: Vec<ScopeMetrics>,
}

#[derive(Clone, PartialEq, Message)]
struct ScopeMetrics {
    #[prost(message, optional, tag = "1")]
    pub scope: Option<InstrumentationScope>,
    #[prost(message, repeated, tag = "2")]
    pub metrics: Vec<Metric>,
}

#[derive(Clone, PartialEq, Message)]
struct Metric {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub description: String,
    #[prost(string, tag = "3")]
    pub unit: String,
}

fn main() {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("Usage: otel_decode <path-to-otlp.bin>");
            std::process::exit(2);
        }
    };

    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read {}: {}", path, e);
            std::process::exit(1);
        }
    };

    println!("File: {} ({} bytes)", path, data.len());

    match ExportLogsServiceRequest::decode(data.as_slice()) {
        Ok(req) => summarize_logs(&req),
        Err(e) => println!("Logs decode error: {}", e),
    }

    match ExportTraceServiceRequest::decode(data.as_slice()) {
        Ok(req) => summarize_traces(&req),
        Err(e) => println!("Traces decode error: {}", e),
    }

    match ExportMetricsServiceRequest::decode(data.as_slice()) {
        Ok(req) => summarize_metrics(&req),
        Err(e) => println!("Metrics decode error: {}", e),
    }
}

fn summarize_logs(req: &ExportLogsServiceRequest) {
    let mut record_count = 0usize;
    let mut event_names = BTreeSet::new();
    let mut attribute_keys = BTreeSet::new();

    for resource_logs in &req.resource_logs {
        for scope_logs in &resource_logs.scope_logs {
            for record in &scope_logs.log_records {
                record_count += 1;
                if let Some(name) = log_record_event_name(record) {
                    event_names.insert(name);
                }
                for kv in &record.attributes {
                    attribute_keys.insert(kv.key.clone());
                }
            }
        }
    }

    println!(
        "Logs: resource_logs={}, log_records={}, event_names={}",
        req.resource_logs.len(),
        record_count,
        event_names.len()
    );
    if !event_names.is_empty() {
        println!("  Event names (sample): {:?}", event_names.iter().take(10).collect::<Vec<_>>());
    }
    if !attribute_keys.is_empty() {
        println!(
            "  Attribute keys (sample): {:?}",
            attribute_keys.iter().take(12).collect::<Vec<_>>()
        );
    }
}

fn summarize_traces(req: &ExportTraceServiceRequest) {
    let mut span_count = 0usize;
    let mut event_count = 0usize;
    let mut span_names = BTreeSet::new();
    let mut event_names = BTreeSet::new();
    let mut attribute_keys = BTreeSet::new();

    for resource_spans in &req.resource_spans {
        for scope_spans in &resource_spans.scope_spans {
            for span in &scope_spans.spans {
                span_count += 1;
                if !span.name.is_empty() {
                    span_names.insert(span.name.clone());
                }
                for kv in &span.attributes {
                    attribute_keys.insert(kv.key.clone());
                }
                for event in &span.events {
                    event_count += 1;
                    if !event.name.is_empty() {
                        event_names.insert(event.name.clone());
                    }
                    for kv in &event.attributes {
                        attribute_keys.insert(kv.key.clone());
                    }
                }
            }
        }
    }

    println!(
        "Traces: resource_spans={}, spans={}, events={}",
        req.resource_spans.len(),
        span_count,
        event_count
    );
    if !span_names.is_empty() {
        println!("  Span names (sample): {:?}", span_names.iter().take(10).collect::<Vec<_>>());
    }
    if !event_names.is_empty() {
        println!(
            "  Event names (sample): {:?}",
            event_names.iter().take(10).collect::<Vec<_>>()
        );
    }
    if !attribute_keys.is_empty() {
        println!(
            "  Attribute keys (sample): {:?}",
            attribute_keys.iter().take(12).collect::<Vec<_>>()
        );
    }
}

fn summarize_metrics(req: &ExportMetricsServiceRequest) {
    let mut metric_count = 0usize;
    let mut metric_names = BTreeSet::new();

    for resource_metrics in &req.resource_metrics {
        for scope_metrics in &resource_metrics.scope_metrics {
            for metric in &scope_metrics.metrics {
                metric_count += 1;
                if !metric.name.is_empty() {
                    metric_names.insert(metric.name.clone());
                }
            }
        }
    }

    println!(
        "Metrics: resource_metrics={}, metrics={}",
        req.resource_metrics.len(),
        metric_count
    );
    if !metric_names.is_empty() {
        println!(
            "  Metric names (sample): {:?}",
            metric_names.iter().take(10).collect::<Vec<_>>()
        );
    }
}

fn log_record_event_name(record: &LogRecord) -> Option<String> {
    let body_name = record.body.as_ref().and_then(|body| match &body.value {
        Some(aiki::editors::codex::otel::any_value::Value::StringValue(s)) => Some(s.clone()),
        _ => None,
    });

    body_name
        .or_else(|| get_string_attribute(&record.attributes, "event.name"))
        .or_else(|| get_string_attribute(&record.attributes, "name"))
}

fn get_string_attribute(attributes: &[KeyValue], key: &str) -> Option<String> {
    attributes.iter().find(|kv| kv.key == key).and_then(|kv| {
        kv.value.as_ref().and_then(|v| {
            if let Some(aiki::editors::codex::otel::any_value::Value::StringValue(s)) = &v.value {
                Some(s.clone())
            } else {
                None
            }
        })
    })
}
