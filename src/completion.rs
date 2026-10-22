use crate::shell_single_quote;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod cache;
mod help;
mod subcommands;

pub use subcommands::{TopLevel, subcommand_query, top_level};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Help,
    Generator,
    File,
    Native,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Registration {
    pub command: String,
    pub enabled: bool,
    pub source: Source,
    pub generator: Vec<String>,
    pub file: String,
    pub watch_files: Vec<String>,
}

impl Default for Registration {
    fn default() -> Self {
        Self {
            command: String::new(),
            enabled: true,
            source: Source::Help,
            generator: Vec::new(),
            file: String::new(),
            watch_files: Vec::new(),
        }
    }
}

pub fn defaults() -> Vec<Registration> {
    vec![Registration {
        command: "git".into(),
        source: Source::Native,
        ..Default::default()
    }]
}

pub fn valid_command(name: &str) -> bool {
    name.bytes()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-+.".contains(&c))
        && name != "zsh-turbo"
}

pub fn cache_dir() -> Option<PathBuf> {
    cache_dir_from(std::env::var_os("XDG_CACHE_HOME"), dirs::home_dir())
}

fn cache_dir_from(
    cache_home: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    let base = cache_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.filter(|path| path.is_absolute())
                .map(|path| path.join(".cache"))
        })?;
    Some(base.join("zsh-turbo/managed-completions"))
}

pub fn refresh_all(entries: &[Registration], force: bool) -> Vec<String> {
    let root = cache_dir();
    entries
        .iter()
        .filter(|e| e.enabled)
        .map(|entry| {
            let result = if entry.source == Source::Native {
                Ok("native")
            } else {
                root.as_deref()
                    .ok_or_else(|| std::io::Error::other("no absolute completion cache directory"))
                    .and_then(|root| cache::refresh(entry, root, force))
            };
            match result {
                Ok(state) => format!("{}: {state}", entry.command),
                Err(error) => format!("{}: {error}", entry.command),
            }
        })
        .collect()
}

pub fn status(entry: &Registration) -> String {
    if entry.source == Source::Native {
        return "native".into();
    }
    cache_dir().map_or_else(
        || "no absolute completion cache directory".into(),
        |root| cache::status(entry, &root),
    )
}

pub fn shell_init(entries: &[Registration]) -> String {
    let entries: Vec<_> = entries
        .iter()
        .filter(|e| e.enabled && e.source != Source::Native && valid_command(&e.command))
        .collect();
    if entries.is_empty() {
        return String::new();
    }
    let Some(root) = cache_dir() else {
        return String::new();
    };
    let mut script = format!(
        "\ntypeset -g _ZSH_TURBO_COMPLETION_CACHE={}\ntypeset -ga _ZSH_TURBO_COMPLETION_COMMANDS=(",
        shell_single_quote(&root.to_string_lossy())
    );
    for entry in entries {
        script.push_str(&format!("{} ", shell_single_quote(&entry.command)));
    }
    script.push_str(")\n");
    script.push_str(include_str!("../shell/completion.zsh"));
    script
}

pub fn expand_path(path: &str) -> std::io::Result<PathBuf> {
    expand_path_from(path, dirs::home_dir())
}

fn expand_path_from(path: &str, home: Option<PathBuf>) -> std::io::Result<PathBuf> {
    if let Some(rest) = path.strip_prefix("~/") {
        return home
            .filter(|home| home.is_absolute())
            .map(|home| home.join(rest))
            .ok_or_else(|| std::io::Error::other("no absolute home directory"));
    }
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(std::io::Error::other("no absolute completion source path"))
    }
}

pub fn valid_source_path(path: &str) -> bool {
    std::path::Path::new(path).is_absolute() || path.starts_with("~/")
}

pub fn valid_generator(program: &str) -> bool {
    crate::executable::valid_program(program)
}

pub fn validate(entries: &[Registration]) -> Result<(), &'static str> {
    let mut names = std::collections::HashSet::new();
    for entry in entries {
        if !valid_command(&entry.command) || !names.insert(&entry.command) {
            return Err("Use a unique command name (letters, digits, -, _, +, .).");
        }
        if entry.source == Source::Generator
            && entry.generator.first().is_none_or(|s| !valid_generator(s))
        {
            return Err("Generator must start with a PATH command, an absolute path, or ~/.");
        }
        if (entry.source == Source::File && !valid_source_path(&entry.file))
            || entry
                .watch_files
                .iter()
                .any(|path| !valid_source_path(path))
        {
            return Err("Completion and watched files must use an absolute path or ~/.");
        }
    }
    Ok(())
}

pub fn refresh_saved_async() {
    if let Ok(executable) = std::env::current_exe()
        && let Ok(mut child) = std::process::Command::new(executable)
            .arg("completion-refresh")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(test)]
mod tests;
