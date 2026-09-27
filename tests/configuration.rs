use std::process::Command;

fn cli(config_home: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"));
    command.env("XDG_CONFIG_HOME", config_home);
    command
}

#[test]
fn 左右一括描画は個別描画と空行や制御文字を含めて一致する() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("zsh-turbo");
    std::fs::create_dir(&dir).unwrap();
    for right in ["[]", "['os_icon']"] {
        std::fs::write(dir.join("config.toml"), format!(
            "[prompt]\nblank_lines=2\nnewline=true\nleft_segments=['os_icon','dir']\nright_segments={right}\n"
        )).unwrap();
        let render = |side| {
            let out = cli(tmp.path())
                .args(["prompt", "--side", side])
                .output()
                .unwrap();
            assert!(out.status.success());
            out.stdout
        };
        let mut expected = render("left");
        expected.push(0);
        expected.extend(render("right"));
        expected.push(0);
        assert_eq!(render("both"), expected);
    }
}

#[test]
fn 補完候補数は設定を使いcli指定で上書きできる() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("zsh-turbo");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("config.toml"), "[suggest]\nmax_suggestions = 2\n").unwrap();
    let history = tmp.path().join("history");
    std::fs::write(&history, "git status\ngit log\ngit diff\n").unwrap();
    for (args, count) in [
        (vec![], 2),
        (vec!["--max", "1"], 1),
        (vec!["--max", "0"], 0),
    ] {
        let output = cli(tmp.path())
            .args(["complete", "git", "--history-file"])
            .arg(&history)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().lines().count(),
            count
        );
    }
}

#[cfg(unix)]
#[test]
fn ipセグメントは設定を使い環境変数指定を優先する() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("zsh-turbo");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(
        dir.join("config.toml"),
        "[prompt]\nleft_segments = ['ip']\nip_interface = 'config-iface'\n",
    )
    .unwrap();
    let command = tmp.path().join(if cfg!(target_os = "macos") {
        "ipconfig"
    } else {
        "ip"
    });
    let script = if cfg!(target_os = "macos") {
        "#!/bin/sh\nprintf '%s\\n' \"$2\"\n"
    } else {
        "#!/bin/sh\nprintf '2: test inet %s/24 scope global\\n' \"$6\"\n"
    };
    std::fs::write(&command, script).unwrap();
    std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755)).unwrap();
    for (override_value, expected) in [(None, "config-iface"), (Some("env-iface"), "env-iface")] {
        let mut command = cli(tmp.path());
        command
            .args(["prompt", "--side", "left"])
            .env("PATH", tmp.path())
            .env_remove("ZSH_TURBO_IP_INTERFACE");
        if let Some(value) = override_value {
            command.env("ZSH_TURBO_IP_INTERFACE", value);
        }
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8(output.stdout).unwrap().contains(expected));
    }
}

#[cfg(unix)]
#[test]
fn 初期化は設定を補完初期化前に渡しシェル変数を優先する() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("zsh-turbo");
    std::fs::create_dir(&dir).unwrap();
    let completion_dir = tmp.path().join("completions ' 日本語");
    std::fs::create_dir(&completion_dir).unwrap();
    std::fs::write(
        completion_dir.join("_zt_tui_test"),
        "#compdef zt-tui-test\n",
    )
    .unwrap();
    let marker = tmp.path().join("unexpected-command");
    let payload = format!(
        "fg=8'$(touch {})`touch {}`",
        marker.display(),
        marker.display()
    );
    let config = serde_json::json!({
        "prompt": {"transient": true},
        "suggest": {"strategy": "fuzzy", "highlight_color": payload,
            "keys": {"tab": "default", "right": "full", "alt_f": "step", "ctrl_right": "default"}},
        "shell": {"completion_dirs": completion_dir.to_str().unwrap(), "term_shell_integration": "0"}
    });
    std::fs::write(dir.join("config.toml"), toml::to_string(&config).unwrap()).unwrap();
    let output = cli(tmp.path()).arg("init").output().unwrap();
    assert!(output.status.success());
    let script = tmp.path().join("init.zsh");
    std::fs::write(&script, output.stdout).unwrap();
    for override_values in [false, true] {
        let code = r#"
# runner の追加補完を除外し、標準の autoload 関数だけを使う。
fpath=(
  ${(M)fpath:#*/zsh/functions}
  ${(M)fpath:#*/zsh/functions/*}
  ${(M)fpath:#*/zsh/*/functions}
  ${(M)fpath:#*/zsh/*/functions/*}
)
commands[zsh-turbo]=/usr/bin/true
setopt SH_GLOB NO_UNSET
source "$1"
print -rl -- "$ZSH_TURBO_TRANSIENT" "$ZSH_TURBO_SUGGEST_STRATEGY" "$ZSH_TURBO_SUGGEST_HIGHLIGHT" "$ZSH_TURBO_COMPLETION_DIRS" "$ZSH_TURBO_TERM_SHELL_INTEGRATION"
print -rl -- "$ZSH_TURBO_KEY_TAB" "$ZSH_TURBO_KEY_RIGHT" "$ZSH_TURBO_KEY_ALT_F" "$ZSH_TURBO_KEY_CTRL_RIGHT"
print -r -- "${_comps[zt-tui-test]:-missing}"
"#;
        let mut command = Command::new("zsh");
        command
            .args(["-dfi", "-c", code, "zsh"])
            .arg(&script)
            .env("ZDOTDIR", tmp.path())
            .env("TERM_PROGRAM", "test")
            .env("TERM_PROGRAM_VERSION", "0")
            .env("ZSH_COMPDUMP", tmp.path().join("dump"));
        for name in [
            "ZSH_TURBO_TRANSIENT",
            "ZSH_TURBO_SUGGEST_STRATEGY",
            "ZSH_TURBO_SUGGEST_HIGHLIGHT",
            "ZSH_TURBO_COMPLETION_DIRS",
            "ZSH_TURBO_TERM_SHELL_INTEGRATION",
            "ZSH_TURBO_KEY_TAB",
            "ZSH_TURBO_KEY_RIGHT",
            "ZSH_TURBO_KEY_ALT_F",
            "ZSH_TURBO_KEY_CTRL_RIGHT",
        ] {
            command.env_remove(name);
        }
        if override_values {
            command
                .env("ZSH_TURBO_TRANSIENT", "0")
                .env("ZSH_TURBO_SUGGEST_STRATEGY", "prefix")
                .env("ZSH_TURBO_SUGGEST_HIGHLIGHT", "fg=3")
                .env("ZSH_TURBO_COMPLETION_DIRS", "")
                .env("ZSH_TURBO_TERM_SHELL_INTEGRATION", "1")
                .env("ZSH_TURBO_KEY_TAB", "full")
                .env("ZSH_TURBO_KEY_RIGHT", "step")
                .env("ZSH_TURBO_KEY_ALT_F", "word")
                .env("ZSH_TURBO_KEY_CTRL_RIGHT", "word");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let lines: Vec<_> = stdout.lines().collect();
        if override_values {
            assert_eq!(&lines[..5], ["0", "prefix", "fg=3", "", "1"]);
            assert_eq!(&lines[5..9], ["full", "step", "word", "word"]);
        } else {
            assert_eq!(
                &lines[..5],
                [
                    "1",
                    "fuzzy",
                    &payload,
                    completion_dir.to_str().unwrap(),
                    "0"
                ]
            );
            assert_eq!(&lines[5..9], ["default", "full", "step", "default"]);
        }
        assert_eq!(lines[9], "_zt_tui_test");
        assert!(!marker.exists());
    }
}

#[test]
fn 配色と入力位置が設定から実際のプロンプトに反映される() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("zsh-turbo");
    std::fs::create_dir(&dir).unwrap();
    for style in ["lean", "classic", "rainbow", "pure"] {
        for newline in [false, true] {
            std::fs::write(dir.join("config.toml"), format!(
                "[prompt]\nprompt_style = '{style}'\nnewline = {newline}\nfont_level = 'ascii'\nleft_segments = ['os_icon', 'dir']\nright_segments = ['os_icon']\n[style]\nrainbow_palette = 'blue'\n"
            )).unwrap();
            let left = cli(tmp.path())
                .args(["prompt", "--side", "left"])
                .output()
                .unwrap();
            assert!(left.status.success());
            let left = String::from_utf8(left.stdout).unwrap();
            assert_eq!(
                left.matches('\n').count(),
                usize::from(newline),
                "{style}: {left:?}"
            );
            if style == "rainbow" {
                assert!(left.contains("48;5;24"));
                assert!(left.contains("48;5;153"));
            }
            let right = cli(tmp.path())
                .args(["prompt", "--side", "right"])
                .output()
                .unwrap();
            assert!(right.status.success());
            let right = String::from_utf8(right.stdout).unwrap();
            assert!(!right.contains('\n'));
            assert!(!right.contains("48;5;"));
        }
    }
}

#[test]
fn 個別配色と境界色は非表示ブロックや並べ替えに影響されない() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("zsh-turbo");
    std::fs::create_dir(&dir).unwrap();
    for names in [
        "['status', 'os_icon', 'dir']",
        "['dir', 'status', 'os_icon']",
    ] {
        for status in ["0", "1"] {
            std::fs::write(
                dir.join("config.toml"),
                format!(
                    r##"
[prompt]
prompt_style = 'rainbow'
font_level = 'ascii'
left_segments = {names}
right_segments = ['dir']
[style.rainbow_overrides.os_icon]
fg = '231'
bg = '24'
[style.rainbow_overrides.dir]
fg = '16'
bg = '#f0c674'
"##
                ),
            )
            .unwrap();
            let output = cli(tmp.path())
                .args(["prompt", "--side", "left", "--last-status", status])
                .output()
                .unwrap();
            assert!(output.status.success(), "{:?}", output);
            let left = String::from_utf8(output.stdout).unwrap();
            assert!(left.contains("38;5;16;48;2;240;198;116"), "{left:?}");
            assert!(left.contains("38;5;231;48;5;24"), "{left:?}");
            if names.starts_with("['status'") {
                assert!(left.contains("38;5;24;48;2;240;198;116"), "{left:?}");
            }
            let output = cli(tmp.path())
                .args(["prompt", "--side", "right"])
                .output()
                .unwrap();
            assert!(output.status.success());
            assert!(!String::from_utf8(output.stdout).unwrap().contains("48;"));
        }
    }
}

#[test]
fn 空行数は入力位置と独立し全スタイルの左側だけに適用される() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("zsh-turbo");
    std::fs::create_dir(&dir).unwrap();
    for style in ["lean", "classic", "rainbow", "pure"] {
        for newline in [false, true] {
            for count in [0, 1, 3, 100] {
                std::fs::write(dir.join("config.toml"), format!(
                    "[prompt]\nprompt_style = '{style}'\nnewline = {newline}\nblank_lines = {count}\nfont_level = 'ascii'\nleft_segments = ['os_icon']\nright_segments = ['os_icon']\n"
                )).unwrap();
                for side in ["left", "right"] {
                    let output = cli(tmp.path())
                        .args(["prompt", "--side", side])
                        .output()
                        .unwrap();
                    assert!(output.status.success());
                    let expected = if side == "left" { count.min(10) } else { 0 };
                    assert_eq!(
                        output.stdout.iter().take_while(|b| **b == b'\n').count(),
                        expected
                    );
                    assert_eq!(
                        output.stdout.iter().filter(|b| **b == b'\n').count(),
                        expected + usize::from(newline && side == "left")
                    );
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn 再描画とtransientでも空行数と端末マーカーを維持する() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("zsh-turbo");
    std::fs::create_dir(&dir).unwrap();
    let script = tmp.path().join("prompt.zsh");
    let source = include_str!("../shell/init.zsh");
    std::fs::write(
        &script,
        source.split("# ─── オートサジェスト連携").next().unwrap(),
    )
    .unwrap();
    for count in [0, 1, 3] {
        std::fs::write(dir.join("config.toml"), format!(
            "[prompt]\nblank_lines = {count}\nnewline = false\nleft_segments = ['os_icon']\nright_segments = []\n"
        )).unwrap();
        for markers in ["0", "1"] {
            let code = r#"
setopt SH_GLOB NO_UNSET
source "$1"
ZSH_TURBO_CMD="$2"
ZSH_TURBO_TRANSIENT=1
ZSH_TURBO_TERM_SHELL_INTEGRATION="$3"
function zle() { return 0; }
function _zsh_turbo_close_async_fd() { return 0; }
function _zsh_turbo_clear_suggestion() { return 0; }
_ZSH_TURBO_ASYNC_FD=0
_ZSH_TURBO_HIGHLIGHT_FD=0
_zsh_turbo_render_prompt
print -rn -- "$PROMPT"$'\0'
_zsh_turbo_render_prompt
print -rn -- "$PROMPT"$'\0'
ZSH_TURBO_VI_MODE=insert
KEYMAP=vicmd
_zsh_turbo_zle_keymap_select
print -rn -- "$PROMPT"$'\0'
_zsh_turbo_zle_line_finish
print -rn -- "$PROMPT"$'\0'
_zsh_turbo_zle_line_finish
print -rn -- "$PROMPT"$'\0'
"#;
            let output = Command::new("zsh")
                .args(["-dfc", code, "zsh"])
                .arg(&script)
                .arg(env!("CARGO_BIN_EXE_zsh-turbo"))
                .arg(markers)
                .env("XDG_CONFIG_HOME", tmp.path())
                .env("TERM_PROGRAM", "test")
                .env("TERM_PROGRAM_VERSION", "0")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            let prompts: Vec<_> = stdout.split_terminator('\0').collect();
            assert_eq!(prompts.len(), 5);
            for prompt in &prompts {
                let body = prompt
                    .strip_prefix("%{\u{1b}]133;A\u{7}%}")
                    .unwrap_or(prompt);
                assert_eq!(body.bytes().take_while(|b| *b == b'\n').count(), count);
                assert_eq!(
                    prompt.matches("\u{1b}]133;A\u{7}").count(),
                    usize::from(markers == "1")
                );
            }
            assert_eq!(prompts[0], prompts[1]);
            assert_eq!(prompts[0], prompts[2]);
            assert_eq!(prompts[3], prompts[4]);
        }
    }
}
