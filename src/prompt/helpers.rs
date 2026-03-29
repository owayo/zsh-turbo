use std::path::Path;
use std::process::Command;
use std::time::Duration;

const CMD_TIMEOUT: Duration = Duration::from_millis(500);

/// Run a command with a 500ms timeout, returning stdout on success.
pub fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                let output = child.wait_with_output().ok()?;
                return Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
            }
            Ok(Some(_)) => return None,
            Ok(None) => {
                if start.elapsed() > CMD_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
}

pub fn truncate_path(path: &str, max_components: usize, symbol: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= max_components + 1 {
        return path.to_string();
    }
    let first = parts[0];
    let last_parts = &parts[parts.len() - max_components..];
    format!("{first}/{symbol}/{}", last_parts.join("/"))
}

pub fn has_marker_file(dir: &Path, markers: &[&str]) -> bool {
    let mut current = Some(dir);
    while let Some(d) = current {
        for marker in markers {
            if marker.contains('*') {
                if let Ok(entries) = std::fs::read_dir(d) {
                    let suffix = marker.trim_start_matches('*');
                    for entry in entries.flatten() {
                        let name = entry.file_name();
                        if name.to_string_lossy().ends_with(suffix) {
                            return true;
                        }
                    }
                }
            } else if d.join(marker).exists() {
                return true;
            }
        }
        current = d.parent();
    }
    false
}

pub fn extract_version(text: &str) -> String {
    let text = text.trim();
    for word in text.split_whitespace() {
        let w = word.trim_start_matches('v');
        if w.contains('.') && w.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return format!("v{w}");
        }
    }
    text.to_string()
}

pub fn find_up(dir: &Path, filename: &str) -> Option<std::path::PathBuf> {
    let mut current = Some(dir);
    while let Some(d) = current {
        let candidate = d.join(filename);
        if candidate.exists() {
            return Some(candidate);
        }
        current = d.parent();
    }
    None
}

/// Simple JSON string value extraction (avoids serde_json dependency)
pub fn extract_json_value(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let idx = json.find(&pattern)?;
    let rest = &json[idx + pattern.len()..];
    let rest = rest.trim_start().strip_prefix(':')?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
