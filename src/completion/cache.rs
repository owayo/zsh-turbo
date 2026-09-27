use super::{Registration, Source, expand_path, help, valid_command};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
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

fn stamp(path: &Path) -> io::Result<Stamp> {
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
    let path = Path::new(command);
    if path.components().count() > 1 {
        return fs::canonicalize(path);
    }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|p| p.join(command))
        .find(|p| {
            fs::metadata(p).is_ok_and(|m| {
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
}

fn stamps(entry: &Registration) -> io::Result<Vec<Stamp>> {
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
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

pub(super) fn refresh(entry: &Registration, root: &Path, force: bool) -> io::Result<&'static str> {
    if !valid_command(&entry.command) {
        return Err(io::Error::other("invalid command name"));
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
    let result = refresh_locked(entry, root, force);
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

fn refresh_locked(entry: &Registration, root: &Path, force: bool) -> io::Result<&'static str> {
    let record_path = root.join(format!("{}.json", entry.command));
    let script_path = root.join(format!("{}.zsh", entry.command));
    let previous = fs::read(&record_path)
        .ok()
        .and_then(|s| serde_json::from_slice::<Record>(&s).ok());
    let current = stamps(entry)?;
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
            Source::Help => help::generate(&entry.command)?,
            Source::Generator => {
                let (program, args) = entry
                    .generator
                    .split_first()
                    .ok_or_else(|| io::Error::other("generator is empty"))?;
                let args: Vec<_> = args.iter().map(String::as_str).collect();
                crate::prompt::helpers::run_cmd_inner(
                    program,
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
        if stamps(entry)? != current {
            return Err(io::Error::other(
                "executable changed during generation; retrying on next check",
            ));
        }
        let candidate = root.join(format!("{}.pending-{}", entry.command, std::process::id()));
        atomic_write(&candidate, script.as_bytes())?;
        let syntax_ok = crate::prompt::helpers::run_cmd_inner(
            "zsh",
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
        stamps: current,
        hashes,
        updated: if unchanged_content {
            previous.map_or(0, |r| r.updated)
        } else {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
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
