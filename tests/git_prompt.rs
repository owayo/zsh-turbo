#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

fn git(repo: &Path, args: &[&str]) -> std::process::Output {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Example",
            "-c",
            "user.email=you@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    output
}

fn prepare(root: &Path) -> std::path::PathBuf {
    let repo = root.join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    fs::write(repo.join("tracked"), "content\n").unwrap();
    git(&repo, &["add", "tracked"]);
    git(&repo, &["commit", "-qm", "initial"]);
    git(&repo, &["checkout", "-qb", "fixture-branch"]);
    fs::create_dir_all(root.join("config/zsh-turbo")).unwrap();
    fs::write(
        root.join("config/zsh-turbo/config.toml"),
        "[prompt]\nleft_segments=['git']\nright_segments=[]\nfont_level='ascii'\nnewline=false\n",
    )
    .unwrap();
    repo
}

fn prompt(root: &Path, repo: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"))
        .arg("prompt")
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn プロンプト表示でfsmonitorや属性のフィルターを実行しない() {
    for attributes in [".gitattributes", ".git/info/attributes"] {
        for kind in ["clean", "process"] {
            let tmp = tempfile::tempdir().unwrap();
            let repo = prepare(tmp.path());
            fs::write(repo.join(attributes), "tracked filter=MiXeD.Name-1\n").unwrap();
            git(
                &repo,
                &["config", "core.fsmonitor", "touch monitor-ran; false"],
            );
            git(
                &repo,
                &[
                    "config",
                    &format!("filter.MiXeD.Name-1.{kind}"),
                    "touch filter-ran; cat",
                ],
            );
            git(&repo, &["config", "filter.MiXeD.Name-1.required", "true"]);
            fs::File::options()
                .write(true)
                .open(repo.join("tracked"))
                .unwrap()
                .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000))
                .unwrap();
            let index = fs::read(repo.join(".git/index")).unwrap();
            let output = prompt(tmp.path(), &repo);
            assert!(
                !repo.join("monitor-ran").exists(),
                "{attributes}: fsmonitor"
            );
            assert!(!repo.join("filter-ran").exists(), "{attributes}: {kind}");
            assert!(output.contains("fixture-branch"), "{output:?}");
            assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
        }
    }
}

#[test]
fn stashの件数取得で署名検証プログラムを実行しない() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = prepare(tmp.path());
    let head = String::from_utf8(git(&repo, &["rev-parse", "HEAD"]).stdout).unwrap();
    let tree = String::from_utf8(git(&repo, &["rev-parse", "HEAD^{tree}"]).stdout).unwrap();
    let commit = format!(
        "tree {}\nparent {}\nauthor Example <you@example.com> 1000000000 +0000\ncommitter Example <you@example.com> 1000000000 +0000\ngpgsig -----BEGIN PGP SIGNATURE-----\n \n fixture\n -----END PGP SIGNATURE-----\n\nstash fixture\n",
        tree.trim(),
        head.trim()
    );
    let mut child = Command::new("git")
        .args(["hash-object", "-t", "commit", "-w", "--stdin"])
        .current_dir(&repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(commit.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let oid = String::from_utf8(output.stdout).unwrap();
    git(
        &repo,
        &["update-ref", "--create-reflog", "refs/stash", oid.trim()],
    );
    let verifier = repo.join("verifier");
    fs::write(&verifier, "#!/bin/sh\ntouch verifier-ran\nexit 1\n").unwrap();
    fs::set_permissions(&verifier, fs::Permissions::from_mode(0o755)).unwrap();
    git(&repo, &["config", "log.showSignature", "true"]);
    git(
        &repo,
        &["config", "gpg.program", verifier.to_str().unwrap()],
    );
    let output = prompt(tmp.path(), &repo);
    assert!(!repo.join("verifier-ran").exists());
    assert!(output.contains("*1"), "{output:?}");
}

#[test]
fn 部分クローンで欠けたオブジェクトを取りに行かない() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = prepare(tmp.path());
    let tree = String::from_utf8(git(&repo, &["rev-parse", "HEAD^{tree}"]).stdout).unwrap();
    let tree = tree.trim();
    let fetcher = repo.join("fetcher");
    fs::write(&fetcher, "#!/bin/sh\ntouch fetch-ran\nexit 1\n").unwrap();
    fs::set_permissions(&fetcher, fs::Permissions::from_mode(0o755)).unwrap();
    git(&repo, &["config", "extensions.partialClone", "origin"]);
    git(&repo, &["config", "remote.origin.promisor", "true"]);
    git(
        &repo,
        &["config", "remote.origin.url", "ssh://example.invalid/repo"],
    );
    git(
        &repo,
        &["config", "core.sshCommand", fetcher.to_str().unwrap()],
    );
    fs::remove_file(repo.join(".git/objects").join(&tree[..2]).join(&tree[2..])).unwrap();
    let _ = prompt(tmp.path(), &repo);
    assert!(!repo.join("fetch-ran").exists());
    // このフィクスチャで通常の status が実際に取得を試みることも確かめる。
    let _ = Command::new("git")
        .args(["status", "--porcelain", "-b"])
        .current_dir(&repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(repo.join("fetch-ran").exists());
}

#[test]
fn 上書きできないフィルター名ではgitセグメントを表示しない() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = prepare(tmp.path());
    git(
        &repo,
        &["config", "filter.a=b.clean", "touch filter-ran; cat"],
    );
    fs::write(repo.join(".git/info/attributes"), "tracked filter=a=b\n").unwrap();
    fs::File::options()
        .write(true)
        .open(repo.join("tracked"))
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000))
        .unwrap();
    let output = prompt(tmp.path(), &repo);
    assert!(!repo.join("filter-ran").exists());
    assert!(!output.contains("fixture-branch"), "{output:?}");
}
