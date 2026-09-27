#[cfg(unix)]
#[test]
fn registered_completions_refresh_after_binary_replacement_in_a_running_shell() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join("zsh-turbo")).unwrap();
    fs::create_dir(root.join("bin")).unwrap();
    let fixture = "#!/bin/sh\ncase \"$*\" in\n'--help') printf 'Options:\\n  --before  Before update\\nCommands:\\n  remote  Remote commands\\n';;\n'remote --help') printf 'Options:\\n  --transport <MODE>  Transport\\n';;\nesac\n";
    for name in ["demohelp", "demogen", "demofile"] {
        let path = root.join("bin").join(name);
        fs::write(&path, fixture).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::write(root.join("generator"), "printf '%s\\n' '#compdef demogen' '_demo_native() { _arguments \"--native[Native option]\"; }' 'compdef _demo_native demogen'\n").unwrap();
    fs::write(
        root.join("_demofile"),
        "#compdef demofile\n_arguments '--file-option[File option]'\n",
    )
    .unwrap();
    let quote = |path: &std::path::Path| format!("'{}'", path.display());
    let config = format!(
        "[[completions]]\ncommand='demohelp'\n[[completions]]\ncommand='demogen'\nsource='generator'\ngenerator=['/bin/sh',{}]\n[[completions]]\ncommand='demofile'\nsource='file'\nfile={}\n[prompt]\nleft_segments=[]\nright_segments=[]\nnewline=false\n",
        quote(&root.join("generator")),
        quote(&root.join("_demofile"))
    );
    fs::write(root.join("zsh-turbo/config.toml"), config).unwrap();
    fs::write(root.join(".zshrc"), include_str!("zle-init.zsh")).unwrap();
    fs::write(root.join("history"), "").unwrap();
    let driver = include_str!("zle-driver.zsh")
        .split("zpty -b fixture")
        .next()
        .unwrap()
        .to_string()
        + include_str!("managed-completion.zsh");
    fs::write(root.join("driver.zsh"), driver).unwrap();
    let binary = env!("CARGO_BIN_EXE_zsh-turbo");
    let bin = std::path::Path::new(binary).parent().unwrap();
    let output = std::process::Command::new("zsh")
        .args(["-df", "driver.zsh"])
        .current_dir(root)
        .env("TEST_ROOT", root)
        .env("TEST_BINARY", binary)
        .env("ZDOTDIR", root)
        .env("XDG_CONFIG_HOME", root)
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("ZSH_TURBO_TERM_SHELL_INTEGRATION", "0")
        .env("TERM", "xterm-256color")
        .env(
            "PATH",
            format!(
                "{}:{}:{}",
                root.join("bin").display(),
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .unwrap();
    if !output.status.success() {
        eprintln!(
            "terminal: {}",
            fs::read_to_string(root.join("terminal")).unwrap_or_default()
        );
        eprintln!(
            "cache: {:?}",
            fs::read_to_string(root.join("cache/zsh-turbo/managed-completions/demohelp.zsh"))
        );
    }
    assert!(
        output.status.success(),
        "{}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
