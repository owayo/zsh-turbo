#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn 相対homeから作業ディレクトリの設定を読み込まない() {
    let tmp = tempfile::tempdir().unwrap();
    for home in [".", "home"] {
        let config = tmp.path().join(home).join(".config/zsh-turbo");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("config.toml"),
            "[prompt]\nleft_segments=['probe']\nright_segments=[]\n[[prompt.custom]]\nname='probe'\ncommand='printf LOCAL_CONFIG_LOADED'\n",
        )
        .unwrap();
        for xdg in [None, Some(""), Some("config")] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"));
            command
                .arg("prompt")
                .current_dir(tmp.path())
                .env("HOME", home)
                .env("XDG_STATE_HOME", tmp.path().join("state"))
                .env_remove("XDG_CONFIG_HOME");
            if let Some(xdg) = xdg {
                command.env("XDG_CONFIG_HOME", xdg);
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(
                !String::from_utf8_lossy(&output.stdout).contains("LOCAL_CONFIG_LOADED"),
                "HOME={home:?}, XDG_CONFIG_HOME={xdg:?}"
            );
        }
    }
}

#[test]
fn 相対homeでも絶対xdgの設定は読み込む() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config/zsh-turbo");
    fs::create_dir_all(&config).unwrap();
    fs::write(
        config.join("config.toml"),
        "[prompt]\nleft_segments=['probe']\nright_segments=[]\n[[prompt.custom]]\nname='probe'\ncommand='printf ABSOLUTE_CONFIG_LOADED'\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
        .arg("prompt")
        .current_dir(tmp.path())
        .env("HOME", ".")
        .env("XDG_CONFIG_HOME", tmp.path().join("config"))
        .env("XDG_STATE_HOME", tmp.path().join("state"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("ABSOLUTE_CONFIG_LOADED"));
}

#[test]
fn 相対homeのチルダ経由で作業ディレクトリの補完を取り込まない() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config/zsh-turbo");
    fs::create_dir_all(&config).unwrap();
    fs::write(
        config.join("config.toml"),
        "[[completions]]\ncommand='example'\nsource='file'\nfile='~/_example'\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("_example"),
        "#compdef example\n_arguments '--unsafe'\n",
    )
    .unwrap();
    let cache = tmp.path().join("cache");
    let output = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
        .arg("completion-refresh")
        .current_dir(tmp.path())
        .env("HOME", ".")
        .env("XDG_CONFIG_HOME", tmp.path().join("config"))
        .env("XDG_CACHE_HOME", &cache)
        .env("XDG_STATE_HOME", tmp.path().join("state"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("no absolute home directory"));
    assert!(
        !cache
            .join("zsh-turbo/managed-completions/example.zsh")
            .exists()
    );
}

#[test]
fn 設定やフォントの保存先が相対パスなら書き込まず失敗する() {
    let tmp = tempfile::tempdir().unwrap();
    for command in ["configure", "install-font"] {
        let output = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
            .arg(command)
            .current_dir(tmp.path())
            .env("HOME", "home")
            .env("XDG_CONFIG_HOME", "config")
            .env("XDG_DATA_HOME", "data")
            .env("XDG_STATE_HOME", tmp.path().join("state"))
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{command}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("absolute"),
            "{output:?}"
        );
        assert!(!tmp.path().join("home").exists());
        assert!(!tmp.path().join("config").exists());
        assert!(!tmp.path().join("data").exists());
    }
}

#[test]
fn 相対xdg設定からカレントディレクトリのコードを実行しない() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config/zsh-turbo");
    let home = tmp.path().join("home");
    fs::create_dir_all(&config).unwrap();
    fs::create_dir(&home).unwrap();
    fs::write(config.join("config.toml"), "[[completions]]\ncommand='sh'\nsource='generator'\ngenerator=['/bin/sh','-c','printf executed > marker']\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
        .arg("completion-refresh")
        .current_dir(tmp.path())
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", "config")
        .env("XDG_CACHE_HOME", tmp.path().join("cache"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "git: native"
    );
    assert!(!tmp.path().join("marker").exists());
}

#[test]
fn 入力で起動するヘルプはカレントディレクトリの設定を読まない場所で実行する() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&bin).unwrap();
    fs::create_dir_all(&repo).unwrap();
    let npm = bin.join("npm");
    fs::write(
        &npm,
        format!(
            "#!/bin/sh\npwd > '{}'\nprintf 'Commands:\\n  install  Install packages\\n'\n",
            tmp.path().join("help-cwd").display()
        ),
    )
    .unwrap();
    fs::set_permissions(&npm, fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
        .args(["suggest", "--ui-list", "--", "npm"])
        .current_dir(&repo)
        .env("PATH", format!("{}:/bin:/usr/bin", bin.display()))
        .env("XDG_CONFIG_HOME", tmp.path().join("config"))
        .env("XDG_CACHE_HOME", tmp.path().join("cache"))
        .env("XDG_STATE_HOME", tmp.path().join("state"))
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("item\tinstall"));
    assert_eq!(
        fs::read_to_string(tmp.path().join("help-cwd"))
            .unwrap()
            .trim(),
        "/"
    );
}

#[test]
fn 補完生成もカレントディレクトリの設定を読まない場所で実行する() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config/zsh-turbo");
    let repo = tmp.path().join("repo");
    let marker = tmp.path().join("generator-cwd");
    fs::create_dir_all(&config).unwrap();
    fs::create_dir(&repo).unwrap();
    let registration = format!(
        "[[completions]]\ncommand='sh'\nsource='generator'\ngenerator={}\n",
        serde_json::to_string(&[
            "/bin/sh",
            "-c",
            "pwd > \"$1\"; printf '%s\\n' '#compdef sh' '_demo() { _arguments \"--flag[Test]\"; }' 'compdef _demo sh'",
            "generator",
            marker.to_str().unwrap(),
        ])
        .unwrap()
    );
    fs::write(config.join("config.toml"), registration).unwrap();
    let cache = tmp.path().join("cache");
    let output = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
        .arg("completion-refresh")
        .current_dir(&repo)
        .env("PATH", "/bin:/usr/bin")
        .env("XDG_CONFIG_HOME", tmp.path().join("config"))
        .env("XDG_CACHE_HOME", &cache)
        .env("XDG_STATE_HOME", tmp.path().join("state"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "sh: updated"
    );
    assert_eq!(fs::read_to_string(marker).unwrap().trim(), "/");
    assert!(cache.join("zsh-turbo/managed-completions/sh.zsh").is_file());
}

#[test]
fn プロンプトは相対pathから見つかった実行ファイルを起動しない() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config/zsh-turbo");
    let work = tmp.path().join("work");
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(work.join(".git")).unwrap();
    fs::write(
        config.join("config.toml"),
        "[prompt]\nleft_segments=['git']\nright_segments=[]\nnewline=false\n",
    )
    .unwrap();
    let git = work.join("git");
    fs::write(
        &git,
        "#!/bin/sh\nprintf executed > marker\nprintf '## main\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o755)).unwrap();
    for path in [".:/bin:/usr/bin", ":/bin:/usr/bin", "../work:/bin:/usr/bin"] {
        let output = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
            .args(["prompt", "--side", "both"])
            .current_dir(&work)
            .env("PATH", path)
            .env("XDG_CONFIG_HOME", tmp.path().join("config"))
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!work.join("marker").exists(), "{path}");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("main"));
    }
}
