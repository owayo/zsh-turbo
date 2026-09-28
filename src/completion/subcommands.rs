//! 入力欄の下の一覧に出すサブコマンドとオプション。
//! `--help` を解析した結果をコマンドごとにキャッシュし、キー入力のたびに実行しない。

use super::{cache, help};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const VERSION: u32 = 1;
/// mise のシムのようにバイナリが同じでも中身が更新されることがあるため、一定時間で取り直す
const TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// 失敗・タイムアウトは長く覚えず、少し待って取り直す
const FAILURE_TTL: Duration = Duration::from_secs(5 * 60);
const HELP_TIMEOUT: Duration = Duration::from_secs(3);

/// `--help` から読み取ったトップレベルのサブコマンドとオプション (名前, 説明)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopLevel {
    pub commands: Vec<(String, String)>,
    pub options: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    stamp: cache::Stamp,
    updated: u64,
    failed: bool,
    help: TopLevel,
}

/// PATH 上の `command` のサブコマンドとオプションを返す。見つからなければ None。
pub fn top_level(command: &str) -> Option<TopLevel> {
    if !super::valid_command(command) {
        return None;
    }
    let binary = cache::resolve(command).ok()?;
    // 入力しただけで実行するため、PATH の `.` や空要素 (カレントディレクトリ) で見つかった
    // 実行ファイルは使わない。信頼できないリポジトリに置かれた `./uv` などを起動しない
    if !binary.is_absolute() {
        return None;
    }
    load(
        command,
        &binary,
        &cache_root(),
        SystemTime::now(),
        &|binary| {
            // npm のように使い方を表示して 1 で終わるものもあるため、終了コードは問わない
            crate::prompt::helpers::run_cmd_any_status(binary.to_str()?, &["--help"], HELP_TIMEOUT)
                .filter(|text| !text.is_empty())
        },
    )
}

fn cache_root() -> PathBuf {
    super::cache_dir().with_file_name("subcommands")
}

fn seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn load(
    command: &str,
    binary: &Path,
    root: &Path,
    now: SystemTime,
    run_help: &dyn Fn(&Path) -> Option<String>,
) -> Option<TopLevel> {
    let stamp = cache::stamp(binary).ok()?;
    let path = root.join(format!("{command}.json"));
    let fresh = || {
        let record = serde_json::from_slice::<Record>(&fs::read(&path).ok()?).ok()?;
        let ttl = if record.failed { FAILURE_TTL } else { TTL };
        (record.version == VERSION
            && record.stamp == stamp
            && seconds(now).saturating_sub(record.updated) < ttl.as_secs())
        .then_some(record.help)
    };
    if let Some(help) = fresh() {
        return Some(help);
    }
    // キー入力ごとに起動される複数のプロセスが一斉に `--help` を実行しないよう、
    // 取り直しは 1 つに絞り、待っていた側はその結果を読み直して使う
    fs::create_dir_all(root).ok()?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(format!("{command}.lock")))
        .ok()?;
    lock.lock().ok()?;
    if let Some(help) = fresh() {
        return Some(help);
    }
    let help = run_help(binary)
        .map(|text| help::parse(&text).into_top_level())
        .unwrap_or_default();
    // エラー文だけが返った場合 (mise の未信頼の設定など) も、短い期限で取り直す
    let failed = help.commands.is_empty() && help.options.is_empty();
    let record = Record {
        version: VERSION,
        stamp,
        updated: seconds(now),
        failed,
        help,
    };
    if let Ok(json) = serde_json::to_vec(&record) {
        let _ = cache::atomic_write(&path, &json);
    }
    Some(record.help)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const HELP: &str = "Usage: demo <COMMAND>\n\nCommands:\n  sync  Update the environment\n  run   Run a command\n\nOptions:\n  -q, --quiet  Use quiet output\n";

    #[test]
    fn helpを解析してキャッシュし期限切れとバイナリの変更で取り直す() {
        let tmp = tempfile::tempdir().unwrap();
        let binary = tmp.path().join("demo");
        fs::write(&binary, "v1").unwrap();
        let root = tmp.path().join("cache");
        let runs = Cell::new(0);
        let run = |_: &Path| {
            runs.set(runs.get() + 1);
            Some(HELP.to_owned())
        };
        let now = SystemTime::now();

        let help = load("demo", &binary, &root, now, &run).unwrap();
        assert_eq!(
            help.commands,
            [
                ("sync".to_owned(), "Update the environment".to_owned()),
                ("run".to_owned(), "Run a command".to_owned())
            ]
        );
        assert_eq!(
            help.options,
            [
                ("-q".to_owned(), "Use quiet output".to_owned()),
                ("--quiet".to_owned(), "Use quiet output".to_owned())
            ]
        );
        // キャッシュが新しい間は実行しない
        assert_eq!(load("demo", &binary, &root, now, &run), Some(help.clone()));
        assert_eq!(runs.get(), 1);
        // 期限を過ぎたら取り直す
        load("demo", &binary, &root, now + TTL, &run).unwrap();
        assert_eq!(runs.get(), 2);
        // バイナリが変わったら取り直す
        fs::write(&binary, "version 2").unwrap();
        load("demo", &binary, &root, now + TTL, &run).unwrap();
        assert_eq!(runs.get(), 3);
    }

    #[test]
    fn 失敗は短い期限で取り直す() {
        let tmp = tempfile::tempdir().unwrap();
        let binary = tmp.path().join("demo");
        fs::write(&binary, "v1").unwrap();
        let root = tmp.path().join("cache");
        let now = SystemTime::now();
        let fails = Cell::new(0);
        let fail = |_: &Path| {
            fails.set(fails.get() + 1);
            None
        };
        assert_eq!(
            load("demo", &binary, &root, now, &fail),
            Some(TopLevel::default())
        );
        load("demo", &binary, &root, now + Duration::from_secs(60), &fail).unwrap();
        assert_eq!(fails.get(), 1);
        load("demo", &binary, &root, now + FAILURE_TTL, &fail).unwrap();
        assert_eq!(fails.get(), 2);
    }

    #[test]
    fn 他のプロセスが取り直している間は待ってその結果を使う() {
        let tmp = tempfile::tempdir().unwrap();
        let binary = tmp.path().join("demo");
        fs::write(&binary, "v1").unwrap();
        let root = tmp.path().join("cache");
        fs::create_dir_all(&root).unwrap();
        let now = SystemTime::now();
        let held = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join("demo.lock"))
            .unwrap();
        held.lock().unwrap();
        let waiter = {
            let (binary, root) = (binary.clone(), root.clone());
            std::thread::spawn(move || {
                let never = |_: &Path| -> Option<String> {
                    panic!("取り直した結果があれば --help を実行しない")
                };
                load("demo", &binary, &root, now, &never)
            })
        };
        std::thread::sleep(Duration::from_millis(50));
        // ロックを持つ側が取り直した結果を書いてから解放する
        let help = TopLevel {
            commands: vec![("sync".into(), "Update".into())],
            options: Vec::new(),
        };
        let record = Record {
            version: VERSION,
            stamp: cache::stamp(&binary).unwrap(),
            updated: seconds(now),
            failed: false,
            help: help.clone(),
        };
        cache::atomic_write(
            &root.join("demo.json"),
            &serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        drop(held);
        assert_eq!(waiter.join().unwrap(), Some(help));
    }

    #[test]
    fn 実行ファイルが無いコマンドは対象外() {
        assert_eq!(top_level("zsh-turbo-no-such-command-xyz"), None);
        assert_eq!(top_level("bad name"), None);
    }
}
