use super::{Registration, Source, expand_path, help, valid_command};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// 生成する補完スクリプトの形を変えたら上げ、既存のキャッシュを作り直させる。
const VERSION: u32 = 3;
const FAILURE_BACKOFF: u64 = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Stamp {
    path: PathBuf,
    len: u64,
    modified: u128,
    changed: i128,
    inode: u64,
}

#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    registration: Registration,
    stamps: Vec<Stamp>,
    hashes: Vec<String>,
    updated: u64,
}

#[derive(Serialize, Deserialize)]
struct Failure {
    version: u32,
    registration: Registration,
    stamps: Vec<Stamp>,
    updated: u64,
    error: String,
}

pub(super) fn stamp(path: &Path) -> io::Result<Stamp> {
    let path = fs::canonicalize(path)?;
    let meta = fs::metadata(&path)?;
    if !meta.is_file() {
        return Err(io::Error::other("not a regular file"));
    }
    #[cfg(unix)]
    let (changed, inode) = {
        use std::os::unix::fs::MetadataExt;
        (
            meta.ctime() as i128 * 1_000_000_000 + meta.ctime_nsec() as i128,
            meta.ino(),
        )
    };
    #[cfg(not(unix))]
    let (changed, inode) = (0, 0);
    Ok(Stamp {
        path,
        len: meta.len(),
        modified: meta
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        changed,
        inode,
    })
}

pub(super) fn resolve(command: &str) -> io::Result<PathBuf> {
    crate::executable::resolve(command)
}

fn stamps(entry: &Registration) -> io::Result<Vec<Stamp>> {
    super::validate(std::slice::from_ref(entry)).map_err(io::Error::other)?;
    let mut paths = Vec::new();
    if entry.source != Source::File {
        paths.push(resolve(&entry.command)?);
    } else if let Ok(binary) = resolve(&entry.command) {
        paths.push(binary);
    }
    if entry.source == Source::Generator {
        let generator = entry
            .generator
            .first()
            .ok_or_else(|| io::Error::other("generator is empty"))?;
        paths.push(resolve(generator)?);
    }
    if entry.source == Source::File {
        paths.push(expand_path(&entry.file));
    }
    paths.extend(entry.watch_files.iter().map(|p| expand_path(p)));
    paths.into_iter().map(|p| stamp(&p)).collect()
}

fn digest(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let tmp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    atomic_write_with_temp(path, bytes, &tmp)
}

fn atomic_write_with_temp(path: &Path, bytes: &[u8], tmp: &Path) -> io::Result<()> {
    // 予約に失敗した名前は自分のファイルではないため、後始末で削除しない。
    let mut file = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

pub(super) fn refresh(entry: &Registration, root: &Path, force: bool) -> io::Result<&'static str> {
    refresh_at(
        entry,
        root,
        force,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
}

fn refresh_at(
    entry: &Registration,
    root: &Path,
    force: bool,
    now: u64,
) -> io::Result<&'static str> {
    if !valid_command(&entry.command) {
        return Err(io::Error::other("invalid command name"));
    }
    if entry.source == Source::Native {
        return Ok("native");
    }
    fs::create_dir_all(root)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(format!("{}.lock", entry.command)))?;
    if force {
        lock.lock()?;
    } else {
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Ok("updating"),
            Err(TryLockError::Error(error)) => return Err(error),
        }
    }
    let failure_path = root.join(format!("{}.failure", entry.command));
    let result = stamps(entry).and_then(|current| {
        let failure = fs::read(&failure_path)
            .ok()
            .and_then(|s| serde_json::from_slice::<Failure>(&s).ok());
        if !force
            && let Some(failure) = failure.filter(|f| {
                f.version == VERSION
                    && f.registration == *entry
                    && f.stamps == current
                    && now.saturating_sub(f.updated) < FAILURE_BACKOFF
            })
        {
            return Err(io::Error::other(failure.error));
        }
        let result = refresh_locked(entry, root, force, &current, now);
        if let Err(error) = &result {
            let failure = Failure {
                version: VERSION,
                registration: entry.clone(),
                stamps: current,
                updated: now,
                error: error.to_string(),
            };
            if let Ok(bytes) = serde_json::to_vec(&failure) {
                let _ = atomic_write(&failure_path, &bytes);
            }
        } else {
            let _ = fs::remove_file(&failure_path);
        }
        result
    });
    let error_path = root.join(format!("{}.error", entry.command));
    match &result {
        Err(error) => {
            let _ = atomic_write(&error_path, error.to_string().as_bytes());
        }
        Ok(_) => {
            let _ = fs::remove_file(error_path);
        }
    }
    lock.unlock()?;
    result
}

fn refresh_locked(
    entry: &Registration,
    root: &Path,
    force: bool,
    current: &[Stamp],
    now: u64,
) -> io::Result<&'static str> {
    let record_path = root.join(format!("{}.json", entry.command));
    let script_path = root.join(format!("{}.zsh", entry.command));
    let previous = fs::read(&record_path)
        .ok()
        .and_then(|s| serde_json::from_slice::<Record>(&s).ok());
    let reusable = previous
        .as_ref()
        .filter(|r| r.version == VERSION && r.registration == *entry && script_path.is_file());
    if !force && reusable.is_some_and(|r| r.stamps == current) {
        return Ok("cached");
    }
    let hashes = current
        .iter()
        .map(|s| digest(&s.path))
        .collect::<io::Result<Vec<_>>>()?;
    let unchanged_content = !force
        && reusable.is_some_and(|r| {
            r.hashes == hashes
                && r.stamps
                    .iter()
                    .map(|s| &s.path)
                    .eq(current.iter().map(|s| &s.path))
        });
    if !unchanged_content {
        let script = match entry.source {
            Source::Native => return Ok("native"),
            Source::Help => help::generate(&entry.command, &resolve(&entry.command)?)?,
            Source::Generator => {
                let (program, args) = entry
                    .generator
                    .split_first()
                    .ok_or_else(|| io::Error::other("generator is empty"))?;
                let args: Vec<_> = args.iter().map(String::as_str).collect();
                crate::prompt::helpers::run_cmd_inner(
                    resolve(program)?
                        .to_str()
                        .ok_or_else(|| io::Error::other("executable path is not UTF-8"))?,
                    &args,
                    false,
                    Duration::from_secs(10),
                )
                .filter(|s| s.contains("compdef") || s.contains("#compdef"))
                .ok_or_else(|| {
                    io::Error::other(
                        "completion generator failed, timed out, or returned no completion",
                    )
                })?
            }
            Source::File => {
                let script = fs::read_to_string(expand_path(&entry.file))?;
                if script.len() > 4 * 1024 * 1024 || !script.starts_with("#compdef") {
                    return Err(io::Error::other(
                        "expected a zsh #compdef completion file (max 4 MiB)",
                    ));
                }
                let source_path = fs::canonicalize(expand_path(&entry.file))?;
                let name = expand_path(&entry.file)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .filter(|s| {
                        s.starts_with('_')
                            && s.bytes()
                                .all(|c| c.is_ascii_alphanumeric() || b"_-+.".contains(&c))
                    })
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("_{}", entry.command));
                format!(
                    "{name}() {{\n local -a funcsourcetrace=({})\n{script}\n}}\ncompdef {name} {}\n",
                    crate::shell_single_quote(&format!("{}:1", source_path.display())),
                    crate::shell_single_quote(&entry.command)
                )
            }
        };
        if stamps(entry)?.as_slice() != current {
            return Err(io::Error::other(
                "executable changed during generation; retrying on next check",
            ));
        }
        let zsh = resolve("zsh")?;
        let zsh = zsh
            .to_str()
            .ok_or_else(|| io::Error::other("zsh path is not UTF-8"))?;
        let candidate = root.join(format!("{}.pending-{}", entry.command, std::process::id()));
        atomic_write(&candidate, script.as_bytes())?;
        let syntax_ok = crate::prompt::helpers::run_cmd_inner(
            zsh,
            &["-dfn", &candidate.to_string_lossy()],
            true,
            Duration::from_secs(3),
        )
        .is_some();
        if !syntax_ok {
            let _ = fs::remove_file(&candidate);
            return Err(io::Error::other(
                "generated completion contains invalid zsh syntax",
            ));
        }
        let result = fs::rename(&candidate, &script_path);
        if result.is_err() {
            let _ = fs::remove_file(&candidate);
        }
        result?;
    }
    let record = Record {
        version: VERSION,
        registration: entry.clone(),
        stamps: current.to_vec(),
        hashes,
        updated: if unchanged_content {
            previous.map_or(0, |r| r.updated)
        } else {
            now
        },
    };
    atomic_write(&record_path, &serde_json::to_vec(&record)?)?;
    Ok(if unchanged_content {
        "cached"
    } else {
        "updated"
    })
}

pub(super) fn status(entry: &Registration, root: &Path) -> String {
    if entry.source == Source::Native {
        return "native".into();
    }
    if !valid_command(&entry.command) {
        return "invalid command name".into();
    }
    if let Ok(error) = fs::read_to_string(root.join(format!("{}.error", entry.command))) {
        return error
            .chars()
            .filter(|c| !c.is_control())
            .take(160)
            .collect();
    }
    if root.join(format!("{}.zsh", entry.command)).is_file() {
        "ready".into()
    } else {
        "pending".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn 失敗後は再生成を待ち強制更新や監視ファイルの変更では待たない() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let generator = tmp.path().join("generator");
        let count = tmp.path().join("count");
        let mode = tmp.path().join("mode");
        let root = tmp.path().join("cache");
        fs::write(&generator, "#!/bin/sh\nn=0\n[ ! -f \"$1\" ] || read -r n < \"$1\"\nprintf '%s\\n' \"$((n+1))\" > \"$1\"\nread -r mode < \"$2\"\n[ \"$mode\" = ok ] || exit 1\nprintf '%s\\n' '#compdef sh' '_demo() { _arguments \"--flag[Test flag]\"; }' 'compdef _demo sh'\n").unwrap();
        fs::set_permissions(&generator, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(&mode, "failed\n").unwrap();
        let entry = Registration {
            command: "sh".into(),
            source: Source::Generator,
            generator: vec![
                generator.to_string_lossy().into(),
                count.to_string_lossy().into(),
                mode.to_string_lossy().into(),
            ],
            watch_files: vec![mode.to_string_lossy().into()],
            ..Default::default()
        };
        assert!(refresh_at(&entry, &root, false, 1000).is_err());
        assert!(refresh_at(&entry, &root, false, 1001).is_err());
        assert_eq!(fs::read_to_string(&count).unwrap(), "1\n");
        assert!(refresh_at(&entry, &root, false, 1000 + FAILURE_BACKOFF).is_err());
        assert_eq!(fs::read_to_string(&count).unwrap(), "2\n");
        assert!(refresh_at(&entry, &root, true, 1000 + FAILURE_BACKOFF + 1).is_err());
        assert_eq!(fs::read_to_string(&count).unwrap(), "3\n");
        fs::write(&mode, "ok\n").unwrap();
        assert_eq!(refresh_at(&entry, &root, false, 1302).unwrap(), "updated");
        assert_eq!(fs::read_to_string(&count).unwrap(), "4\n");
        assert!(!root.join("sh.failure").exists());
        assert!(!root.join("sh.error").exists());
        let working = fs::read(root.join("sh.zsh")).unwrap();
        fs::write(&mode, "failed\n").unwrap();
        assert!(refresh_at(&entry, &root, false, 1303).is_err());
        assert!(refresh_at(&entry, &root, false, 1304).is_err());
        assert_eq!(fs::read_to_string(&count).unwrap(), "5\n");
        assert_eq!(fs::read(root.join("sh.zsh")).unwrap(), working);
        let mut changed = entry;
        changed.generator.push("changed-setting".into());
        assert!(refresh_at(&changed, &root, false, 1305).is_err());
        assert_eq!(fs::read_to_string(&count).unwrap(), "6\n");
    }

    #[test]
    fn 一時ファイルの予約失敗では既存ファイルを削除しない() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("cache.zsh");
        let reserved = tmp.path().join("reserved");
        fs::write(&dest, b"working").unwrap();
        fs::write(&reserved, b"another writer").unwrap();
        assert_eq!(
            atomic_write_with_temp(&dest, b"updated", &reserved)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(&dest).unwrap(), b"working");
        assert_eq!(fs::read(&reserved).unwrap(), b"another writer");
    }

    #[test]
    fn 置換の失敗では自分の一時ファイルだけを削除する() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("directory");
        let reserved = tmp.path().join("reserved");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("keep"), b"working").unwrap();
        assert!(atomic_write_with_temp(&dest, b"updated", &reserved).is_err());
        assert!(!reserved.exists());
        assert_eq!(fs::read(dest.join("keep")).unwrap(), b"working");
    }
}
