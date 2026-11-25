//! Field extraction shared by the VS Code journal and legacy probes.

use std::path::Path;
use std::time::SystemTime;

use memchr::memmem;
use serde_json::{Map, Value};

use crate::record::ContextFields;
use crate::time::parse_rfc3339_timestamp_ms;

pub(crate) fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    memmem::find(haystack, needle).is_some()
}

pub(crate) fn has_component(path: &Path, wanted: &str) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(wanted)
    })
}

pub(crate) fn modified_after(path: &Path, cutoff: Option<&SystemTime>) -> bool {
    match cutoff {
        Some(cutoff) => path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| modified >= *cutoff),
        None => true,
    }
}

pub(crate) fn display_model_id(model_id: &str) -> String {
    model_id
        .strip_prefix("copilot/")
        .unwrap_or(model_id)
        .to_owned()
}

pub(crate) fn merge_context_from_object(context: &mut ContextFields, map: &Map<String, Value>) {
    assign_string(&mut context.session_id, map, "sessionId");
    assign_string(&mut context.request_id, map, "requestId");
    assign_string(&mut context.response_id, map, "responseId");
    assign_string(&mut context.model_id, map, "modelId");
    assign_string(&mut context.agent_id, map, "agentId");

    if context.timestamp_ms.is_none() {
        context.timestamp_ms = map.get("timestamp").and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(parse_rfc3339_timestamp_ms))
        });
    }

    if let Some(Value::Object(model_state)) = map.get("modelState")
        && let Some(completed_at) = model_state.get("completedAt").and_then(Value::as_i64)
    {
        context.timestamp_ms = Some(completed_at);
    }

    if let Some(completed_at) = map.get("completedAt").and_then(Value::as_i64) {
        context.timestamp_ms = Some(completed_at);
    }

    // The chat-session `result` node carries wall-clock timing for the whole
    // exchange (request submitted -> response completed) under `timings`.
    if context.total_elapsed_ms.is_none()
        && let Some(Value::Object(timings)) = map.get("timings")
        && let Some(elapsed) = timings.get("totalElapsed").and_then(Value::as_i64)
        && elapsed > 0
    {
        context.total_elapsed_ms = Some(elapsed);
    }
}

fn assign_string(target: &mut Option<String>, map: &Map<String, Value>, key: &str) {
    if target.is_none()
        && let Some(value) = map.get(key).and_then(Value::as_str)
    {
        *target = Some(value.to_owned());
    }
}