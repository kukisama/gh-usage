use super::*;
use crate::record::assign_turn_indices;
use crate::scan::scan_file;
use crate::test_support::temporary_root;
use serde_json::json;

#[test]
fn extracts_project_and_bounded_title_for_cli_sessions() {
    assert_eq!(
        project_name_from_cli_path(r"C:\work\gh-usage").as_deref(),
        Some("gh-usage")
    );
    assert_eq!(
        project_name_from_cli_path("/home/user/project/").as_deref(),
        Some("project")
    );
    assert_eq!(
        normalized_cli_title(Some("\n  Useful CLI session  \nsecond line")).as_deref(),
        Some("Useful CLI session")
    );
}

#[test]
fn aggregates_cli_usage_by_session_turn_and_model() {
    let cli_root = temporary_root("hybrid-cli");
    let first_events = cli_root.join("session-state/session-1/events.jsonl");
    let legacy_events = cli_root.join("session-state/session-legacy/events.jsonl");
    fs::create_dir_all(first_events.parent().unwrap()).unwrap();
    fs::create_dir_all(legacy_events.parent().unwrap()).unwrap();
    fs::write(
        &first_events,
        concat!(
            "{\"type\":\"user.message\",\"timestamp\":\"2026-08-01T00:00:09Z\",\"data\":{\"content\":\"Correct JSONL title\\nmore detail\"}}\n",
            "{\"type\":\"assistant.message\",\"timestamp\":\"2026-08-01T00:00:10.008Z\",\"data\":{\"model\":\"gpt-test\",\"apiCallId\":\"api-1\"}}\n",
            "{\"type\":\"assistant.message\",\"timestamp\":\"2026-08-01T00:00:12.010Z\",\"data\":{\"model\":\"gpt-test\",\"apiCallId\":\"api-2\"}}\n",
            "{\"type\":\"assistant.message\",\"timestamp\":\"2026-08-01T00:00:20.009Z\",\"data\":{\"model\":\"gpt-test\",\"apiCallId\":\"api-3\"}}\n"
        ),
    )
    .unwrap();
    fs::write(
        &legacy_events,
        concat!(
            "{\"type\":\"user.message\",\"timestamp\":\"2026-08-01T00:00:30Z\",\"data\":{\"content\":\"Legacy JSONL title\"}}\n",
            "{\"type\":\"session.shutdown\",\"timestamp\":\"2026-08-01T00:01:00Z\",\"data\":{\"totalPremiumRequests\":1,\"modelMetrics\":{\"gpt-legacy\":{\"requests\":{\"cost\":1,\"count\":3}}}}}\n"
        ),
    )
    .unwrap();
    fs::write(
        first_events.parent().unwrap().join("workspace.yaml"),
        "name: Correct CLI title property\ncwd: C:\\work\\jsonl-project\n",
    )
    .unwrap();
    fs::write(
        legacy_events.parent().unwrap().join("workspace.yaml"),
        "name: Legacy CLI title property\ncwd: C:\\work\\legacy-jsonl-project\n",
    )
    .unwrap();

    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY, cwd TEXT, repository TEXT, summary TEXT
             );
             CREATE TABLE turns (
                session_id TEXT, turn_index INTEGER, user_message TEXT
             );
             CREATE TABLE assistant_usage_events (
                id INTEGER PRIMARY KEY, session_id TEXT, turn_index INTEGER,
                model TEXT, total_nano_aiu INTEGER, duration_ms INTEGER,
                created_at TEXT, agent_id TEXT
             );
             INSERT INTO sessions VALUES
                ('session-1', 'C:\\work\\gh-usage', 'owner/gh-usage',
                 'Wrong SQLite title'),
                ('session-legacy', 'C:\\work\\legacy-project', 'owner/legacy',
                 'Wrong legacy SQLite title'),
                ('session-db-only', 'C:\\work\\db-only', 'owner/db-only',
                 'Database-only record must be ignored');
             INSERT INTO turns VALUES ('session-1', 0, 'fallback title');
             INSERT INTO assistant_usage_events VALUES
                (1, 'session-1', 0, 'gpt-test', 1500000000, 2000,
                 '2026-08-01T00:00:10Z', NULL),
                (2, 'session-1', 0, 'gpt-test', 2000000000, 1000,
                 '2026-08-01T00:00:12Z', NULL),
                (3, 'session-1', 1, 'gpt-test', 500000000, 500,
                 '2026-08-01T00:00:20Z', NULL),
                (4, 'session-db-only', 0, 'gpt-test', 9000000000, 500,
                 '2026-08-01T00:00:30Z', NULL);",
        )
        .unwrap();

    let result = scan_cli_usage_connection(
        &connection,
        &cli_root.join("session-store.db"),
        None,
    )
    .unwrap();

    assert_eq!(result.records.len(), 3);
    let first = &result.records[0];
    assert!((first.credits - 3.5).abs() < f64::EPSILON);
    assert_eq!(first.turn_index, Some(1));
    assert_eq!(first.session_total, Some(2));
    assert_eq!(first.duration_ms, Some(4002));
    assert_eq!(first.project.as_deref(), Some("jsonl-project"));
    assert_eq!(
        first.chat_title.as_deref(),
        Some("Correct CLI title property")
    );
    assert!(first.details.contains("2 model calls"));
    assert_eq!(first.source, "copilot.cli.events");
    assert_eq!(PathBuf::from(&first.file), first_events);
    assert_eq!(first.line, 2);

    let legacy = result
        .records
        .iter()
        .find(|record| record.session_id.as_deref() == Some("session-legacy"))
        .unwrap();
    assert_eq!(legacy.amount(), (BillingUnit::RequestCount, 3.0));
    assert_eq!(legacy.model, "gpt-legacy");
    assert_eq!(legacy.project.as_deref(), Some("legacy-jsonl-project"));
    assert_eq!(
        legacy.chat_title.as_deref(),
        Some("Legacy CLI title property")
    );
    assert_eq!(PathBuf::from(&legacy.file), legacy_events);
    assert_eq!(legacy.line, 2);
    assert!(result
        .records
        .iter()
        .all(|record| record.session_id.as_deref() != Some("session-db-only")));

    fs::remove_dir_all(cli_root).unwrap();
}

#[test]
fn matches_cli_usage_events_when_response_model_differs() {
    let root = temporary_root("cli-usage-events");
    let events = root.join("session-state/session/events.jsonl");
    fs::create_dir_all(events.parent().unwrap()).unwrap();
    let usage = |second: u32, model: &str, api: &str, nano: i64| json!({
        "type": "session.usage_record",
        "timestamp": format!("2026-11-23T00:00:{second:02}.123Z"),
        "data": {"usage": {"model": model, "apiCallId": api,
            "copilotUsage": {"totalNanoAiu": nano}}}
    });
    let response = |second: u32, model: &str, api: &str| json!({
        "type": "assistant.message",
        "timestamp": format!("2026-11-23T00:00:{second:02}.123Z"),
        "data": {"model": model, "apiCallId": api}
    });
    let log = [
        usage(10, "ordinary", "first", 1_000_000_000),
        response(10, "ordinary", "first"),
        usage(20, "phase-model", "second", 2_250_000_000),
        response(25, "fusion-model", "second"),
        usage(30, "phase-model", "third", 4_100_000_000),
    ];
    fs::write(&events, log.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")).unwrap();

    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE assistant_usage_events (
            id INTEGER PRIMARY KEY, session_id TEXT, turn_index INTEGER, model TEXT,
            total_nano_aiu INTEGER, duration_ms INTEGER, created_at TEXT, agent_id TEXT
        );
        INSERT INTO assistant_usage_events VALUES
            (1, 'session', 0, 'ordinary', 1000000000, 500, '2026-11-23T00:00:10.123Z', NULL),
            (2, 'session', 0, 'phase-model', 2250000000, 2000, '2026-11-23T00:00:20.123Z', NULL),
            (3, 'session', 0, 'phase-model', 4000000000, 500, '2026-11-23T00:00:30.123Z', NULL);",
    ).unwrap();

    let result = scan_cli_usage_connection(&db, &root.join("session-store.db"), None).unwrap();
    assert_eq!(result.parse_errors, 0);
    assert_eq!(result.records.len(), 2);
    let ordinary = result.records.iter().find(|r| r.model == "ordinary").unwrap();
    assert_eq!((ordinary.line, ordinary.credits), (2, 1.0));
    let phase = result.records.iter().find(|r| r.model == "phase-model").unwrap();
    assert_eq!(PathBuf::from(&phase.file), events);
    assert_eq!((phase.line, phase.credits, phase.duration_ms, phase.turn_index), (3, 2.25, Some(2000), Some(1)));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn extracts_only_real_root_cli_user_messages_as_titles() {
    let root = br#"{"type":"user.message","data":{"content":"JSONL title"}}"#;
    let sourced = br#"{"type":"user.message","data":{"source":"system","content":"Not a title"}}"#;
    let subagent = br#"{"type":"user.message","agentId":"agent-1","data":{"content":"Not a title"}}"#;

    assert_eq!(
        extract_cli_user_title_from_line(root).as_deref(),
        Some("JSONL title")
    );
    assert_eq!(extract_cli_user_title_from_line(sourced), None);
    assert_eq!(extract_cli_user_title_from_line(subagent), None);
}

#[test]
fn scans_legacy_cli_jsonl_without_sqlite() {
    let cli_root = temporary_root("legacy-jsonl-only");
    let events = cli_root.join("session-state/old-session/events.jsonl");
    fs::create_dir_all(events.parent().unwrap()).unwrap();
    fs::write(
        &events,
        concat!(
            "{\"type\":\"user.message\",\"timestamp\":\"2026-08-01T00:00:00Z\",\"data\":{\"content\":\"Old JSONL title\"}}\n",
            "{\"type\":\"session.shutdown\",\"timestamp\":\"2026-08-01T00:01:00Z\",\"data\":{\"modelMetrics\":{\"gpt-old\":{\"requests\":{\"cost\":2}}}}}\n"
        ),
    )
    .unwrap();
    fs::write(
        events.parent().unwrap().join("workspace.yaml"),
        "name: Old CLI title property\ncwd: C:\\work\\old-project\n",
    )
    .unwrap();

    let mut result = scan_file(&events, &HashMap::new(), false);
    assign_turn_indices(&mut result.records);

    assert_eq!(result.records.len(), 1);
    let record = &result.records[0];
    assert_eq!(record.turn_index, None);
    assert_eq!(record.session_total, None);
    assert_eq!(record.session_id.as_deref(), Some("old-session"));
    assert_eq!(record.chat_title.as_deref(), Some("Old CLI title property"));
    assert_eq!(record.project.as_deref(), Some("old-project"));
    assert_eq!(record.model, "gpt-old");
    assert_eq!(record.amount(), (BillingUnit::Unclassified, 2.0));
    assert_eq!(record.file, events.to_string_lossy());
    assert_eq!(record.line, 2);

    fs::remove_dir_all(cli_root).unwrap();
}

#[test]
fn shutdown_prefers_explicit_nano_aiu_without_reusing_requests_cost() {
    let data = json!({"totalNanoAiu": 125500000000u64, "totalPremiumRequests": 3,
        "modelMetrics": {"old-model": {"requests": {"cost": 3}}}});
    assert_eq!(cli_shutdown_amounts(data.as_object().unwrap()),
        vec![("Unattributed".to_owned(), BillingUnit::Credits, 125.5)]);
}

#[test]
fn resumed_cli_shutdown_snapshots_are_not_added() {
    let root = temporary_root("resumed-cli");
    let events = root.join("session-state/old-session/events.jsonl");
    fs::create_dir_all(events.parent().unwrap()).unwrap();
    let journal = [1, 1, 3, 2].iter().enumerate().map(|(index, amount)| json!({
        "type":"session.shutdown", "timestamp": format!("2026-10-21T00:00:0{index}Z"),
        "data": {"sessionStartTime": 1234, "totalNanoAiu": *amount * 1_000_000_000u64, "totalPremiumRequests": 15}
    }).to_string()).collect::<Vec<_>>().join("\n");
    fs::write(&events, journal).unwrap();
    let result = scan_file(&events, &HashMap::new(), false);
    assert_eq!(result.records.len(), 1);
    assert_eq!(result.records[0].amount(), (BillingUnit::Credits, 3.0));
    assert_eq!(result.records[0].line, 3);
    fs::remove_dir_all(root).unwrap();
}