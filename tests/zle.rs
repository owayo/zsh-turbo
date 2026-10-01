#[cfg(unix)]
#[test]
fn 実zleで薄い候補と履歴選択と通常補完が動作する() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir(root.join("zsh-turbo")).unwrap();
    std::fs::create_dir(root.join("candidate-dir")).unwrap();
    // ファイル一覧の選択用。パン.txt は macOS でよく見る NFD 形式の名前で置く。
    std::fs::create_dir_all(root.join("books/sub")).unwrap();
    for name in [
        "Alpha Beta.txt",
        "alpha.md",
        ".hidden",
        "\u{30cf}\u{309a}\u{30f3}.txt",
        "sub/inner.txt",
    ] {
        std::fs::write(root.join("books").join(name), "").unwrap();
    }
    std::fs::write(
        root.join("Makefile"),
        "build:\n\t@true\ncheck:\n\t@true\nclean:\n\t@true\ndeploy:\n\t@true\n",
    )
    .unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"scripts":{"dev":"vite","test":"node test.js"}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("pyproject.toml"),
        "[project.scripts]\nhello = 'example:main'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("deno.jsonc"),
        "{ // task\n \"tasks\": {\"check\": \"deno check .\",},\n}",
    )
    .unwrap();
    std::fs::write(root.join("mise.toml"), "[tasks]\nbuild = 'echo build'\n").unwrap();
    std::fs::write(root.join("justfile"), "check:\n  echo check\n").unwrap();
    std::fs::write(
        root.join("Taskfile.yml"),
        "version: '3'\ntasks:\n  build:\n    cmds: ['echo build']\n",
    )
    .unwrap();
    std::fs::write(root.join("zsh-turbo/config.toml"), "[prompt]\nleft_segments=[]\nright_segments=[]\nnewline=false\n[suggest]\nmax_suggestions=10\n").unwrap();
    let mut history = Vec::new();
    let commands = "echo sample-alpha\necho sample-alpha\necho sample-beta\necho 日本語\nls -l /path/to/hoge/fuga\nmake busted\npnpm deploy\n";
    for byte in commands.bytes() {
        if byte >= 0x80 {
            history.extend([0x83, byte ^ 0x20]);
        } else {
            history.push(byte);
        }
    }
    std::fs::write(root.join("history"), history).unwrap();
    // サブコマンド一覧の確認用に、決まった --help を返す偽物を置く
    let fake_bin = root.join("fakebin");
    std::fs::create_dir(&fake_bin).unwrap();
    let fake_uv = fake_bin.join("uv");
    std::fs::write(
        &fake_uv,
        "#!/bin/sh\n[ \"$1\" = --help ] || exit 1\ncat <<'EOF'\nUsage: uv [OPTIONS] <COMMAND>\n\nCommands:\n  run   Run a command or script\n  sync  Update the project's environment\n  self  Manage the uv executable\n\nOptions:\n  -q, --quiet  Use quiet output\nEOF\n",
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_uv, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::copy(&fake_uv, fake_bin.join("pnpm")).unwrap();
    }
    std::fs::write(root.join(".zshrc"), include_str!("zle-init.zsh")).unwrap();
    let harness = root.join("test.zsh");
    std::fs::write(&harness, include_str!("zle-driver.zsh")).unwrap();
    let binary = env!("CARGO_BIN_EXE_zsh-turbo");
    // 候補用の履歴にも実行ディレクトリを記録し、共通履歴からの混入に依存しない。
    for line in commands.lines() {
        use std::io::Write;
        let mut child = std::process::Command::new(binary)
            .arg("record")
            .current_dir(root)
            .env("XDG_CONFIG_HOME", root)
            .env("XDG_STATE_HOME", root.join("state"))
            .stdin(std::process::Stdio::piped())
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
    let bin_dir = std::path::Path::new(binary).parent().unwrap();
    let output = std::process::Command::new("zsh")
        .args(["-df", harness.to_str().unwrap()])
        .current_dir(root)
        .env("TEST_ROOT", root)
        .env("TEST_BINARY", binary)
        .env("ZDOTDIR", root)
        .env("XDG_CONFIG_HOME", root)
        .env("XDG_CACHE_HOME", root.join("cache"))
        // 実行したタスクの利用記録を実環境の ~/.local/state へ書かない
        .env("XDG_STATE_HOME", root.join("state"))
        .env("ZSH_TURBO_TERM_SHELL_INTEGRATION", "0")
        .env("ZSH_TURBO_TRANSIENT", "0")
        .env("ZSH_TURBO_SUGGEST_STRATEGY", "prefix")
        .env("ZSH_TURBO_SUGGEST_HIGHLIGHT", "fg=8")
        .env("TERM", "xterm-256color")
        .env("LC_ALL", "en_US.UTF-8")
        .env(
            "PATH",
            format!(
                "{}:{}:{}",
                fake_bin.display(),
                bin_dir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("ZLE OK"));
}
