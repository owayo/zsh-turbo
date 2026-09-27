#[cfg(unix)]
#[test]
fn 実zleで薄い候補と履歴選択と通常補完が動作する() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir(root.join("zsh-turbo")).unwrap();
    std::fs::create_dir(root.join("candidate-dir")).unwrap();
    std::fs::write(root.join("zsh-turbo/config.toml"), "[prompt]\nleft_segments=[]\nright_segments=[]\nnewline=false\n[suggest]\nmax_suggestions=10\n").unwrap();
    let mut history = Vec::new();
    for byte in "echo sample-alpha\necho sample-alpha\necho sample-beta\necho 日本語\nls -l /path/to/hoge/fuga\n".bytes() {
        if byte >= 0x80 {
            history.extend([0x83, byte ^ 0x20]);
        } else {
            history.push(byte);
        }
    }
    std::fs::write(root.join("history"), history).unwrap();
    std::fs::write(root.join(".zshrc"), include_str!("zle-init.zsh")).unwrap();
    let harness = root.join("test.zsh");
    std::fs::write(&harness, include_str!("zle-driver.zsh")).unwrap();
    let binary = env!("CARGO_BIN_EXE_zsh-turbo");
    let bin_dir = std::path::Path::new(binary).parent().unwrap();
    let output = std::process::Command::new("zsh")
        .args(["-df", harness.to_str().unwrap()])
        .current_dir(root)
        .env("TEST_ROOT", root)
        .env("TEST_BINARY", binary)
        .env("ZDOTDIR", root)
        .env("XDG_CONFIG_HOME", root)
        .env("ZSH_TURBO_TERM_SHELL_INTEGRATION", "0")
        .env("ZSH_TURBO_TRANSIENT", "0")
        .env("ZSH_TURBO_SUGGEST_STRATEGY", "prefix")
        .env("ZSH_TURBO_SUGGEST_HIGHLIGHT", "fg=8")
        .env("TERM", "xterm-256color")
        .env("LC_ALL", "en_US.UTF-8")
        .env(
            "PATH",
            format!(
                "{}:{}",
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
