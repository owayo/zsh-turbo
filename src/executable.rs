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
        .find(|p| {
            std::fs::metadata(p).is_ok_and(|m| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    m.is_file() && m.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    m.is_file()
                }
            })
        })
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
}
