//! 自動実行する外部コマンドを、カレントディレクトリに依存しないパスへ解決する。

use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn valid_program(program: &str) -> bool {
    Path::new(program).is_absolute()
        || program.starts_with("~/")
        || (!program.is_empty()
            && Path::new(program)
                .parent()
                .is_some_and(|parent| parent.as_os_str().is_empty()))
}

pub(crate) fn resolve(command: &str) -> io::Result<PathBuf> {
    resolve_in(command, &std::env::var_os("PATH").unwrap_or_default())
}

fn resolve_in(command: &str, path_env: &std::ffi::OsStr) -> io::Result<PathBuf> {
    if !valid_program(command) {
        return Err(io::Error::other(
            "executable must use a PATH command, an absolute path, or ~/",
        ));
    }
    let path = if let Some(rest) = command.strip_prefix("~/") {
        dirs::home_dir()
            .filter(|home| home.is_absolute())
            .ok_or_else(|| io::Error::other("no absolute home directory"))?
            .join(rest)
    } else {
        PathBuf::from(command)
    };
    if path.is_absolute() {
        // symlink の名前で動作を切り替えるシムもあるため、実行パスは canonicalize しない。
        return Ok(path);
    }
    std::env::split_paths(path_env)
        .map(|p| p.join(command))
        .find(|p| is_executable_file(p))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "executable not found in PATH"))
        .and_then(|path| {
            if path.is_absolute() {
                Ok(path)
            } else {
                Err(io::Error::other(
                    "refusing executable found through a relative PATH entry",
                ))
            }
        })
}

/// このユーザーが実行できる通常ファイルか。zsh のコマンド探索と同じく、symlink を辿った先が
/// 通常ファイルで `access(X_OK)` が通るものとする (壊れた symlink とディレクトリは含めない)。
/// 実行ビットだけを見ると、他のユーザー向けのビットしか無いファイルも実行できると誤る。
pub(crate) fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file()) && can_execute(path)
}

#[cfg(unix)]
fn can_execute(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    const X_OK: std::ffi::c_int = 1;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // 安全性: C 文字列ポインタは呼び出しの間有効で、`access(2)` は副作用がない。
    unsafe { access(c_path.as_ptr(), X_OK) == 0 }
}

#[cfg(not(unix))]
fn can_execute(_path: &Path) -> bool {
    true
}

#[cfg(unix)]
unsafe extern "C" {
    fn access(pathname: *const std::ffi::c_char, mode: std::ffi::c_int) -> std::ffi::c_int;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn 実行ファイルの相対パスを拒否してシムのパスは維持する() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("actual");
        let shim = tmp.path().join("shim");
        std::fs::write(&target, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        symlink(&target, &shim).unwrap();
        let path_env = std::env::join_paths([tmp.path()]).unwrap();
        assert_eq!(resolve_in("shim", &path_env).unwrap(), shim);
        assert!(resolve_in("./shim", &path_env).is_err());
    }

    #[test]
    fn 実行できる名前はコマンド名と絶対パスとホーム起点だけ() {
        for program in ["tool", "/usr/bin/tool", "~/bin/tool"] {
            assert!(valid_program(program), "{program}");
        }
        for program in ["", "bin/tool", "./tool", "../tool", "~user/tool"] {
            assert!(!valid_program(program), "{program}");
        }
    }

    #[test]
    fn ホーム起点のパスはpathを探さずホームディレクトリに解決する() {
        let home = dirs::home_dir().filter(|home| home.is_absolute()).unwrap();
        assert_eq!(
            resolve_in("~/bin/tool", std::ffi::OsStr::new("")).unwrap(),
            home.join("bin/tool")
        );
    }

    #[cfg(unix)]
    #[test]
    fn 実行権限のないファイルとディレクトリを飛ばして次のpathを使う() {
        use std::os::unix::fs::PermissionsExt;
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let third = tempfile::tempdir().unwrap();
        std::fs::write(first.path().join("tool"), b"#!/bin/sh\n").unwrap();
        std::fs::create_dir(second.path().join("tool")).unwrap();
        let actual = third.path().join("tool");
        std::fs::write(&actual, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&actual, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path_env = std::env::join_paths([first.path(), second.path(), third.path()]).unwrap();
        assert_eq!(resolve_in("tool", &path_env).unwrap(), actual);
        let error = resolve_in("missing-tool", &path_env).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
