//! 本人だけが読める状態ファイルの保存。

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
}

pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

/// 同じディレクトリの一意な一時ファイルへ書いてから置き換える
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let tmp = path.with_extension(format!("tmp-{}-{nanos}", std::process::id()));
    let mut file = private_options().create_new(true).open(&tmp)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 状態ファイルを置き換えて一時ファイルを残さない() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        write_atomic(&path, b"old data").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn 置換に失敗したら既存の内容を保ち一時ファイルを除去する() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), b"existing").unwrap();
        assert!(write_atomic(&path, b"replacement").is_err());
        assert_eq!(fs::read(path.join("keep")).unwrap(), b"existing");
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
    }
}
