use super::*;
use crate::test_support::temporary_root;
use serde_json::json;

#[test]
fn parses_credit_details_with_bullet_separator() {
    let (model, credits) = parse_credit_details("GPT-5.5 • 105.7 credits").unwrap();

    assert_eq!(model, "GPT-5.5");
    assert_eq!(credits, 105.7);
}

#[test]
fn extracts_custom_title_from_chat_session_line() {
    let value = json!({"kind":1,"k":["customTitle"],"v":"Switching speed optimization notes"});

    assert_eq!(
        extract_custom_title(&value).as_deref(),
        Some("Switching speed optimization notes")
    );
}

#[test]
fn quick_extracts_nested_custom_title() {
    let line = br#"{"kind":0,"v":{"version":3,"customTitle":"\u65b0\u5efaAIFriend\u76ee\u5f55\u548c\u63d2\u4ef6","title":"fallback"}}"#;
    assert_eq!(
        quick_extract_title_from_line(line).as_deref(),
        Some("\u{65b0}\u{5efa}AIFriend\u{76ee}\u{5f55}\u{548c}\u{63d2}\u{4ef6}")
    );
}

#[test]
fn quick_extracts_falls_back_to_title() {
    let line = br#"{"kind":0,"v":{"title":"plain title"}}"#;
    assert_eq!(quick_extract_title_from_line(line).as_deref(), Some("plain title"));
}

#[test]
fn quick_extracts_handles_escaped_quote() {
    let line = br#"{"customTitle":"He said \"hi\" yo"}"#;
    assert_eq!(quick_extract_title_from_line(line).as_deref(), Some("He said \"hi\" yo"));
}

#[test]
fn extracts_copilot_cli_shutdown_model_metrics() {
    let value = json!({
        "type": "session.shutdown",
        "timestamp": "2026-05-18T10:56:39.403Z",
        "data": {
            "totalPremiumRequests": 1,
            "modelMetrics": {
                "gpt-5.4": {
                    "requests": {"count": 3, "cost": 1},
                    "usage": {"inputTokens": 20028, "outputTokens": 62}
                }
            }
        }
    });

    let mut extraction = LineExtraction::default();
    collect_line_extraction(&value, ContextFields::default(), &mut extraction);

    assert_eq!(extraction.records.len(), 1);
    assert_eq!(extraction.records[0].model, "gpt-5.4");
    assert_eq!(extraction.records[0].credits, 3.0);
    assert_eq!(extraction.records[0].unit, BillingUnit::RequestCount);
    assert_eq!(extraction.records[0].context.timestamp_ms, Some(1779101799403));
}

#[test]
fn keeps_latest_vscode_result_for_each_response() {
    let root = temporary_root("vscode-result-rewrites");
    let session = root.join("workspace/chatSessions/session.jsonl");
    fs::create_dir_all(session.parent().unwrap()).unwrap();
    fs::write(
        &session,
        concat!(
            "{\"kind\":1,\"k\":[\"requests\",0,\"result\"],\"v\":{\"details\":\"GPT-Test • 12.5 credits\",\"metadata\":{\"sessionId\":\"session\",\"responseId\":\"response-1\",\"modelId\":\"copilot/gpt-test\"},\"completedAt\":1000}}\n",
            "{\"kind\":1,\"k\":[\"requests\",1,\"result\"],\"v\":{\"details\":\"GPT-Test • 3 credits\",\"metadata\":{\"sessionId\":\"session\",\"responseId\":\"response-2\",\"modelId\":\"copilot/gpt-test\"},\"completedAt\":2000}}\n",
            "{\"kind\":1,\"k\":[\"requests\",0,\"result\"],\"v\":{\"details\":\"GPT-Test • 12.5 credits\",\"metadata\":{\"sessionId\":\"session\",\"responseId\":\"response-1\",\"modelId\":\"copilot/gpt-test\"},\"completedAt\":1000}}\n"
        ),
    )
    .unwrap();

    let result = scan_file(&session, &HashMap::new(), false);

    assert_eq!(result.records.len(), 2);
    let first = result
        .records
        .iter()
        .find(|record| record.response_id.as_deref() == Some("response-1"))
        .unwrap();
    assert_eq!(first.credits, 12.5);
    assert_eq!(first.line, 3);
    assert_eq!(first.model_id.as_deref(), Some("gpt-test"));

    fs::remove_dir_all(root).unwrap();
}