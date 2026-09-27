use super::*;
use std::fs;

#[test]
fn registration_roundtrips_and_legacy_config_defaults() {
    let config: crate::config::Config = toml::from_str("").unwrap();
    assert!(config.completions.is_empty());
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
