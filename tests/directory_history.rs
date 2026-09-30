use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn cli(root: &Path, dir: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"));
    command
        .current_dir(root.join(dir))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("HISTFILE", root.join("history"));
    command
}

fn setup() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for dir in ["app", "desktop", "app/child", "config/zsh-turbo"] {
        std::fs::create_dir_all(tmp.path().join(dir)).unwrap();
    }
    std::fs::write(tmp.path().join("history"), "pnpm tauri:build\n").unwrap();
    tmp
}

fn record(root: &Path, dir: &str, line: &str) {
    let mut child = cli(root, dir)
        .arg("record")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(line.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}

fn output(command: &mut Command) -> String {
    let output = command.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn 候補とghostと履歴メニューは現在のディレクトリだけを検索する() {
    let tmp = setup();
    let root = tmp.path();
    record(root, "desktop", "pnpm tauri:build");
    assert_eq!(output(cli(root, "app").args(["suggest", "pn"])), "");
    record(root, "app", "pnpm run");
    assert_eq!(output(cli(root, "app").args(["suggest", "pn"])), "pnpm run");
    assert_eq!(
        output(cli(root, "desktop").args(["suggest", "pn"])),
        "pnpm tauri:build"
    );
    assert_eq!(output(cli(root, "app/child").args(["suggest", "pn"])), "");
    // シェルは HISTFILE が明示されてもディレクトリを優先する。
    let response = output(cli(root, "app").args([
        "suggest",
        "--directory-history",
        "--ui-list",
        "--history-file",
        root.join("history").to_str().unwrap(),
        "--",
        "pn",
    ]));
    assert!(response.contains("ghost\tpnpm run\n"), "{response}");
    assert!(!response.contains("tauri"), "{response}");
    assert_eq!(
        output(cli(root, "app").args([
            "complete",
            "--directory-history",
            "--strategy",
            "fuzzy",
            "--",
            "pn",
        ])),
        "pnpm run\n"
    );
    // 明示的な履歴ファイルを検索する公開 CLI の使い方も維持する。
    assert_eq!(
        output(cli(root, "app").args([
            "suggest",
            "--history-file",
            root.join("history").to_str().unwrap(),
            "--",
            "pn",
        ])),
        "pnpm tauri:build"
    );
}

#[test]
fn 日本語とコロンで始まる通常コマンドを保持し空白や制御文字や長すぎる行は記録しない() {
    let tmp = setup();
    let root = tmp.path();
    for line in [
        " echo secret",
        "echo first\necho second",
        "echo\tsecret",
        "echo\0secret",
    ] {
        record(root, "app", line);
    }
    record(root, "app", &format!("echo {}", "x".repeat(4096)));
    assert_eq!(
        output(cli(root, "app").args(["complete", "--strategy", "fuzzy", "--", ""])),
        ""
    );
    record(root, "app", "echo 日本語");
    record(root, "app", ": 123:0;echo literal");
    record(root, "app", "echo trailing\\");
    assert_eq!(
        output(cli(root, "app").args(["suggest", "echo 日"])),
        "echo 日本語"
    );
    assert_eq!(
        output(cli(root, "app").args(["suggest", ": 123"])),
        ": 123:0;echo literal"
    );
    assert_eq!(
        output(cli(root, "app").args(["suggest", "echo trail"])),
        "echo trailing\\"
    );
}

#[test]
fn 記録を無効にした場合と設定が読めない場合は既存履歴を変更しない() {
    let tmp = setup();
    let root = tmp.path();
    record(root, "app", "pnpm run");
    for config in [
        "[suggest]\nrecord_directory_history = false\n",
        "[suggest\n",
    ] {
        std::fs::write(root.join("config/zsh-turbo/config.toml"), config).unwrap();
        record(root, "app", "pnpm tauri:build");
    }
    std::fs::write(root.join("config/zsh-turbo/config.toml"), "").unwrap();
    assert_eq!(output(cli(root, "app").args(["suggest", "pn"])), "pnpm run");
}

#[cfg(unix)]
#[test]
fn 同じ実体のディレクトリは履歴を共有する() {
    let tmp = setup();
    let root = tmp.path();
    std::os::unix::fs::symlink(root.join("app"), root.join("alias")).unwrap();
    record(root, "alias", "pnpm run");
    assert_eq!(output(cli(root, "app").args(["suggest", "pn"])), "pnpm run");
}

#[cfg(unix)]
#[test]
fn 実際のpreexecは通常コマンドも実行前のディレクトリに記録する() {
    let tmp = setup();
    let root = tmp.path();
    let quote = |text: &str| format!("'{}'", text.replace('\'', "'\\''"));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/init.zsh");
    let script = format!(
        "source {}\nZSH_TURBO_CMD={}\n_zsh_turbo_preexec 'echo 日本語'\ncd ../desktop\n_zsh_turbo_preexec 'pnpm tauri:build'\nrepeat 100; do [[ \"$(\"$ZSH_TURBO_CMD\" suggest pn)\" == 'pnpm tauri:build' ]] && break; sleep 0.05; done\n",
        quote(source.to_str().unwrap()),
        quote(env!("CARGO_BIN_EXE_zsh-turbo")),
    );
    let result = Command::new("zsh")
        .args(["-dfc", &script])
        .current_dir(root.join("app"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("ZDOTDIR", root)
        .env("ZSH_TURBO_RECORD_DIRECTORY_HISTORY", "1")
        .env("ZSH_TURBO_RECORD_TASK_USAGE", "0")
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    assert_eq!(
        output(cli(root, "app").args(["suggest", "echo"])),
        "echo 日本語"
    );
    assert_eq!(output(cli(root, "app").args(["suggest", "pn"])), "");
    assert_eq!(
        output(cli(root, "desktop").args(["suggest", "pn"])),
        "pnpm tauri:build"
    );
    use std::os::unix::fs::PermissionsExt;
    let history_dir = root.join("state/zsh-turbo/directory-history");
    assert_eq!(
        std::fs::metadata(&history_dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    for file in std::fs::read_dir(history_dir).unwrap() {
        assert_eq!(
            file.unwrap().metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
