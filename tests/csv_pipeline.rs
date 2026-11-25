use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gh-usage"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn success(output: Output) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

fn payload(path: &Path) -> Value {
    let html = fs::read_to_string(path).unwrap();
    let data = html.split_once("<script id=\"gh-usage-data\" type=\"application/json\">").unwrap().1
        .split_once("</script>").unwrap().0;
    serde_json::from_str(data).unwrap()
}

fn session(root: &Path, machine: &str, requests: Vec<Value>) -> PathBuf {
    let storage = root.join(machine).join("workspaceStorage");
    let path = storage.join("ws/chatSessions/shared.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, json!({"sessionId": "shared", "requests": requests}).to_string()).unwrap();
    storage
}

#[test]
fn it_renders_from_csv_without_logs_and_merges_cross_machine_slots() {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("gh-usage-csv-{}-{now}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    // Beijing 08:05 and 08:20 share a slot; exactly 08:30 starts the next one.
    let start = chrono::DateTime::parse_from_rfc3339("2025-01-06T08:00:00+08:00").unwrap().timestamp_millis();
    let desk = session(&root, "desk-log", vec![
        json!({"requestId":"same", "timestamp":start + 300_000, "message":{"text":"synthetic-private-body"}, "copilotCredits":13.5}),
        json!({"requestId":"automatic", "timestamp":start + 3_600_000, "message":{"text":"automatic"}, "isSystemInitiated":true}),
    ]);
    let laptop = session(&root, "laptop-log", vec![
        json!({"requestId":"same", "timestamp":start + 1_200_000, "message":{"text":"free request"}}),
        json!({"requestId":"next", "timestamp":start + 1_800_000, "message":{"text":"no usage row"}}),
    ]);
    success(run(&root, &["--hostname", "desk", "--no-cli-logs", "--vscode-workspace-storage", desk.to_str().unwrap()]));
    success(run(&root, &["--hostname", "desk", "--no-cli-logs", "--no-effort", "--vscode-workspace-storage", desk.to_str().unwrap(), "--output", "no-effort.csv"]));
    let no_effort = payload(&root.join("no-effort.html"));
    assert_eq!(no_effort["localSourceHost"], "desk");
    assert!(no_effort.get("effort").is_none());
    success(run(&root, &["--no-html", "--hostname", "laptop", "--no-cli-logs", "--vscode-workspace-storage", laptop.to_str().unwrap(), "--output", "custom.csv"]));
    assert!(!root.join("custom.html").exists());
    let desk_csv = root.join("copilot-effort-desk.csv");
    let csv = fs::read_to_string(&desk_csv).unwrap();
    assert!(!csv.contains("synthetic-private-body"));
    assert!(!csv.contains("automatic"));
    let wrong_input = run(&root, &["--from-csv", "copilot-effort-desk.csv"]);
    assert!(!wrong_input.status.success());
    assert!(String::from_utf8_lossy(&wrong_input.stderr).contains("expected a usage CSV"));
    fs::remove_dir_all(desk).unwrap();
    fs::remove_dir_all(laptop).unwrap();

    let direct = payload(&root.join("copilot-usage-desk.html"));
    let html = fs::read_to_string(root.join("copilot-usage-desk.html")).unwrap();
    assert_eq!(html.contains("<template id=\"rhythm-theme-layout\">"), cfg!(feature = "rhythm-theme"));
    assert!(!html.contains("__GH_USAGE_THEME_"));
    success(run(&root, &["--from-csv", "copilot-usage-desk.csv", "--html", "replay.html"]));
    let replay = payload(&root.join("replay.html"));
    for key in ["records", "history", "effort", "effortHosts"] { assert_eq!(replay[key], direct[key]); }
    assert_eq!(direct["localSourceHost"], "desk");
    assert_eq!(replay.get("localSourceHost"), Some(&Value::Null));
    assert_eq!(replay["records"][0]["credits"], 13.5);

    success(run(&root, &["--from-csv", "custom.csv"]));
    let effort_only = payload(&root.join("custom.html"));
    assert_eq!(effort_only["records"], json!([]));
    assert_eq!(effort_only["effort"].as_array().unwrap().len(), 2);

    fs::rename(root.join("custom.csv"), root.join("copilot-usage-laptop.csv")).unwrap();
    fs::rename(root.join("custom.effort.csv"), root.join("copilot-effort-laptop.csv")).unwrap();
    fs::rename(root.join("custom.export.json"), root.join("copilot-usage-laptop.export.json")).unwrap();
    fs::copy(root.join("copilot-usage-desk.csv"), root.join("copilot-usage-desk-copy.csv")).unwrap();
    fs::copy(root.join("copilot-usage-desk.export.json"), root.join("copilot-usage-desk-copy.export.json")).unwrap();
    fs::copy(&desk_csv, root.join("copilot-effort-desk-copy.csv")).unwrap();
    for (src, dst) in [("copilot-usage-laptop.csv", "copilot-usage-laptop-copy.csv"),
        ("copilot-usage-laptop.export.json", "copilot-usage-laptop-copy.export.json"),
        ("copilot-effort-laptop.csv", "copilot-effort-laptop-copy.csv")] {
        fs::copy(root.join(src), root.join(dst)).unwrap();
    }
    fs::write(root.join("copilot-usage-legacy.csv"), "hostname,model,credits\nlegacy,test,2\n").unwrap();
    fs::write(root.join("copilot-usage-empty-desk.csv"), "hostname,model,credits\n").unwrap();
    success(run(&root, &["--merge"]));
    let merged = payload(&root.join("copilot-usage-merged.html"));
    assert_eq!(merged.get("localSourceHost"), Some(&Value::Null));
    assert_eq!(merged["importedHosts"], json!(["desk", "empty-desk", "laptop", "legacy"]));
    assert_eq!(merged["effortHosts"], json!(["desk", "laptop"]));
    assert_eq!(merged["records"].as_array().unwrap().len(), 2);
    let events = merged["effort"].as_array().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events.iter().filter(|e| e["event_id"] == "same").count(), 2);
    let slots = events.iter().map(|event| event["timestamp_ms"].as_i64().unwrap().div_euclid(1_800_000)).collect::<BTreeSet<_>>();
    assert_eq!(slots.len(), 2);

    let overlapping = run(&root, &["--from-csv", "copilot-usage-desk.csv", "--html", "copilot-effort-desk.csv"]);
    assert!(!overlapping.status.success());
    assert_eq!(fs::read_to_string(&desk_csv).unwrap(), csv);
    fs::remove_dir_all(root).unwrap();
}

fn test_root(name: &str) -> PathBuf {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("gh-usage-{name}-{}-{now}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    root
}

fn failure(output: Output, message: &str) -> String {
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(stderr.contains(message), "{stderr}");
    stderr
}

fn snapshot(root: &Path, machine: &str, name: Option<&str>, credits: Option<u32>, total: Option<u32>, timestamp: i64) {
    let mut request = json!({"requestId": "request", "timestamp": timestamp, "message": {"text": "synthetic"}});
    if let Some(credits) = credits { request["copilotCredits"] = json!(credits); }
    let storage = session(root, machine, vec![request]);
    if let Some(total) = total {
        let file = storage.join("ws/chatSessions/shared.json");
        let mut data: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        data["sessionCostInfo"] = json!({"credits":total});
        fs::write(&file, data.to_string()).unwrap();
    }
    let mut args = vec!["--no-html", "--hostname", machine, "--no-cli-logs",
        "--vscode-workspace-storage", storage.to_str().unwrap()];
    if let Some(name) = name { args.extend(["--output", name]); }
    success(run(root, &args));
}

const START: i64 = 1_736_121_900_000;

#[test]
fn local_scan_counts_copies_of_one_vscode_request_only_once() {
    let root = test_root("workspace-copy");
    let storage = session(&root, "desk", vec![json!({
        "requestId": "request", "timestamp": START, "message": {"text": "synthetic"}, "copilotCredits": 10
    })]);
    let copy = storage.join("other/chatSessions/shared.json");
    fs::create_dir_all(copy.parent().unwrap()).unwrap();
    fs::copy(storage.join("ws/chatSessions/shared.json"), &copy).unwrap();
    let mut copied: Value = serde_json::from_slice(&fs::read(&copy).unwrap()).unwrap();
    copied["requests"].as_array_mut().unwrap().push(json!({
        "requestId": "free", "timestamp": START + 60_000, "message": {"text": "synthetic"}
    }));
    fs::write(&copy, copied.to_string()).unwrap();
    for (workspace, label) in [("ws", "source"), ("other", "copy")] {
        fs::write(storage.join(workspace).join("workspace.json"),
            json!({"folder": format!("file:///C:/fixtures/{label}")}).to_string()).unwrap();
    }
    success(run(&root, &["--hostname", "desk", "--no-cli-logs", "--vscode-workspace-storage", storage.to_str().unwrap()]));
    let rows = payload(&root.join("copilot-usage-desk.html"))["records"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["credits"], 10.0);
    assert_eq!(rows[0]["session_total"], 2);
    assert_eq!(fs::read_to_string(root.join("copilot-usage-desk.csv")).unwrap().lines().count(), 2);
    let mut changed: Value = serde_json::from_slice(&fs::read(&copy).unwrap()).unwrap();
    changed["requests"][0]["copilotCredits"] = json!(20);
    fs::write(copy, changed.to_string()).unwrap();
    failure(run(&root, &["--hostname", "desk", "--no-cli-logs", "--vscode-workspace-storage", storage.to_str().unwrap()]),
        "conflicting usage rows");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merges_only_each_machines_current_standard_export() {
    let root = test_root("canonical");
    snapshot(&root, "desk", None, Some(10), Some(20), START);
    snapshot(&root, "desk", None, Some(20), Some(20), START + 1_800_000);
    snapshot(&root, "laptop", None, Some(3), None, START + 1_800_000);
    // These are newer on disk but are not the authoritative export.
    snapshot(&root, "desk", Some("copilot-usage-desk-archive.csv"), Some(10), Some(20), START);
    fs::write(root.join("copilot-usage-desk-legacy-copy.csv"),
        "hostname,source,session_id,model,credits\ndesk,vscode.chatSessions,shared,m,200\n").unwrap();
    snapshot(&root, "desk", Some("custom.csv"), Some(100), None, START);
    let result = run(&root, &["--merge"]);
    let warnings = String::from_utf8_lossy(&result.stderr).into_owned();
    success(result);
    assert!(warnings.contains("ignoring") && warnings.contains("copilot-usage-desk-archive.csv")
        && warnings.contains("copilot-usage-desk-legacy-copy.csv"));
    let merged = payload(&root.join("copilot-usage-merged.html"));
    let records = merged["records"].as_array().unwrap();
    assert_eq!(merged["importedHosts"], json!(["desk", "laptop"]));
    assert_eq!(records.len(), 2);
    assert_eq!(records.iter().map(|r| r["credits"].as_f64().unwrap()).sum::<f64>(), 23.0);
    assert!(records.iter().all(|r| r["source"] != "vscode.sessionCost"));
    let events = merged["effort"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e["timestamp_ms"] == START + 1_800_000));
    success(run(&root, &["--from-csv", "copilot-usage-desk-archive.csv"]));
    let archive = payload(&root.join("copilot-usage-desk-archive.html"));
    assert_eq!(archive["records"].as_array().unwrap().len(), 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_wrong_companion_and_does_not_reimport_stale_effort() {
    let root = test_root("companion");
    let desk = "copilot-usage-desk.csv";
    snapshot(&root, "desk", None, Some(5), None, START);
    snapshot(&root, "laptop", None, Some(7), None, START + 1_800_000);
    fs::copy(root.join("copilot-effort-laptop.csv"), root.join("copilot-effort-desk.csv")).unwrap();
    failure(run(&root, &["--from-csv", desk]), "effort CSV does not match export receipt");
    failure(run(&root, &["--merge"]), "effort CSV does not match export receipt");
    let storage = root.join("desk/workspaceStorage");
    success(run(&root, &["--no-effort", "--no-html", "--hostname", "desk", "--no-cli-logs",
        "--vscode-workspace-storage", storage.to_str().unwrap(), "--output", desk]));
    success(run(&root, &["--from-csv", desk]));
    let rebuilt = payload(&root.join("copilot-usage-desk.html"));
    assert!(rebuilt.get("effort").is_none());
    success(run(&root, &["--merge"]));
    let merged = payload(&root.join("copilot-usage-merged.html"));
    assert_eq!(merged["effortHosts"], json!(["laptop"]));
    fs::remove_file(root.join("copilot-effort-laptop.csv")).unwrap();
    success(run(&root, &["--merge"]));
    assert!(payload(&root.join("copilot-usage-merged.html")).get("effort").is_none());
    let laptop_csv = root.join("copilot-usage-laptop.csv");
    let mut bytes = fs::read(&laptop_csv).unwrap();
    bytes.push(b'\n');
    fs::write(laptop_csv, bytes).unwrap();
    failure(run(&root, &["--merge"]), "usage CSV does not match export receipt");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn does_not_promote_archives_when_the_standard_export_is_missing() {
    let root = test_root("missing-canonical");
    snapshot(&root, "desk", Some("copilot-usage-desk-archive.csv"), None, None, START);
    let warning = failure(run(&root, &["--merge"]), "no standard usage CSV files found");
    assert!(warning.contains("ignoring") && warning.contains("copilot-usage-desk-archive.csv"));
    assert!(!root.join("copilot-usage-merged.html").exists());
    success(run(&root, &["--from-csv", "copilot-usage-desk-archive.csv"]));
    assert_eq!(payload(&root.join("copilot-usage-desk-archive.html"))["effortHosts"], json!(["desk"]));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn single_and_multi_machine_import_use_the_same_billing_validation() {
    let root = test_root("billing-validation");
    let name = "copilot-usage-desk.csv";
    fs::write(root.join(name), "hostname,source,model,file,line,credits,request_count,unclassified_usage\n\
        desk,copilot.cli.events,A,events.jsonl,1,15,3,\n").unwrap();
    failure(run(&root, &["--from-csv", name]), "conflicting billing units");
    failure(run(&root, &["--merge"]), "conflicting billing units");
    fs::write(root.join(name), "hostname,source,model,file,line,credits,request_count,unclassified_usage\n\
        desk,copilot.cli.events,A,events.jsonl,1,0,3,\n\
        desk,copilot.cli.events,B,events.jsonl,1,0,,5\n").unwrap();
    success(run(&root, &["--from-csv", name]));
    success(run(&root, &["--merge"]));
    let replay = payload(&root.join("copilot-usage-desk.html"));
    let merged = payload(&root.join("copilot-usage-merged.html"));
    assert_eq!(replay["history"], merged["history"]);
    assert_eq!(merged["records"], json!([]));
    assert_eq!(merged["history"].as_array().unwrap().len(), 2);
    assert_eq!(merged["history"][0]["request_count"], 3);
    assert_eq!(merged["history"][1]["unclassified_usage"].as_f64(), Some(5.0));
    fs::write(root.join(name), "hostname,source,session_id,request_id,turn_index,credits\n\
        desk,vscode.chatSessions,shared,request,1,10\n\
        desk,vscode.chatSessions,shared,request,1,20\n").unwrap();
    failure(run(&root, &["--from-csv", name]), "conflicting usage rows");
    failure(run(&root, &["--merge"]), "conflicting usage rows");
    fs::remove_dir_all(root).unwrap();
}