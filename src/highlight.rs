use std::collections::HashSet;
use std::env;
use std::path::Path;

/// A highlight span: start position, end position, style string
#[derive(Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub style: String,
}

/// Classify tokens in a shell command line and return highlight spans.
pub fn highlight_buffer(buffer: &str, aliases: &str, functions: &str) -> Vec<Span> {
    if buffer.is_empty() {
        return Vec::new();
    }

    let alias_set: HashSet<&str> = aliases.split('\n').filter(|s| !s.is_empty()).collect();
    let func_set: HashSet<&str> = functions.split('\n').filter(|s| !s.is_empty()).collect();

    let path_dirs = build_path_cache();
    let mut spans = Vec::new();
    let mut pos = 0;
    let mut is_command_position = true;
    let mut after_pipe_or_semi = false;

    let chars: Vec<char> = buffer.chars().collect();

    while pos < chars.len() {
        // Skip whitespace
        if chars[pos].is_whitespace() {
            pos += 1;
            continue;
        }

        let token_start = pos;

        // Detect token type and advance pos
        if chars[pos] == '\'' {
            // Single-quoted string
            pos += 1;
            while pos < chars.len() && chars[pos] != '\'' {
                pos += 1;
            }
            if pos < chars.len() {
                pos += 1;
            }
            spans.push(Span {
                start: token_start,
                end: pos,
                style: "fg=yellow".into(),
            });
            is_command_position = false;
            continue;
        }

        if chars[pos] == '"' {
            // Double-quoted string
            pos += 1;
            while pos < chars.len() && chars[pos] != '"' {
                if chars[pos] == '\\' {
                    pos += 1;
                }
                pos += 1;
            }
            if pos < chars.len() {
                pos += 1;
            }
            spans.push(Span {
                start: token_start,
                end: pos,
                style: "fg=yellow".into(),
            });
            is_command_position = false;
            continue;
        }

        if chars[pos] == '$' && pos + 1 < chars.len() {
            // Variable
            pos += 1;
            if chars[pos] == '{' {
                while pos < chars.len() && chars[pos] != '}' {
                    pos += 1;
                }
                if pos < chars.len() {
                    pos += 1;
                }
            } else {
                while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                    pos += 1;
                }
            }
            spans.push(Span {
                start: token_start,
                end: pos,
                style: "fg=cyan".into(),
            });
            is_command_position = false;
            continue;
        }

        // Operators: |, ||, &&, ;, &
        if matches!(chars[pos], '|' | '&' | ';') {
            let op_start = pos;
            let ch = chars[pos];
            pos += 1;
            if pos < chars.len() && chars[pos] == ch {
                pos += 1; // || or &&
            }
            spans.push(Span {
                start: op_start,
                end: pos,
                style: "fg=magenta".into(),
            });
            is_command_position = true;
            after_pipe_or_semi = true;
            continue;
        }

        // Redirections: >, >>, <, 2>, &>
        if matches!(chars[pos], '>' | '<') {
            let op_start = pos;
            pos += 1;
            if pos < chars.len() && (chars[pos] == '>' || chars[pos] == '&') {
                pos += 1;
            }
            spans.push(Span {
                start: op_start,
                end: pos,
                style: "fg=magenta".into(),
            });
            is_command_position = false;
            continue;
        }

        // Regular word token
        let word_start = pos;
        while pos < chars.len()
            && !chars[pos].is_whitespace()
            && !matches!(chars[pos], '|' | '&' | ';' | '>' | '<' | '\'' | '"')
        {
            if chars[pos] == '\\' && pos + 1 < chars.len() {
                pos += 1;
            }
            pos += 1;
        }
        let word: String = chars[word_start..pos].iter().collect();

        if is_command_position || after_pipe_or_semi {
            // Command position: classify the word
            let style = classify_command(&word, &alias_set, &func_set, &path_dirs);
            spans.push(Span {
                start: word_start,
                end: pos,
                style,
            });
            is_command_position = false;
            after_pipe_or_semi = false;
        } else if word.starts_with('-') {
            // Option
            spans.push(Span {
                start: word_start,
                end: pos,
                style: "fg=cyan".into(),
            });
        } else if word.contains('/') || word.starts_with('.') || word.starts_with('~') {
            // Path-like
            let style = if path_exists(&word) {
                "fg=magenta,underline"
            } else {
                "fg=magenta"
            };
            spans.push(Span {
                start: word_start,
                end: pos,
                style: style.into(),
            });
        } else if word.contains('*') || word.contains('?') || word.contains('[') {
            // Glob
            spans.push(Span {
                start: word_start,
                end: pos,
                style: "fg=blue,bold".into(),
            });
        }
        // Plain arguments get no special highlighting (default terminal color)
    }

    spans
}

/// Known zsh/bash builtins
const BUILTINS: &[&str] = &[
    "alias",
    "autoload",
    "bg",
    "bindkey",
    "break",
    "builtin",
    "cd",
    "chdir",
    "command",
    "compctl",
    "compadd",
    "compdef",
    "continue",
    "declare",
    "dirs",
    "echo",
    "emulate",
    "enable",
    "eval",
    "exec",
    "exit",
    "export",
    "false",
    "fc",
    "fg",
    "float",
    "functions",
    "getln",
    "getopts",
    "hash",
    "history",
    "integer",
    "jobs",
    "kill",
    "let",
    "limit",
    "local",
    "log",
    "logout",
    "noglob",
    "popd",
    "print",
    "printf",
    "pushd",
    "pwd",
    "read",
    "readonly",
    "rehash",
    "return",
    "set",
    "setopt",
    "shift",
    "source",
    "suspend",
    "test",
    "times",
    "trap",
    "true",
    "ttyctl",
    "type",
    "typeset",
    "ulimit",
    "umask",
    "unalias",
    "unfunction",
    "unhash",
    "unlimit",
    "unset",
    "unsetopt",
    "vared",
    "wait",
    "whence",
    "where",
    "which",
    "zcompile",
    "zformat",
    "zle",
    "zmodload",
    "zparseopts",
    "zregexparse",
    "zstyle",
];

const RESERVED_WORDS: &[&str] = &[
    "if",
    "then",
    "else",
    "elif",
    "fi",
    "for",
    "in",
    "do",
    "done",
    "while",
    "until",
    "repeat",
    "case",
    "esac",
    "select",
    "function",
    "coproc",
    "nocorrect",
    "foreach",
    "end",
    "time",
];

fn classify_command(
    word: &str,
    aliases: &HashSet<&str>,
    functions: &HashSet<&str>,
    path_dirs: &[String],
) -> String {
    // sudo, env, command, etc. → treat next word as command
    if matches!(
        word,
        "sudo" | "env" | "command" | "builtin" | "exec" | "noglob" | "nocorrect"
    ) {
        return "fg=green,bold".into();
    }

    if RESERVED_WORDS.contains(&word) {
        return "fg=yellow,bold".into();
    }

    if BUILTINS.contains(&word) {
        return "fg=green".into();
    }

    if aliases.contains(word) || functions.contains(word) {
        return "fg=green".into();
    }

    // Check $PATH
    if command_in_path(word, path_dirs) {
        return "fg=green".into();
    }

    // Check if it's a path to an executable
    if word.contains('/') {
        let p = Path::new(word);
        if p.exists() {
            return "fg=green,underline".into();
        }
    }

    // Unknown command
    "fg=red,bold".into()
}

fn build_path_cache() -> Vec<String> {
    env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(String::from)
        .collect()
}

fn command_in_path(cmd: &str, path_dirs: &[String]) -> bool {
    if cmd.is_empty() || cmd.contains('/') {
        return false;
    }
    path_dirs
        .iter()
        .any(|dir| Path::new(dir).join(cmd).is_file())
}

fn path_exists(path: &str) -> bool {
    let expanded = if let Some(rest) = path.strip_prefix('~') {
        if let Some(home) = dirs::home_dir() {
            home.join(rest.trim_start_matches('/'))
        } else {
            return false;
        }
    } else {
        Path::new(path).to_path_buf()
    };
    expanded.exists()
}

/// Format spans as newline-separated "start end style" for zsh consumption
pub fn format_spans(spans: &[Span]) -> String {
    spans
        .iter()
        .map(|s| format!("{} {} {}", s.start, s.end, s.style))
        .collect::<Vec<_>>()
        .join("\n")
}
