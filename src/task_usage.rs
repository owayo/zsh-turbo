//! ディレクトリごとのタスクの利用記録。
//! `make install` のようにタスクを実行した度合いを (ディレクトリ, コマンド, タスク名) ごとに
//! 減衰付きの回数で持ち、一覧で最初に選ぶ項目を決めるのに使う。実行行の全文や引数は保存しない。

use crate::{
    project_tasks,
    state::{create_private_dir, private_options, write_atomic},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const VERSION: u32 = 1;
/// 利用回数の重みが半分になるまでの秒数 (30 日)
const HALF_LIFE: f64 = 30.0 * 24.0 * 60.0 * 60.0;
/// 保存する組の上限。超えたら重みの小さいものから捨てる
const MAX_ENTRIES: usize = 1000;
/// 重みがこれより小さくなった組は捨てる (1 回だけの利用なら約 200 日後)
const MIN_SCORE: f64 = 0.01;
/// 履歴全体での呼び出しの傾向を、このディレクトリでの利用何回分として扱うか
const HISTORY_WEIGHT: f64 = 3.0;
/// `record` が受け取る実行行の上限。超える行はタスクの呼び出しとして扱わない
pub const MAX_LINE_BYTES: u64 = 4096;

#[derive(Serialize, Deserialize)]
struct Store {
    version: u32,
    entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    dir: String,
    command: String,
    task: String,
    /// `used` の時点まで減衰させた利用回数
    score: f64,
    /// 最後に使った時刻 (UNIX 秒)
    used: u64,
}

/// このディレクトリでのタスクの利用
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Usage {
    /// 現在時刻まで減衰させた利用回数
    pub score: f64,
    /// 最後に使った時刻 (UNIX 秒)
    pub used: u64,
}

/// 履歴全体でのタスクの呼び出し
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryUse {
    pub count: usize,
    /// 最後の呼び出しが新しい方から何番目の履歴か (0 が最新)
    pub newest: usize,
}

/// 保存先。`XDG_STATE_HOME` が未設定・空・相対パスなら `~/.local/state` 配下。
/// 記録はコマンドを実行したディレクトリで動くため、相対パスを採るとプロジェクト内に書いてしまう
pub fn store_path() -> Option<PathBuf> {
    store_path_from(std::env::var_os("XDG_STATE_HOME"), dirs::home_dir())
}

fn store_path_from(state_home: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    let base = state_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home.map(|home| home.join(".local/state")))?;
    Some(base.join("zsh-turbo").join("task-usage.json"))
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 時刻 `used` の重み `score` を `now` まで減衰させる
fn decayed(score: f64, used: u64, now: u64) -> f64 {
    score * (-(now.saturating_sub(used) as f64) / HALF_LIFE).exp2()
}

fn load(path: &Path) -> Option<Store> {
    let store: Store = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (store.version == VERSION).then_some(store)
}

/// `cwd` で `command` のタスクを使った度合いをタスク名ごとに返す。読めなければ空
pub fn usage(path: &Path, cwd: &Path, command: &str, now: u64) -> HashMap<String, Usage> {
    let (Some(dir), Some(store)) = (cwd.to_str(), load(path)) else {
        return HashMap::new();
    };
    store
        .entries
        .into_iter()
        .filter(|entry| entry.dir == dir && entry.command == command)
        .map(|entry| {
            let usage = Usage {
                score: decayed(entry.score, entry.used, now),
                used: entry.used,
            };
            (entry.task, usage)
        })
        .collect()
}

/// 実行した行が `cwd` に定義されたタスクの呼び出しなら、利用を 1 回分加えて true を返す。
/// 先頭が空白の行は、履歴に残さない慣習に合わせて記録しない。
/// 読めない・別の版の記録ファイルは上書きせず、記録をやめる。
pub fn record(line: &str, cwd: &Path, now: u64, path: &Path) -> io::Result<bool> {
    if line.starts_with(char::is_whitespace) {
        return Ok(false);
    }
    let Some((command, task)) = project_tasks::invocation(line) else {
        return Ok(false);
    };
    let (Some(dir), Some(parent)) = (cwd.to_str(), path.parent()) else {
        return Ok(false);
    };
    if !project_tasks::defines(command, task, cwd) {
        return Ok(false);
    }
    create_private_dir(parent)?;
    // 複数のシェルから同時に記録しても加算が失われないよう、読み取りから置換までを排他にする。
    // ロックファイルは置換も削除もしない
    let lock = private_options()
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))?;
    lock.lock()?;
    let mut store = match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Store>(&bytes) {
            Ok(store) if store.version == VERSION => store,
            _ => return Ok(false),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Store {
            version: VERSION,
            entries: Vec::new(),
        },
        Err(error) => return Err(error),
    };
    add(&mut store.entries, dir, command, task, now);
    prune(&mut store.entries, now, |entry| {
        entry.dir == dir && entry.command == command && entry.task == task
    });
    let json = serde_json::to_vec(&store).map_err(io::Error::other)?;
    write_atomic(path, &json)?;
    Ok(true)
}

/// 時刻 `at` の利用を 1 回分加える。記録の処理順が前後しても同じ重みになるよう、
/// 新しい方の時刻へそろえてから足す
fn add(entries: &mut Vec<Entry>, dir: &str, command: &str, task: &str, at: u64) {
    let found = entries
        .iter_mut()
        .find(|entry| entry.dir == dir && entry.command == command && entry.task == task);
    match found {
        Some(entry) => {
            let used = entry.used.max(at);
            entry.score = decayed(entry.score, entry.used, used) + decayed(1.0, at, used);
            entry.used = used;
        }
        None => entries.push(Entry {
            dir: dir.to_owned(),
            command: command.to_owned(),
            task: task.to_owned(),
            score: 1.0,
            used: at,
        }),
    }
}

/// 重みが小さくなった組を捨て、上限を超えたら重みの小さいものから捨てる。
/// 今回加えた組 (`current`) は残す。捨てると、上限まで埋まった後は新しいタスクを
/// 何度使っても 1 回目として捨てられ続け、学習できなくなる
fn prune(entries: &mut Vec<Entry>, now: u64, current: impl Fn(&Entry) -> bool) {
    let weight = |entry: &Entry| decayed(entry.score, entry.used, now);
    entries.retain(|entry| current(entry) || weight(entry) >= MIN_SCORE);
    if entries.len() > MAX_ENTRIES {
        entries.sort_by(|a, b| {
            current(b)
                .cmp(&current(a))
                .then_with(|| weight(b).total_cmp(&weight(a)))
        });
        entries.truncate(MAX_ENTRIES);
    }
}

/// 一覧のタスク名から、最初に選ぶ位置を返す。
/// このディレクトリでの利用回数 (減衰付き) に、履歴全体での呼び出しの割合を
/// 3 回分の重みで足した値が最大のものを選ぶ。導入前の履歴はディレクトリが分からないため、
/// ここでの利用が数回たまるまでは履歴全体の傾向が勝つ。どちらも無ければ None。
/// 同じ値なら、ここで最近使ったもの、履歴で新しいもの、一覧の前のものの順に選ぶ。
pub fn preferred<'a>(
    names: impl IntoIterator<Item = &'a str>,
    local: &HashMap<String, Usage>,
    history: &HashMap<String, HistoryUse>,
) -> Option<usize> {
    let names: Vec<&str> = names.into_iter().collect();
    let total: usize = names
        .iter()
        .filter_map(|name| history.get(*name))
        .map(|called| called.count)
        .sum();
    names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            let here = local.get(*name);
            let called = history.get(*name);
            let prior = called.filter(|_| total > 0).map_or(0.0, |called| {
                HISTORY_WEIGHT * called.count as f64 / total as f64
            });
            let score = here.map_or(0.0, |usage| usage.score) + prior;
            let used = here.map_or(0, |usage| usage.used);
            let newest = called.map_or(usize::MAX, |called| called.newest);
            (score > 0.0).then_some((index, score, used, newest))
        })
        .max_by(|a, b| {
            a.1.total_cmp(&b.1)
                .then(a.2.cmp(&b.2))
                .then(b.3.cmp(&a.3))
                .then(b.0.cmp(&a.0))
        })
        .map(|(index, ..)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 60 * 60;
    const NOW: u64 = 1_800_000_000;

    fn make_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Makefile"), "build:\ninstall:\ntest:\n").unwrap();
        dir
    }

    fn local(entries: &[(&str, f64, u64)]) -> HashMap<String, Usage> {
        entries
            .iter()
            .map(|&(name, score, used)| (name.to_owned(), Usage { score, used }))
            .collect()
    }

    fn history(entries: &[(&str, usize, usize)]) -> HashMap<String, HistoryUse> {
        entries
            .iter()
            .map(|&(name, count, newest)| (name.to_owned(), HistoryUse { count, newest }))
            .collect()
    }

    #[test]
    fn 定義されたタスクの呼び出しだけをディレクトリごとに数える() {
        let project = make_project();
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("zsh-turbo/task-usage.json");
        assert!(record("make install", project.path(), NOW, &path).unwrap());
        assert!(record("make install PREFIX=~/.local", project.path(), NOW, &path).unwrap());
        assert!(record("make test", project.path(), NOW, &path).unwrap());
        // 先頭が空白の行、定義にないタスク、呼び出しと判定できない行は記録しない
        for line in [
            " make build",
            "make unknown",
            "make build && rm x",
            "echo make",
        ] {
            assert!(!record(line, project.path(), NOW, &path).unwrap(), "{line}");
        }
        let used = usage(&path, project.path(), "make", NOW);
        assert_eq!(used.len(), 2);
        assert_eq!(
            used["install"],
            Usage {
                score: 2.0,
                used: NOW
            }
        );
        assert_eq!(
            used["test"],
            Usage {
                score: 1.0,
                used: NOW
            }
        );
        // 別のディレクトリ・別のコマンドの記録は混ぜない
        let other = make_project();
        assert!(usage(&path, other.path(), "make", NOW).is_empty());
        assert!(usage(&path, project.path(), "just", NOW).is_empty());
        // 引数や実行行の全文は保存しない
        let saved = fs::read_to_string(&path).unwrap();
        assert!(!saved.contains("PREFIX"), "{saved}");
    }

    #[test]
    fn 利用回数は30日で半分に減衰し記録の順序が前後しても同じ重みになる() {
        let project = make_project();
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("task-usage.json");
        record("make install", project.path(), NOW, &path).unwrap();
        let later = usage(&path, project.path(), "make", NOW + 30 * DAY);
        assert!((later["install"].score - 0.5).abs() < 1e-9);

        let mut forward = Vec::new();
        add(&mut forward, "/p", "make", "test", NOW);
        add(&mut forward, "/p", "make", "test", NOW + DAY);
        let mut reversed = Vec::new();
        add(&mut reversed, "/p", "make", "test", NOW + DAY);
        add(&mut reversed, "/p", "make", "test", NOW);
        assert_eq!(forward[0].used, reversed[0].used);
        assert!((forward[0].score - reversed[0].score).abs() < 1e-9);
    }

    #[test]
    fn 重みの小さい組と上限を超えた組を捨てる() {
        let mut entries = Vec::new();
        add(&mut entries, "/old", "make", "build", NOW - 400 * DAY);
        for index in 0..MAX_ENTRIES + 5 {
            add(
                &mut entries,
                &format!("/p{index}"),
                "make",
                "build",
                NOW - (index as u64) * 60,
            );
        }
        prune(&mut entries, NOW, |_| false);
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert!(entries.iter().all(|entry| entry.dir != "/old"));
        // 新しいものを残す
        assert!(entries.iter().any(|entry| entry.dir == "/p0"));
        assert!(
            !entries
                .iter()
                .any(|entry| entry.dir == format!("/p{}", MAX_ENTRIES + 4))
        );
    }

    #[test]
    fn 上限まで埋まっていても今回使ったタスクは残り回数を重ねられる() {
        let project = make_project();
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("task-usage.json");
        // 上限まで、新しいタスクの 1 回より重い組で埋める
        let entries = (0..MAX_ENTRIES)
            .map(|index| Entry {
                dir: format!("/p{index}"),
                command: "make".into(),
                task: "build".into(),
                score: 2.0,
                used: NOW,
            })
            .collect();
        let store = Store {
            version: VERSION,
            entries,
        };
        fs::write(&path, serde_json::to_vec(&store).unwrap()).unwrap();
        for _ in 0..3 {
            assert!(record("make test", project.path(), NOW, &path).unwrap());
        }
        assert_eq!(usage(&path, project.path(), "make", NOW)["test"].score, 3.0);
        assert_eq!(load(&path).unwrap().entries.len(), MAX_ENTRIES);
    }

    #[test]
    fn 読めない記録ファイルは上書きしない() {
        let project = make_project();
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("task-usage.json");
        for broken in ["{not json", r#"{"version":999,"entries":[]}"#] {
            fs::write(&path, broken).unwrap();
            assert!(!record("make install", project.path(), NOW, &path).unwrap());
            assert_eq!(fs::read_to_string(&path).unwrap(), broken);
            assert!(usage(&path, project.path(), "make", NOW).is_empty());
        }
    }

    #[cfg(unix)]
    #[test]
    fn 記録は本人だけが読めるファイルに置く() {
        use std::os::unix::fs::PermissionsExt;
        let project = make_project();
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("new/zsh-turbo/task-usage.json");
        record("make install", project.path(), NOW, &path).unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        // 一時ファイルを残さない
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    #[test]
    fn 並行して記録しても加算を失わない() {
        let project = make_project();
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("task-usage.json");
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..5 {
                        record("make test", project.path(), NOW, &path).unwrap();
                    }
                });
            }
        });
        assert_eq!(
            usage(&path, project.path(), "make", NOW)["test"].score,
            40.0
        );
    }

    #[test]
    fn 記録が少ない間は履歴全体の傾向を優先しここでの利用が重なると上書きする() {
        let names = ["build", "install", "test"];
        // 記録も履歴もなければ選ばない (一覧の先頭のまま)
        assert_eq!(preferred(names, &HashMap::new(), &HashMap::new()), None);
        // 履歴全体では install が多い
        let calls = history(&[("install", 133, 5), ("test", 2, 0)]);
        assert_eq!(preferred(names, &HashMap::new(), &calls), Some(1));
        // ここで test を 1〜2 回使っただけでは履歴全体の傾向が勝つ
        for count in [1.0, 2.0] {
            let here = local(&[("test", count, NOW)]);
            assert_eq!(preferred(names, &here, &calls), Some(1), "{count}");
        }
        // 3 回使うと上書きする
        let here = local(&[("test", 3.0, NOW)]);
        assert_eq!(preferred(names, &here, &calls), Some(2));
        // 一覧にないタスクの履歴は割合に含めない
        let calls = history(&[("install", 1, 0), ("clean-lfs", 100, 1)]);
        assert_eq!(preferred(names, &HashMap::new(), &calls), Some(1));
    }

    #[test]
    fn 同じ重みならここで最近使ったもの履歴で新しいもの一覧の前のものを選ぶ() {
        let names = ["build", "install", "test"];
        let here = local(&[("build", 1.0, NOW - DAY), ("test", 1.0, NOW)]);
        assert_eq!(preferred(names, &here, &HashMap::new()), Some(2));
        let calls = history(&[("build", 1, 3), ("test", 1, 1)]);
        assert_eq!(preferred(names, &HashMap::new(), &calls), Some(2));
        let here = local(&[("build", 1.0, NOW), ("test", 1.0, NOW)]);
        assert_eq!(preferred(names, &here, &HashMap::new()), Some(0));
    }

    #[test]
    fn 保存先は空や相対パスのxdg_state_homeを無視して既定の場所を使う() {
        let home = Some(PathBuf::from("/home/me"));
        assert_eq!(
            store_path_from(Some("/state".into()), home.clone()),
            Some(PathBuf::from("/state/zsh-turbo/task-usage.json"))
        );
        for state_home in [None, Some(OsString::new()), Some("state".into())] {
            assert_eq!(
                store_path_from(state_home, home.clone()),
                Some(PathBuf::from(
                    "/home/me/.local/state/zsh-turbo/task-usage.json"
                ))
            );
        }
        // ホームも分からなければ記録しない (カレントディレクトリへ書かない)
        assert_eq!(store_path_from(None, None), None);
    }
}
