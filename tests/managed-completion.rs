#[cfg(unix)]
#[test]
fn 相対pathや補完元から入力時にコードを実行しない() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let work = root.path().join("work");
    let config = root.path().join("config");
    fs::create_dir(&work).unwrap();
    fs::create_dir_all(config.join("zsh-turbo")).unwrap();
    let executable = work.join("demo");
    fs::write(
        &executable,
        "#!/bin/sh\nprintf 'executed\\n' >> marker\nprintf 'Options:\\n  --flag  Test flag\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    for path in [":/bin:/usr/bin", ".:/bin:/usr/bin", "../work:/bin:/usr/bin"] {
        for source in ["", "source='generator'\ngenerator=['demo']\n"] {
            fs::write(
                config.join("zsh-turbo/config.toml"),
                format!("[[completions]]\ncommand='demo'\n{source}"),
            )
            .unwrap();
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
                .arg("completion-refresh")
                .current_dir(&work)
                .env("PATH", path)
                .env("XDG_CONFIG_HOME", &config)
                .env("XDG_CACHE_HOME", root.path().join("cache"))
                .output()
                .unwrap();
            assert!(output.status.success());
            assert!(String::from_utf8_lossy(&output.stdout).contains("relative PATH"));
            assert!(!work.join("marker").exists());
        }
    }
    for source in [
        "source='generator'\ngenerator=['./demo']",
        "source='file'\nfile='./_demo'",
    ] {
        fs::write(
            config.join("zsh-turbo/config.toml"),
            format!("[[completions]]\ncommand='demo'\n{source}\n"),
        )
        .unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
            .arg("completion-refresh")
            .current_dir(&work)
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_CACHE_HOME", root.path().join("cache"))
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).contains("absolute path"));
        assert!(!work.join("marker").exists());
    }
    let zsh = work.join("zsh");
    fs::copy(&executable, &zsh).unwrap();
    let source = root.path().join("_demo");
    fs::write(&source, "#compdef demo\n_arguments '--flag[Test flag]'\n").unwrap();
    let source = toml::Value::String(source.to_string_lossy().into()).to_string();
    fs::write(
        config.join("zsh-turbo/config.toml"),
        format!("[[completions]]\ncommand='demo'\nsource='file'\nfile={source}\n"),
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
        .arg("completion-refresh")
        .current_dir(&work)
        .env("PATH", ".:/bin:/usr/bin")
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_CACHE_HOME", root.path().join("cache"))
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("relative PATH"));
    assert!(!work.join("marker").exists());
}

#[cfg(unix)]
#[test]
fn 非ゼロ終了のヘルプも読みシムの呼び出し名を保つ() {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    let config = root.path().join("config");
    let cache = root.path().join("cache");
    fs::create_dir(&bin).unwrap();
    fs::create_dir_all(config.join("zsh-turbo")).unwrap();
    fs::write(
        config.join("zsh-turbo/config.toml"),
        "[[completions]]\ncommand='demo'\n",
    )
    .unwrap();
    let actual = bin.join("actual");
    fs::write(&actual, "#!/bin/sh\ncase \"$0\" in */demo) ;; *) exit 2 ;; esac\n[ \"$LC_ALL\" = C ] || exit 2\nprintf 'Options:\\n  --[no-]color  Colorize output\\n'\nexit 1\n").unwrap();
    fs::set_permissions(&actual, fs::Permissions::from_mode(0o755)).unwrap();
    symlink(&actual, bin.join("demo")).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
        .arg("completion-refresh")
        .env("PATH", format!("{}:/bin:/usr/bin", bin.display()))
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_CACHE_HOME", &cache)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "demo: updated"
    );
    let script = fs::read_to_string(cache.join("zsh-turbo/managed-completions/demo.zsh")).unwrap();
    assert!(script.contains("--color[Colorize output]"));
    assert!(script.contains("--no-color[Colorize output]"));
}

#[cfg(unix)]
#[test]
fn registered_completions_refresh_after_binary_replacement_in_a_running_shell() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join("zsh-turbo")).unwrap();
    fs::create_dir(root.join("bin")).unwrap();
    let fixture = "#!/bin/sh\ncase \"$*\" in\n'--help') printf 'Options:\\n  --before  Before update\\n  --config <FILE>  Config file\\nCommands:\\n  remote  Remote commands\\n';;\n'remote --help') printf 'Options:\\n  --transport <MODE>  Transport\\n';;\nesac\n";
    // サブコマンドを持つ CLI でも、ファイル引数は matcher-list の部分一致で補完できる
    fs::write(root.join("sample-settings.toml"), "").unwrap();
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
        .env("XDG_STATE_HOME", root.join("state"))
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
