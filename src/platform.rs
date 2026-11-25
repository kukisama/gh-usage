use std::env;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// True only on an interactive console; redirected runs never pause.
pub(crate) fn console_is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

pub(crate) fn show_double_click_exit_notice(output: &Path, html: Option<&Path>) {
    println!();
    println!("CSV saved to {}", output.display());
    println!("  Import usage into Excel or Power BI. Human submissions are in a separate effort CSV.");
    if let Some(html) = html {
        println!();
        println!("HTML report saved to {}", html.display());
        println!(
            "  A self-contained, offline-friendly page with charts, host/model"
        );
        println!(
            "  filters, and a paginated record table. Safe to email or archive."
        );
        // Open the report automatically so viewing it never requires a keypress.
        // This branch only runs on a genuine Explorer double-click; terminal,
        // CI, and the winget validation sandbox never reach here.
        if let Err(error) = open_path(html) {
            eprintln!("warning: {error:#}");
        }
        println!();
        println!(">> The report is ready to view.");
        println!("   If it didn't open, double-click the HTML file above, or run");
        println!("   `gh-usage --html <PATH>` to regenerate it at a path you choose.");
    } else {
        println!();
        println!("HTML report skipped (--no-html).");
        println!("   Re-run without --no-html (or `gh-usage --html <PATH>`) to build it.");
    }
    println!();
    println!(">> Press any key to close this window. Your files are already saved.");
    wait_for_any_key();
}

pub(crate) fn show_merge_exit_notice(html: &Path, hosts: &[&str]) {
    println!();
    println!("Merged HTML report saved to {}", html.display());
    println!(
        "  Combined data from {} machine(s): {}",
        hosts.len(),
        hosts.join(", ")
    );
    println!(
        "  Open it in any browser - charts, filters, and pagination work offline,"
    );
    println!(
        "  and the single file is safe to email or archive."
    );
    // Open automatically; only reached on a real Explorer double-click.
    if let Err(error) = open_path(html) {
        eprintln!("warning: {error:#}");
    }
    println!();
    println!(">> The merged report is ready to view.");
    println!("   If it didn't open, double-click the HTML file above.");
    println!();
    println!(">> Press any key to close this window. The HTML is already saved.");
    wait_for_any_key();
}

pub(crate) fn open_path(path: &Path) -> Result<()> {
    // Absolute paths avoid interpreting filenames as launcher options. Do not
    // canonicalize: Windows extended-length paths can confuse shell handlers.
    let absolute = std::path::absolute(path)?;
    anyhow::ensure!(absolute.is_file(), "HTML report not found: {}", absolute.display());
    open::that(&absolute).with_context(|| {
        format!("HTML saved to {}, but could not launch its default browser", absolute.display())
    })?;
    // Keep stdout clean for --output - and report only the launch request,
    // not a browser-rendering success that we cannot observe here.
    eprintln!("Requested browser opening: {}", absolute.display());
    Ok(())
}

// Detects whether the executable was launched directly from Windows Explorer.
// Terminal, CI, and the winget validation sandbox must not wait for a keypress.
#[cfg(windows)]
pub(crate) fn launched_from_explorer() -> bool {
    windows_parent_process_name()
        .as_deref()
        .is_some_and(|name| name.eq_ignore_ascii_case("explorer.exe"))
}

#[cfg(not(windows))]
pub(crate) fn launched_from_explorer() -> bool {
    false
}

#[cfg(windows)]
fn wait_for_any_key() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, ReadConsoleW, SetConsoleMode, ENABLE_ECHO_INPUT,
        ENABLE_LINE_INPUT, STD_INPUT_HANDLE,
    };

    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if handle == INVALID_HANDLE_VALUE {
            return;
        }

        let mut original_mode = 0;
        if GetConsoleMode(handle, &mut original_mode) == 0 {
            let mut input = String::new();
            let _ = std::io::stdin().read_line(&mut input);
            return;
        }

        let single_key_mode = original_mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT);
        let _ = SetConsoleMode(handle, single_key_mode);

        let mut buffer = [0u16; 1];
        let mut chars_read = 0;
        let _ = ReadConsoleW(
            handle,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            &mut chars_read,
            std::ptr::null_mut(),
        );

        let _ = SetConsoleMode(handle, original_mode);
    }
}

#[cfg(not(windows))]
fn wait_for_any_key() {}

#[cfg(windows)]
fn windows_parent_process_name() -> Option<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut entry = std::mem::zeroed::<PROCESSENTRY32W>();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        let current_pid = std::process::id();
        let mut parent_pid = None;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == current_pid {
                    parent_pid = Some(entry.th32ParentProcessID);
                    break;
                }

                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        let Some(parent_pid) = parent_pid else {
            let _ = CloseHandle(snapshot);
            return None;
        };

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == parent_pid {
                    let name = wide_process_name_to_string(&entry.szExeFile);
                    let _ = CloseHandle(snapshot);
                    return name;
                }

                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
        None
    }
}

#[cfg(windows)]
fn wide_process_name_to_string(buffer: &[u16]) -> Option<String> {
    let len = buffer.iter().position(|ch| *ch == 0).unwrap_or(buffer.len());
    if len == 0 {
        None
    } else {
        Some(String::from_utf16_lossy(&buffer[..len]))
    }
}

pub(crate) fn default_vscode_workspace_storage() -> Option<PathBuf> {
    let candidates = vscode_user_data_dirs().into_iter()
        .map(|path| path.join("User").join("workspaceStorage"))
        .collect::<Vec<_>>();

    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .or_else(|| candidates.into_iter().next())
}

#[cfg(windows)]
fn vscode_user_data_dirs() -> Vec<PathBuf> {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| vec![path.join("Code"), path.join("Code - Insiders")])
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn vscode_user_data_dirs() -> Vec<PathBuf> {
    home_dir()
        .map(|home| {
            let application_support = home.join("Library").join("Application Support");
            vec![
                application_support.join("Code"),
                application_support.join("Code - Insiders"),
            ]
        })
        .unwrap_or_default()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn vscode_user_data_dirs() -> Vec<PathBuf> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".config")));

    config_home
        .map(|path| vec![path.join("Code"), path.join("Code - Insiders")])
        .unwrap_or_default()
}

pub(crate) fn default_copilot_cli_root() -> Option<PathBuf> {
    home_dir().map(|path| path.join(".copilot"))
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub(crate) fn detect_hostname() -> String {
    if let Some(name) = env::var("COMPUTERNAME")
        .ok()
        .or_else(|| env::var("HOSTNAME").ok())
        .map(|name| sanitize_hostname(&name))
        .filter(|name| !name.is_empty())
    {
        return name;
    }

    #[cfg(unix)]
    {
        for path in ["/proc/sys/kernel/hostname", "/etc/hostname"] {
            if let Ok(text) = std::fs::read_to_string(path) {
                let cleaned = sanitize_hostname(&text);
                if !cleaned.is_empty() {
                    return cleaned;
                }
            }
        }
    }

    "unknown-host".to_owned()
}

pub(crate) fn sanitize_hostname(raw: &str) -> String {
    raw.trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}