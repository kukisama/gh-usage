use super::{Cli, normalize_view_args, open_path, resolve_html_path, run_merge};
use clap::{Parser, error::ErrorKind};
use std::path::{Path, PathBuf};

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(normalize_view_args(args.iter().copied())).unwrap()
}

#[test]
fn accepts_view_spellings_and_preserves_defaults() {
    assert!(!parse(&["gh-usage"]).view);
    for flag in ["--view", "-v", "-view"] {
        let cli = parse(&["gh-usage", flag]);
        assert!(cli.view);
        assert_eq!(
            resolve_html_path(&cli, Path::new("copilot-usage-host.csv"), "host"),
            Some(PathBuf::from("copilot-usage-host.html"))
        );
    }
    assert!(parse(&["gh-usage", "--since-days", "7", "--view"]).view);
}

#[test]
fn view_conflicts_with_disabled_html() {
    for flag in ["--view", "-v", "-view"] {
        for suffix in [vec!["--no-html"], vec!["--merge", "--no-html"]] {
            let mut args = vec!["gh-usage", flag];
            args.extend(suffix);
            let error = Cli::try_parse_from(normalize_view_args(args)).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
        }
    }
}

#[test]
fn compatibility_spelling_does_not_rewrite_values_or_separator() {
    for args in [
        vec!["gh-usage", "--", "-view"],
        vec!["gh-usage", "--html", "-view"],
        vec!["gh-usage", "--hostname=-view"],
    ] {
        let expected: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
        assert_eq!(normalize_view_args(args), expected);
    }
    assert_eq!(parse(&["gh-usage", "--hostname=-view"]).hostname.as_deref(), Some("-view"));
}

#[test]
fn stdout_only_gains_html_when_explicitly_requested() {
    for (args, expected) in [
        (vec!["gh-usage", "-o", "-"], None),
        (vec!["gh-usage", "-v", "-o", "-"], Some("copilot-usage-host.html")),
        (vec!["gh-usage", "-v", "-o", "-", "--html", "report.html"], Some("report.html")),
        (vec!["gh-usage", "-o", "-", "--html", "report.html"], Some("report.html")),
    ] {
        assert_eq!(
            resolve_html_path(&parse(&args), Path::new("-"), "host"),
            expected.map(PathBuf::from)
        );
    }
}

#[test]
fn custom_report_path_and_legacy_no_html_are_preserved() {
    let cli = parse(&["gh-usage", "-v", "--html", "custom report.html"]);
    assert_eq!(resolve_html_path(&cli, Path::new("data.json"), "host"), Some("custom report.html".into()));
    let cli = parse(&["gh-usage", "-v", "--format", "json"]);
    assert_eq!(resolve_html_path(&cli, Path::new("reports/data.json"), "host"), Some("reports/data.html".into()));
    let cli = parse(&["gh-usage", "--no-html"]);
    assert_eq!(resolve_html_path(&cli, Path::new("data.csv"), "host"), None);
}

#[test]
fn merge_accepts_view_and_respects_custom_html() {
    let root = super::tests::temporary_cli_root("merge-view");
    std::fs::write(root.join("copilot-usage-test.csv"), "hostname,model,credits\nhost,test-model,12.5\n").unwrap();
    let output = root.join("custom report.html");
    let cli = parse(&["gh-usage", "--merge", "-v"]);
    assert!(cli.view);
    assert_eq!(cli.merge, Some(PathBuf::from(".")));
    // Test report generation without launching a real browser in unit tests.
    run_merge(&root, false, false, Some(&output)).unwrap();
    let html = std::fs::read_to_string(&output).unwrap();
    assert!(html.contains("\"credits\":12.5"));
    assert!(!root.join("copilot-usage-merged.html").exists());
    run_merge(&root, false, false, None).unwrap();
    assert!(root.join("copilot-usage-merged.html").is_file());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_report_is_not_sent_to_browser() {
    let root = super::tests::temporary_cli_root("missing-report");
    let error = open_path(&root.join("missing.html")).unwrap_err();
    assert!(error.to_string().contains("HTML report not found"));
    std::fs::remove_dir_all(root).unwrap();
}