use std::io::Read as _;
#[cfg(unix)]
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CMD_TIMEOUT: Duration = Duration::from_millis(500);

/// 500ms のタイムアウト付きでコマンドを実行し、成功時の出力を返す。
/// stdout/stderr は別スレッドで並行 drain し、pipe バッファ満杯による
/// 子プロセスの書き込みブロック（デッドロック）を防ぐ。
pub fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    run_cmd_inner(cmd, args, true, CMD_TIMEOUT)
}

/// 500ms のタイムアウト付きでコマンドを実行し、stdout がある場合だけ返す。
pub fn run_cmd_stdout(cmd: &str, args: &[&str]) -> Option<String> {
    run_cmd_inner(cmd, args, false, CMD_TIMEOUT)
}

pub(crate) fn run_cmd_inner(
    cmd: &str,
    args: &[&str],
    stderr_fallback: bool,
    timeout: Duration,
) -> Option<String> {
    let mut command = Command::new(cmd);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().ok()?;

    let stdout_pipe = child.stdout.take()?;
    let stderr_pipe = child.stderr.take()?;

    // 別スレッドで pipe を空にし続け、子プロセスのブロックを防ぐ。
    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut h = stdout_pipe.take(4 * 1024 * 1024 + 1);
        let _ = h.read_to_end(&mut buf);
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut h = stderr_pipe.take(4 * 1024 * 1024 + 1);
        let _ = h.read_to_end(&mut buf);
        buf
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) | Err(_) => {
                terminate_command(&mut child);
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return None;
            }
        }
    };

    if !status.success() {
        terminate_command(&mut child);
        let _ = stdout_thread.join();
        let _ = stderr_thread.join();
        return None;
    }

    while (!stdout_thread.is_finished() || !stderr_thread.is_finished())
        && start.elapsed() < timeout
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    if !stdout_thread.is_finished() || !stderr_thread.is_finished() {
        terminate_command(&mut child);
        let _ = stdout_thread.join();
        let _ = stderr_thread.join();
        return None;
    }

    let stdout_buf = stdout_thread.join().unwrap_or_default();
    let stderr_buf = stderr_thread.join().unwrap_or_default();

    if stdout_buf.len() > 4 * 1024 * 1024 || stderr_buf.len() > 4 * 1024 * 1024 {
        return None;
    }
    let stdout = String::from_utf8_lossy(&stdout_buf).trim().to_string();
    if !stdout.is_empty() {
        return Some(stdout);
    }

    if !stderr_fallback {
        return None;
    }

    // java -version のように成功時でも標準エラーへ出力するコマンドに対応する。
    Some(String::from_utf8_lossy(&stderr_buf).trim().to_string())
}

fn terminate_command(child: &mut Child) {
    #[cfg(unix)]
    {
        let killed_group = i32::try_from(child.id())
            .ok()
            .is_some_and(|group| unsafe { kill_process_group(group) } == 0);
        if !killed_group {
            let _ = child.kill();
        }
    }
    #[cfg(not(unix))]
    let _ = child.kill();

    let _ = child.wait();
}

#[cfg(unix)]
unsafe fn kill_process_group(group: i32) -> i32 {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }

    // 負の PID は、その絶対値をプロセスグループ ID として扱う。
    unsafe { kill(-group, 9) }
}

pub fn truncate_path(path: &str, max_components: usize, symbol: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= max_components.saturating_add(1) {
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
        let Some(start) = word.find(|c: char| c.is_ascii_digit()) else {
            continue;
        };
        let version = word[start..].trim_end_matches(|c: char| {
            !c.is_ascii_alphanumeric() && c != '.' && c != '-' && c != '_'
        });
        if version.contains('.') && version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return format!("v{version}");
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

/// JSON オブジェクト直下の文字列値を取り出す。
pub fn extract_json_value(json: &str, key: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    value.get(key)?.as_str().map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── truncate_path のテスト ───────────────────────────────────

    #[test]
    fn truncate_path_long_path() {
        let result = truncate_path("~/a/b/c/d", 2, "\u{2026}");
        assert_eq!(result, "~/\u{2026}/c/d");
    }

    #[test]
    fn truncate_path_short_path_unchanged() {
        // "~/a/b" は 3 要素なので、max_components=2 ならそのまま返る。
        assert_eq!(truncate_path("~/a/b", 2, "\u{2026}"), "~/a/b");
    }

    #[test]
    fn truncate_path_exact_boundary() {
        // "~/a/b/c" は境界ちょうどなので、省略せずに返す。
        assert_eq!(truncate_path("~/a/b/c", 3, "\u{2026}"), "~/a/b/c");
    }

    #[test]
    fn truncate_path_with_max_one() {
        let result = truncate_path("~/a/b/c/d", 1, "...");
        assert_eq!(result, "~/.../d");
    }

    #[test]
    fn truncate_path_single_component() {
        assert_eq!(truncate_path("~", 2, "\u{2026}"), "~");
    }

    #[test]
    fn truncate_path_absolute_path() {
        // 絶対パスは先頭が空文字列のため "/.../c/d" の形式になる
        assert_eq!(truncate_path("/a/b/c/d", 2, "\u{2026}"), "/\u{2026}/c/d");
    }

    #[test]
    fn truncate_path_empty_string() {
        // 空文字列は分割しても 1 要素 (空) なのでそのまま返る
        assert_eq!(truncate_path("", 2, "\u{2026}"), "");
    }

    #[test]
    fn truncate_path_root_path() {
        // "/" は分割すると 2 要素 (空, 空) なので max=2 (境界) より大きくはなく返る
        assert_eq!(truncate_path("/", 2, "\u{2026}"), "/");
    }

    #[test]
    fn truncate_path_最大要素数でもオーバーフローしない() {
        assert_eq!(truncate_path("~/a/b", usize::MAX, "\u{2026}"), "~/a/b");
    }

    #[test]
    fn truncate_path_japanese_path() {
        // マルチバイト文字を含むパスでもパニックしない
        let result = truncate_path("~/プロジェクト/サブ/フォルダ/ファイル", 2, "\u{2026}");
        assert!(result.contains("\u{2026}"));
        assert!(result.contains("ファイル"));
    }

    // ── extract_version のテスト ─────────────────────────────────

    #[test]
    fn extract_version_node() {
        assert_eq!(extract_version("v18.12.0"), "v18.12.0");
    }

    #[test]
    fn extract_version_node_with_prefix() {
        assert_eq!(extract_version("node v18.12.0"), "v18.12.0");
    }

    #[test]
    fn extract_version_rustc() {
        assert_eq!(extract_version("rustc 1.75.0"), "v1.75.0");
    }

    #[test]
    fn extract_version_go() {
        assert_eq!(
            extract_version("go version go1.21.5 darwin/arm64"),
            "v1.21.5"
        );
    }

    #[test]
    fn extract_version_go_plain() {
        assert_eq!(extract_version("1.21.5"), "v1.21.5");
    }

    #[test]
    fn extract_version_java_stderr_output() {
        assert_eq!(
            extract_version("openjdk version \"21.0.2\" 2024-01-16"),
            "v21.0.2"
        );
    }

    #[test]
    fn extract_version_python() {
        assert_eq!(extract_version("Python 3.12.0"), "v3.12.0");
    }

    #[test]
    fn extract_version_no_version_returns_original() {
        assert_eq!(extract_version("hello world"), "hello world");
    }

    #[test]
    fn run_cmd_reads_stderr_when_stdout_is_empty() {
        assert_eq!(
            run_cmd("sh", &["-c", "printf 'v21.0.2\\n' >&2"]),
            Some("v21.0.2".to_string())
        );
    }

    #[test]
    fn run_cmd_prefers_stdout_over_stderr() {
        assert_eq!(
            run_cmd("sh", &["-c", "printf 'stdout\\n'; printf 'stderr\\n' >&2"]),
            Some("stdout".to_string())
        );
    }

    #[test]
    fn run_cmd_stdout_標準エラーだけならnone() {
        assert_eq!(
            run_cmd_stdout("sh", &["-c", "printf 'stderr\\n' >&2"]),
            None
        );
    }

    // ── extract_json_value のテスト ──────────────────────────────

    #[test]
    fn extract_json_value_found() {
        let json = r#"{"name":"foo","version":"1.0"}"#;
        assert_eq!(extract_json_value(json, "name"), Some("foo".to_string()));
        assert_eq!(extract_json_value(json, "version"), Some("1.0".to_string()));
    }

    #[test]
    fn extract_json_value_missing_key() {
        let json = r#"{"name":"foo"}"#;
        assert_eq!(extract_json_value(json, "missing"), None);
    }

    #[test]
    fn extract_json_value_with_spaces() {
        let json = r#"{ "name" : "bar" }"#;
        assert_eq!(extract_json_value(json, "name"), Some("bar".to_string()));
    }

    #[test]
    fn extract_json_value_empty_value() {
        let json = r#"{"key":""}"#;
        assert_eq!(extract_json_value(json, "key"), Some("".to_string()));
    }

    // ── has_marker_file のテスト ────────────────────────────────

    #[test]
    fn has_marker_file_exact_match() {
        // 一時ディレクトリにマーカーファイルを作成して検出を確認する
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        assert!(has_marker_file(dir.path(), &["Cargo.toml"]));
    }

    #[test]
    fn has_marker_file_no_match() {
        // マーカーが存在しないディレクトリでは false を返す
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_marker_file(dir.path(), &["nonexistent.txt"]));
    }

    #[test]
    fn has_marker_file_glob_pattern() {
        // ワイルドカード付きマーカー (*.rs) でファイルを検出する
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "").unwrap();
        assert!(has_marker_file(dir.path(), &["*.rs"]));
    }

    #[test]
    fn has_marker_file_glob_no_match() {
        // ワイルドカードに一致するファイルがない場合は false
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.py"), "").unwrap();
        assert!(!has_marker_file(dir.path(), &["*.rs"]));
    }

    #[test]
    fn has_marker_file_traverses_to_parent() {
        // 子ディレクトリから親のマーカーファイルを発見できることを確認する
        let parent = tempfile::tempdir().unwrap();
        let child = parent.path().join("subdir");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(parent.path().join("Cargo.toml"), "").unwrap();
        assert!(has_marker_file(&child, &["Cargo.toml"]));
    }

    // ── find_up のテスト ────────────────────────────────────────

    #[test]
    fn find_up_in_current_dir() {
        // カレントディレクトリに対象ファイルがある場合
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.txt");
        std::fs::write(&target, "").unwrap();
        assert_eq!(find_up(dir.path(), "target.txt"), Some(target));
    }

    #[test]
    fn find_up_in_parent_dir() {
        // 親ディレクトリにある場合、子から探索して発見する
        let parent = tempfile::tempdir().unwrap();
        let child = parent.path().join("child");
        std::fs::create_dir(&child).unwrap();
        let target = parent.path().join("found.txt");
        std::fs::write(&target, "").unwrap();
        assert_eq!(find_up(&child, "found.txt"), Some(target));
    }

    #[test]
    fn find_up_not_found() {
        // どの階層にも存在しないファイルは None を返す
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find_up(dir.path(), "no_such_file.xyz"), None);
    }

    // ── extract_json_value 追加テスト ───────────────────────────

    #[test]
    fn extract_json_value_トップレベルのみ取得する() {
        // package.json の name/version はルート直下だけを対象にする
        let json = r#"{"scripts":{"name":"wrong"},"name":"test"}"#;
        assert_eq!(extract_json_value(json, "name"), Some("test".to_string()));
    }

    #[test]
    fn extract_json_value_number_returns_none() {
        // 値が数値の場合、文字列として解析できないため None を返す
        let json = r#"{"count": 42, "name": "foo"}"#;
        assert_eq!(extract_json_value(json, "count"), None);
    }

    #[test]
    fn extract_json_value_エスケープ済み文字列を扱える() {
        let json = r#"{"name":"foo\"bar"}"#;
        assert_eq!(
            extract_json_value(json, "name"),
            Some("foo\"bar".to_string())
        );
    }

    // ── run_cmd 追加テスト ──────────────────────────────────────

    #[test]
    fn run_cmd_timeout_returns_none() {
        // タイムアウト超過で None を返すことを確認する（sleep 10 は 500ms を超える）
        assert_eq!(run_cmd("sleep", &["10"]), None);
    }

    #[cfg(unix)]
    #[test]
    fn run_cmd_timeout_は子孫プロセスも終了する() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join("survived");
        let marker_arg = marker.to_string_lossy();
        let script = r#"(sleep 1; printf survived > "$1") & wait"#;

        assert_eq!(run_cmd("sh", &["-c", script, "sh", &marker_arg]), None);
        std::thread::sleep(Duration::from_millis(700));
        assert!(
            !marker.exists(),
            "タイムアウト後に子孫プロセスが実行を継続している"
        );
    }

    #[test]
    fn run_cmd_failure_returns_none() {
        // 終了コードが非 0 のコマンドは None を返す
        assert_eq!(run_cmd("sh", &["-c", "exit 1"]), None);
    }

    #[test]
    fn run_cmd_handles_large_output_without_deadlock() {
        // pipe バッファ (約 64KB) を超える出力でもデッドロックせず、
        // タイムアウトしないことを確認する。並行 drain の回帰テスト。
        // 256 KB 相当のデータを yes で生成し、head で打ち切る。
        let result = run_cmd("sh", &["-c", "yes 'x' | head -c 262144"]);
        assert!(
            result.is_some(),
            "大量出力でデッドロックまたはタイムアウトしている"
        );
        let out = result.unwrap();
        assert!(
            out.len() > 1024,
            "出力が短すぎる（pipe drain が機能していない可能性）: {} bytes",
            out.len()
        );
    }
}
