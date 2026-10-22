use super::*;
use std::fs;

#[test]
fn チルダを展開しても補完元と監視ファイルは絶対パスに限る() {
    for home in [None, Some(""), Some("."), Some("home")] {
        assert!(expand_path_from("~/completion", home.map(PathBuf::from)).is_err());
        assert_eq!(
            expand_path_from("/completions/_example", home.map(PathBuf::from)).unwrap(),
            PathBuf::from("/completions/_example")
        );
    }
    assert_eq!(
        expand_path_from("~/completion", Some("/home/you".into())).unwrap(),
        PathBuf::from("/home/you/completion")
    );
    for path in ["", "completion", "./completion"] {
        assert!(expand_path_from(path, Some("/home/you".into())).is_err());
    }
}

#[test]
fn キャッシュの保存先は絶対パスだけを使う() {
    use std::path::PathBuf;
    let home = Some(PathBuf::from("/home/you"));
    for value in [None, Some("".into()), Some("relative".into())] {
        assert_eq!(
            cache_dir_from(value, home.clone()),
            Some(PathBuf::from(
                "/home/you/.cache/zsh-turbo/managed-completions"
            ))
        );
    }
    assert_eq!(
        cache_dir_from(Some("/cache".into()), None),
        Some(PathBuf::from("/cache/zsh-turbo/managed-completions"))
    );
    assert_eq!(cache_dir_from(Some("relative".into()), None), None);
    assert_eq!(cache_dir_from(None, Some(PathBuf::from("relative"))), None);
}

#[test]
fn 補完元と監視ファイルの相対パスを拒否する() {
    let mut entry = Registration {
        command: "example".into(),
        source: Source::File,
        ..Default::default()
    };
    for path in [
        "",
        "_example",
        "./_example",
        "../_example",
        "~other/_example",
    ] {
        entry.file = path.into();
        assert!(validate(&[entry.clone()]).is_err(), "{path}");
    }
    for path in ["/completions/_example", "~/completions/_example"] {
        entry.file = path.into();
        assert!(validate(&[entry.clone()]).is_ok());
    }
    entry.watch_files = vec!["relative".into()];
    assert!(validate(&[entry.clone()]).is_err());
    let root = tempfile::tempdir().unwrap();
    assert!(cache::refresh(&entry, root.path(), true).is_err());
    entry.watch_files.clear();
    entry.source = Source::Generator;
    for program in ["", "./generator", "bin/generator", "../generator"] {
        entry.generator = vec![program.into()];
        assert!(validate(&[entry.clone()]).is_err());
    }
    for program in ["generator", "/bin/generator", "~/bin/generator"] {
        entry.generator = vec![program.into()];
        assert!(validate(&[entry.clone()]).is_ok());
    }
}

#[test]
fn registration_roundtrips_and_legacy_config_defaults() {
    let config: crate::config::Config = toml::from_str("").unwrap();
    assert_eq!(config.completions, defaults());
    assert_eq!(crate::config::Config::default().completions, defaults());
    let saved = toml::to_string(&config).unwrap();
    assert_eq!(
        toml::from_str::<crate::config::Config>(&saved)
            .unwrap()
            .completions,
        defaults()
    );
    let empty: crate::config::Config = toml::from_str("completions = []").unwrap();
    assert!(empty.completions.is_empty());
    let saved = toml::to_string(&empty).unwrap();
    assert!(
        toml::from_str::<crate::config::Config>(&saved)
            .unwrap()
            .completions
            .is_empty()
    );
    let custom: crate::config::Config =
        toml::from_str("[[completions]]\ncommand = 'example-cli'").unwrap();
    assert_eq!(custom.completions.len(), 1);
    assert_eq!(custom.completions[0].command, "example-cli");
    let entry = Registration {
        command: "example-cli".into(),
        watch_files: vec!["~/bin/real-cli".into()],
        ..Default::default()
    };
    let encoded = toml::to_string(&entry).unwrap();
    assert_eq!(toml::from_str::<Registration>(&encoded).unwrap(), entry);
    assert!(!valid_command("../command"));
    assert!(!valid_command("x;echo"));
    assert!(!valid_command("zsh-turbo"));
}

#[test]
fn native_registration_keeps_existing_completion_without_creating_cache() {
    let root = tempfile::tempdir().unwrap();
    let entries = defaults();
    let entry = &entries[0];
    assert_eq!(entry.command, "git");
    assert_eq!(entry.source, Source::Native);
    assert!(shell_init(&entries).is_empty());
    assert_eq!(cache::refresh(entry, root.path(), true).unwrap(), "native");
    assert_eq!(cache::status(entry, root.path()), "native");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    let mut mixed = entries;
    mixed.push(Registration {
        command: "example-cli".into(),
        ..Default::default()
    });
    let init = shell_init(&mixed);
    assert!(init.contains("'example-cli'"));
    assert!(!init.contains("'git'"));
}

#[test]
fn changed_source_is_rebuilt_and_failed_update_keeps_cache() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("_example");
    let root = dir.path().join("cache");
    let entry = Registration {
        command: "example".into(),
        source: Source::File,
        file: source.to_string_lossy().into(),
        ..Default::default()
    };
    fs::write(&source, "#compdef example\n_arguments '--before[Before]'\n").unwrap();
    assert_eq!(cache::refresh(&entry, &root, false).unwrap(), "updated");
    assert_eq!(cache::refresh(&entry, &root, false).unwrap(), "cached");
    let metadata = fs::read(root.join("example.json")).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&metadata).unwrap();
    assert_eq!(json["hashes"][0].as_str().unwrap().len(), 64);
    fs::write(&source, "#compdef example\n_arguments '--after[After]'\n").unwrap();
    assert_eq!(cache::refresh(&entry, &root, false).unwrap(), "updated");
    let good = fs::read(root.join("example.zsh")).unwrap();
    assert!(String::from_utf8_lossy(&good).contains("--after"));
    fs::write(&source, "invalid completion").unwrap();
    assert!(cache::refresh(&entry, &root, false).is_err());
    assert_eq!(fs::read(root.join("example.zsh")).unwrap(), good);
    assert!(cache::status(&entry, &root).contains("#compdef"));
}

#[cfg(unix)]
#[test]
fn symlink_retarget_and_generator_watch_changes_invalidate_cache() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    let link = dir.path().join("selected");
    for (path, flag) in [(&first, "first"), (&second, "second")] {
        fs::write(path, format!("printf '%s\\n' '#compdef sh' '_sample() {{ _arguments --{flag}; }}' 'compdef _sample sh'\n")).unwrap();
    }
    symlink(&first, &link).unwrap();
    let entry = Registration {
        command: "sh".into(),
        source: Source::Generator,
        generator: vec!["/bin/sh".into(), link.to_string_lossy().into()],
        watch_files: vec![link.to_string_lossy().into()],
        ..Default::default()
    };
    let root = dir.path().join("cache");
    assert_eq!(cache::refresh(&entry, &root, false).unwrap(), "updated");
    fs::remove_file(&link).unwrap();
    symlink(&second, &link).unwrap();
    assert_eq!(cache::refresh(&entry, &root, false).unwrap(), "updated");
    assert!(
        fs::read_to_string(root.join("sh.zsh"))
            .unwrap()
            .contains("--second")
    );
    assert_eq!(cache::refresh(&entry, &root, true).unwrap(), "updated");
}

#[test]
fn concurrent_updates_use_one_writer_and_valid_atomic_files() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("_example");
    let root = dir.path().join("cache");
    fs::write(&source, "#compdef example\n_arguments '--test[Test]'\n").unwrap();
    let entry = Registration {
        command: "example".into(),
        source: Source::File,
        file: source.to_string_lossy().into(),
        ..Default::default()
    };
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| cache::refresh(&entry, &root, true)))
            .collect();
        for worker in workers {
            assert!(worker.join().unwrap().is_ok());
        }
    });
    let _: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("example.json")).unwrap()).unwrap();
    assert!(
        fs::read_to_string(root.join("example.zsh"))
            .unwrap()
            .contains("--test")
    );
}
