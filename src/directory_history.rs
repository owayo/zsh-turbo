//! 実行したディレクトリごとの履歴。実行場所が不明な共通履歴は混ぜない。

use crate::{state, task_usage};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 1000;
const MAX_BYTES: usize = 1024 * 1024;

pub fn path() -> Option<PathBuf> {
    path_in(&std::env::current_dir().ok()?, &task_usage::store_path()?)
}

fn path_in(cwd: &Path, store: &Path) -> Option<PathBuf> {
    let cwd = fs::canonicalize(cwd).ok()?;
    let key = Sha256::digest(cwd.as_os_str().as_encoded_bytes());
    let key: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
    Some(
        store
            .parent()?
            .join("directory-history")
            .join(format!("{key}.history")),
    )
}

pub fn record(line: &str, cwd: &Path, store: &Path) -> io::Result<bool> {
    if line.is_empty()
        || line.starts_with(char::is_whitespace)
        || line.len() as u64 > task_usage::MAX_LINE_BYTES
        || line.chars().any(char::is_control)
    {
        return Ok(false);
    }
    let Some(path) = path_in(cwd, store) else {
        return Ok(false);
    };
    state::create_private_dir(path.parent().unwrap())?;
    let lock = state::private_options()
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))?;
    lock.lock()?;
    let mut bytes = match fs::read(&path) {
        Ok(bytes) if bytes.len() <= MAX_BYTES && bytes.ends_with(b"\n") => bytes,
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    // 拡張履歴で包み、コマンド自身が履歴ヘッダーに似ていてもそのまま復元する。
    bytes.extend_from_slice(b": 0:0;");
    for byte in line.bytes() {
        if byte >= 0x80 {
            bytes.extend([0x83, byte ^ 0x20]);
        } else {
            bytes.push(byte);
        }
    }
    if line.ends_with('\\') {
        bytes.push(b' ');
    }
    bytes.push(b'\n');
    let mut start = 0;
    for (count, (index, _)) in bytes
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .rev()
        .enumerate()
    {
        if count >= MAX_ENTRIES || bytes.len() - index - 1 > MAX_BYTES {
            start = index + 1;
            break;
        }
    }
    // 最も古い行を含めるとバイト上限を越える場合も、行の途中からは保存しない。
    while bytes.len() - start > MAX_BYTES {
        start += bytes[start..].iter().position(|b| *b == b'\n').unwrap() + 1;
    }
    state::write_atomic(&path, &bytes[start..])?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 古い履歴を行単位で捨て保存件数とサイズを制限する() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("task-usage.json");
        for i in 0..MAX_ENTRIES + 5 {
            record(&format!("echo {i}"), tmp.path(), &store).unwrap();
        }
        let path = path_in(tmp.path(), &store).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), MAX_ENTRIES);
        assert!(text.starts_with(": 0:0;echo 5\n"));
        for _ in 0..300 {
            record(&format!("echo {}", "あ".repeat(1000)), tmp.path(), &store).unwrap();
        }
        let bytes = fs::read(path).unwrap();
        assert!(bytes.len() <= MAX_BYTES);
        assert!(bytes.starts_with(b": 0:0;") && bytes.ends_with(b"\n"));
    }

    #[test]
    fn 並行記録で履歴を失わず壊れた履歴を上書きしない() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("task-usage.json");
        std::thread::scope(|scope| {
            for i in 0..16 {
                let store = &store;
                let cwd = tmp.path();
                scope.spawn(move || record(&format!("echo {i}"), cwd, store).unwrap());
            }
        });
        let path = path_in(tmp.path(), &store).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes.split(|b| *b == b'\n').count(), 17);
        fs::write(&path, b"torn").unwrap();
        assert!(!record("echo next", tmp.path(), &store).unwrap());
        assert_eq!(fs::read(path).unwrap(), b"torn");
    }
}
