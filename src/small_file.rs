//! リポジトリなど信頼できない場所にある小さな設定ファイルを、上限付きで読む。

use std::io::Read;
use std::path::Path;

/// タスク定義や package.json として読む上限。
pub(crate) const PROJECT_FILE_LIMIT: u64 = 1024 * 1024;

/// 通常ファイルだけを `limit` バイトまで読み、UTF-8 の文字列として返す。
/// デバイスや FIFO は長さが 0 と報告されるため、長さの確認だけでは防げない
/// (`/dev/zero` へのリンクは読み続け、FIFO は書き手が来るまで返らない)。
pub(crate) fn read_to_string(path: &Path, limit: u64) -> Option<String> {
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(limit.saturating_add(1))
        .read_to_string(&mut text)
        .ok()?;
    // 確認の後に伸びたファイルも上限を超えたら読まなかったことにする
    (text.len() as u64 <= limit).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 上限以内の通常ファイルだけを読む() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Makefile");
        std::fs::write(&path, "build:\n").unwrap();
        assert_eq!(read_to_string(&path, 7).as_deref(), Some("build:\n"));
        assert_eq!(read_to_string(&path, 6), None);
        assert_eq!(read_to_string(&tmp.path().join("missing"), 7), None);
        // ディレクトリは読まない
        assert_eq!(read_to_string(tmp.path(), 7), None);
    }

    #[cfg(unix)]
    #[test]
    fn デバイスやfifoへのリンクは読まずにすぐ返る() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let device = tmp.path().join("package.json");
        symlink("/dev/zero", &device).unwrap();
        assert_eq!(read_to_string(&device, PROJECT_FILE_LIMIT), None);
        let fifo = tmp.path().join("Makefile");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(status.success());
        // 書き手のいない FIFO を開くと返らないため、開く前に弾く
        assert_eq!(read_to_string(&fifo, PROJECT_FILE_LIMIT), None);
    }
}
