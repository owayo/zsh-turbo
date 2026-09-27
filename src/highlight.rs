use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::sync::LazyLock;

/// ハイライトスパン: 開始位置、終了位置、スタイル文字列
#[derive(Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub style: String,
}

/// 組み込みコマンド・予約語の O(1) 検索用 HashSet（事前計算）。
static BUILTIN_SET: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| BUILTINS.iter().copied().collect());
static RESERVED_SET: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| RESERVED_WORDS.iter().copied().collect());

/// シェルコマンドラインのトークンを分類し、ハイライトスパンを返す。
pub fn highlight_buffer(buffer: &str, aliases: &str, functions: &str) -> Vec<Span> {
    if buffer.is_empty() {
        return Vec::new();
    }

    let alias_set: HashSet<&str> = aliases.split('\n').filter(|s| !s.is_empty()).collect();
    let func_set: HashSet<&str> = functions.split('\n').filter(|s| !s.is_empty()).collect();

    let path_cmds = build_path_cache();
    let mut spans = Vec::new();
    let mut pos = 0;
    let mut is_command_position = true;
    let mut after_pipe_or_semi = false;
    // 直前の語が sudo/env 等のプレフィックスコマンドだったか。
    // プレフィックス直後のオプション (`sudo -u` 等) の扱いにのみ使う。
    let mut after_prefix_command = false;

    let chars: Vec<char> = buffer.chars().collect();

    while pos < chars.len() {
        // 改行はシェル構文上のコマンド区切りとして扱う。
        if chars[pos] == '\n' {
            pos += 1;
            is_command_position = true;
            after_pipe_or_semi = true;
            after_prefix_command = false;
            continue;
        }

        // 空白をスキップ
        if chars[pos].is_whitespace() {
            pos += 1;
            continue;
        }

        let token_start = pos;

        if chars[pos] == '$' && pos + 1 < chars.len() {
            // 変数
            pos += 1;
            if chars[pos] == '{' {
                while pos < chars.len() && chars[pos] != '}' {
                    pos += 1;
                }
                if pos < chars.len() {
                    pos += 1;
                }
            } else if is_zsh_special_parameter_char(chars[pos]) {
                // `$?`, `$$`, `$!` などの特殊パラメータは 1 文字だけで成立する。
                pos += 1;
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
            // 変数展開がコマンド名として消費されるため、区切り直後フラグも落とす。
            is_command_position = false;
            after_pipe_or_semi = false;
            after_prefix_command = false;
            continue;
        }

        // リダイレクト: >, >>, <, 2>, &>, 2>&, 0<&
        if chars[pos].is_ascii_digit() {
            let mut fd_end = pos;
            while fd_end < chars.len() && chars[fd_end].is_ascii_digit() {
                fd_end += 1;
            }
            if fd_end < chars.len() && matches!(chars[fd_end], '>' | '<') {
                let op_start = pos;
                let is_out = chars[fd_end] == '>';
                pos = fd_end + 1;
                // `N>>` の 2 文字目、または `N>&`/`N<&` の fd 複製演算子。
                // bare な `>&`/`<&` 分岐と挙動を揃える。
                if pos < chars.len() && (chars[pos] == '>' || chars[pos] == '&') {
                    pos += 1;
                }
                // `N>|` (CLOBBER 上書き) の `|` は演算子の一部。パイプ扱いにすると
                // リダイレクト先ファイル名がコマンド分類されてしまう。
                if is_out && pos < chars.len() && chars[pos] == '|' {
                    pos += 1;
                }
                spans.push(Span {
                    start: op_start,
                    end: pos,
                    style: "fg=magenta".into(),
                });
                is_command_position = false;
                // `;`/`|` 直後にリダイレクトが来た場合、次の単語はリダイレクト先
                // (ファイル名等) であって新コマンドではないため、after_pipe_or_semi も
                // 落とす必要がある (例: `; > file nextcmd` の file をコマンドと誤認しない)。
                after_pipe_or_semi = false;
                after_prefix_command = false;
                continue;
            }
        }

        if chars[pos] == '&' && pos + 1 < chars.len() && chars[pos + 1] == '>' {
            let op_start = pos;
            pos += 2;
            if pos < chars.len() && chars[pos] == '>' {
                pos += 1;
            }
            spans.push(Span {
                start: op_start,
                end: pos,
                style: "fg=magenta".into(),
            });
            is_command_position = false;
            after_pipe_or_semi = false;
            after_prefix_command = false;
            continue;
        }

        if matches!(chars[pos], '>' | '<') {
            let op_start = pos;
            let is_out = chars[pos] == '>';
            pos += 1;
            if pos < chars.len() && (chars[pos] == '>' || chars[pos] == '&') {
                pos += 1;
            }
            // `>|`/`>>|`/`>&|` (CLOBBER 上書き) の `|` は演算子の一部。
            if is_out && pos < chars.len() && chars[pos] == '|' {
                pos += 1;
            }
            spans.push(Span {
                start: op_start,
                end: pos,
                style: "fg=magenta".into(),
            });
            is_command_position = false;
            after_pipe_or_semi = false;
            after_prefix_command = false;
            continue;
        }

        // 演算子: |, ||, &&, ;, &
        if matches!(chars[pos], '|' | '&' | ';') {
            let op_start = pos;
            let ch = chars[pos];
            pos += 1;
            if pos < chars.len() && chars[pos] == ch {
                pos += 1; // || または && の 2 文字目
            }
            spans.push(Span {
                start: op_start,
                end: pos,
                style: "fg=magenta".into(),
            });
            is_command_position = true;
            after_pipe_or_semi = true;
            after_prefix_command = false;
            continue;
        }

        // 通常の単語トークン。クォートは shell の語結合と同様に単語の一部として
        // 閉じクォートまで消費する (`FOO='a b' cmd` や `ec'ho'` を 1 語に保つ)。
        let word_start = pos;
        // クォートセグメントの範囲 (黄色で塗る)
        let mut quote_segs: Vec<(usize, usize)> = Vec::new();
        // クォートを剥いだ論理語 (コマンド分類・代入判定用)
        let mut logical = String::new();
        // クォート外の文字のみ (引数位置でのオプション/パス/glob 判定用)
        let mut bare = String::new();
        while pos < chars.len()
            && !chars[pos].is_whitespace()
            && !matches!(chars[pos], '|' | '&' | ';' | '>' | '<')
        {
            if chars[pos] == '\'' {
                // シングルクォートセグメント
                let seg_start = pos;
                pos += 1;
                while pos < chars.len() && chars[pos] != '\'' {
                    logical.push(chars[pos]);
                    pos += 1;
                }
                if pos < chars.len() {
                    pos += 1;
                }
                quote_segs.push((seg_start, pos));
                continue;
            }
            if chars[pos] == '"' {
                // ダブルクォートセグメント
                let seg_start = pos;
                pos += 1;
                while pos < chars.len() && chars[pos] != '"' {
                    if chars[pos] == '\\' && pos + 1 < chars.len() {
                        logical.push(chars[pos]);
                        pos += 1;
                    }
                    logical.push(chars[pos]);
                    pos += 1;
                }
                if pos < chars.len() {
                    pos += 1;
                }
                quote_segs.push((seg_start, pos));
                continue;
            }
            if chars[pos] == '\\' && pos + 1 < chars.len() {
                // 先頭の `\cmd` だけはエイリアスを抑止する意味を分類時まで保持する。
                // それ以外のバックスラッシュは shell と同様に論理語から取り除く。
                if pos == word_start {
                    logical.push('\\');
                }
                // エスケープ対象はクォート済みなので glob/option/path 判定から除外する。
                // `\\` を番兵として残し、後続文字が誤って語頭扱いされるのを防ぐ。
                bare.push('\\');
                pos += 1;
                logical.push(chars[pos]);
                pos += 1;
                continue;
            }
            logical.push(chars[pos]);
            bare.push(chars[pos]);
            pos += 1;
        }

        if is_command_position || after_pipe_or_semi {
            if bare.is_empty() && !quote_segs.is_empty() {
                // 全体がクォートの語 (`; 'ls' -la` 等) はコマンド名として消費される。
                // 中身は分類せず文字列色のみ付け、後続をコマンド誤分類しない。
                push_quote_segs(&mut spans, &quote_segs);
                is_command_position = false;
                after_pipe_or_semi = false;
                after_prefix_command = false;
                continue;
            }

            if is_assignment_word(&logical) {
                spans.push(Span {
                    start: word_start,
                    end: pos,
                    style: "fg=cyan".into(),
                });
                push_quote_segs(&mut spans, &quote_segs);
                is_command_position = true;
                after_pipe_or_semi = false;
                continue;
            }

            if after_prefix_command && logical.starts_with('-') {
                // プレフィックスコマンド直後のオプション (`sudo -u` 等) は
                // 未知コマンド (赤) ではなくオプションとして扱う。値付きオプションを
                // 区別できないため、以降はコマンド位置を消費する (誤検出ゼロ優先)。
                spans.push(Span {
                    start: word_start,
                    end: pos,
                    style: "fg=cyan".into(),
                });
                is_command_position = false;
                after_pipe_or_semi = false;
                after_prefix_command = false;
                continue;
            }

            // コマンド位置: 単語を分類
            let style = classify_command(&logical, &alias_set, &func_set, &path_cmds);
            spans.push(Span {
                start: word_start,
                end: pos,
                style,
            });
            push_quote_segs(&mut spans, &quote_segs);
            // プレフィックスコマンドや `if`/`then` 等の後続コマンドを取る予約語では
            // 次の単語もコマンド位置を維持する。
            is_command_position = keeps_command_position(&logical);
            after_prefix_command = is_prefix_command(&logical);
            after_pipe_or_semi = false;
        } else if bare.starts_with('-') {
            // オプション
            spans.push(Span {
                start: word_start,
                end: pos,
                style: "fg=cyan".into(),
            });
            push_quote_segs(&mut spans, &quote_segs);
        } else if bare.contains('/') || bare.starts_with('.') || bare.starts_with('~') {
            // パス風。実在判定はクォートを剥いだ論理語で行う。
            let style = if path_exists(&logical) {
                "fg=magenta,underline"
            } else {
                "fg=magenta"
            };
            spans.push(Span {
                start: word_start,
                end: pos,
                style: style.into(),
            });
            push_quote_segs(&mut spans, &quote_segs);
        } else if bare.contains('*') || bare.contains('?') || bare.contains('[') {
            // glob パターン (クォート内の `*` 等は対象外)
            spans.push(Span {
                start: word_start,
                end: pos,
                style: "fg=blue,bold".into(),
            });
            push_quote_segs(&mut spans, &quote_segs);
        } else {
            // 通常の引数はハイライトなし（端末のデフォルト色）。
            // ただしクォートセグメントは文字列色を付ける。
            push_quote_segs(&mut spans, &quote_segs);
        }
    }

    spans
}

/// クォートセグメントを文字列色スパンとして追加する。
/// 単語全体のスパンより後に push することで region_highlight 上で優先される。
fn push_quote_segs(spans: &mut Vec<Span>, segs: &[(usize, usize)]) {
    for &(start, end) in segs {
        spans.push(Span {
            start,
            end,
            style: "fg=yellow".into(),
        });
    }
}

/// 後続の単語をコマンド位置として扱い続ける語。
/// プレフィックスコマンドに加え、後続がコマンドである予約語を含む。
fn keeps_command_position(word: &str) -> bool {
    is_prefix_command(word)
        || matches!(
            word,
            "if" | "then"
                | "else"
                | "elif"
                | "do"
                | "while"
                | "until"
                | "time"
                | "coproc"
                | "!"
                | "{"
        )
}

/// sudo, env 等のプレフィックスコマンド（次の単語もコマンドとして扱う）
fn is_prefix_command(word: &str) -> bool {
    matches!(
        word,
        "sudo" | "env" | "command" | "builtin" | "exec" | "noglob" | "nocorrect"
    )
}

fn is_assignment_word(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn is_zsh_special_parameter_char(c: char) -> bool {
    matches!(c, '?' | '!' | '$' | '#' | '-' | '*' | '@')
}

/// zsh/bash 組み込みコマンド
const BUILTINS: &[&str] = &[
    ".",
    ":",
    "[",
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
    "!",
    "{",
    "}",
    "[[",
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
    path_cmds: &HashSet<OsString>,
) -> String {
    // `\cmd` はエイリアス展開・予約語解釈を抑止する zsh の常用イディオム。
    // 分類は `\` を剥いだ語で行うが、予約語・エイリアスには一致させない。
    if let Some(lookup) = word.strip_prefix('\\') {
        if BUILTIN_SET.contains(lookup)
            || functions.contains(lookup)
            || command_in_path(lookup, path_cmds)
        {
            return "fg=green".into();
        }
        if lookup.contains('/') && path_exists(lookup) {
            return "fg=green,underline".into();
        }
        return "fg=red,bold".into();
    }

    // sudo, env, command 等 → 次の単語もコマンドとして扱う
    if is_prefix_command(word) {
        return "fg=green,bold".into();
    }

    if RESERVED_SET.contains(word) {
        return "fg=yellow,bold".into();
    }

    if BUILTIN_SET.contains(word) {
        return "fg=green".into();
    }

    if aliases.contains(word) || functions.contains(word) {
        return "fg=green".into();
    }

    // キャッシュされた $PATH エントリから検索
    if command_in_path(word, path_cmds) {
        return "fg=green".into();
    }

    // 実行可能ファイルへのパスか確認 (`~/bin/x` のチルダ展開込み)
    if word.contains('/') && path_exists(word) {
        return "fg=green,underline".into();
    }

    // 不明なコマンド
    "fg=red,bold".into()
}

/// $PATH 内のすべての実行可能ファイル名を HashSet に格納する。
/// read_dir() 一括走査でコマンドごとの stat() を回避する。
fn build_path_cache() -> HashSet<OsString> {
    build_path_cache_from(&env::var("PATH").unwrap_or_default())
}

fn build_path_cache_from(path_env: &str) -> HashSet<OsString> {
    let current_dir = env::current_dir().ok();
    build_path_cache_from_in(path_env, current_dir.as_deref())
}

fn build_path_cache_from_in(path_env: &str, current_dir: Option<&Path>) -> HashSet<OsString> {
    let mut cmds = HashSet::new();
    for dir in path_env.split(':') {
        // zsh の PATH では空要素がカレントディレクトリを表す。
        let path = if dir.is_empty() {
            let Some(current_dir) = current_dir else {
                continue;
            };
            current_dir
        } else {
            Path::new(dir)
        };
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                if is_executable_entry(&entry) {
                    cmds.insert(entry.file_name());
                }
            }
        }
    }
    cmds
}

#[cfg(unix)]
fn is_executable_entry(entry: &std::fs::DirEntry) -> bool {
    use std::os::unix::fs::PermissionsExt;

    // DirEntry::metadata() は symlink を辿らない (lstat 相当) ため、Homebrew 等の
    // symlink されたコマンドが全て除外されてしまう。辿る fs::metadata を使う。
    // dangling symlink は除外のままにする。
    std::fs::metadata(entry.path())
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_entry(entry: &std::fs::DirEntry) -> bool {
    entry.path().is_file()
}

fn command_in_path(cmd: &str, path_cmds: &HashSet<OsString>) -> bool {
    if cmd.is_empty() || cmd.contains('/') {
        return false;
    }
    path_cmds.contains(OsString::from(cmd).as_os_str())
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

/// スパンを改行区切りの "start end style" 形式にフォーマットする（zsh 用）
pub fn format_spans(spans: &[Span]) -> String {
    spans
        .iter()
        .map(|s| format!("{} {} {}", s.start, s.end, s.style))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    // ---------------------------------------------------------------
    // スパン文字列化のテスト
    // ---------------------------------------------------------------

    #[test]
    fn format_spans_empty() {
        assert_eq!(format_spans(&[]), "");
    }

    #[test]
    fn format_spans_single() {
        let spans = vec![Span {
            start: 0,
            end: 2,
            style: "fg=green".into(),
        }];
        assert_eq!(format_spans(&spans), "0 2 fg=green");
    }

    #[test]
    fn format_spans_multiple() {
        let spans = vec![
            Span {
                start: 0,
                end: 2,
                style: "fg=green".into(),
            },
            Span {
                start: 3,
                end: 6,
                style: "fg=red,bold".into(),
            },
        ];
        assert_eq!(format_spans(&spans), "0 2 fg=green\n3 6 fg=red,bold");
    }

    // ---------------------------------------------------------------
    // PATH 内コマンド判定のテスト
    // ---------------------------------------------------------------

    #[test]
    fn command_in_path_empty_cmd() {
        let set = HashSet::new();
        assert!(!command_in_path("", &set));
    }

    #[test]
    fn command_in_path_with_slash() {
        let set = HashSet::from([OsString::from("foo")]);
        assert!(!command_in_path("/usr/bin/foo", &set));
    }

    #[test]
    fn command_in_path_found() {
        let set = HashSet::from([OsString::from("rustc")]);
        assert!(command_in_path("rustc", &set));
    }

    #[test]
    fn command_in_path_not_found() {
        let set = HashSet::from([OsString::from("rustc")]);
        assert!(!command_in_path("nonexistent_xyz", &set));
    }

    #[cfg(unix)]
    #[test]
    fn build_path_cache_from_実行可能ファイルだけを登録する() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("zt-executable");
        let plain = dir.path().join("zt-plain");
        std::fs::write(&executable, "").unwrap();
        std::fs::write(&plain, "").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();

        let set = build_path_cache_from(dir.path().to_str().unwrap());

        assert!(set.contains(OsString::from("zt-executable").as_os_str()));
        assert!(!set.contains(OsString::from("zt-plain").as_os_str()));
    }

    #[cfg(unix)]
    #[test]
    fn build_path_cache_from_空のpath要素はカレントディレクトリを探す() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("zt-local-command");
        std::fs::write(&executable, "").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();

        let set = build_path_cache_from_in(":/path/that/does/not/exist", Some(dir.path()));
        assert!(set.contains(OsString::from("zt-local-command").as_os_str()));
    }

    // ---------------------------------------------------------------
    // バッファ全体のハイライトテスト
    // ---------------------------------------------------------------

    /// テスト補助: トークンテキストとスタイルのペアを収集する。
    fn span_styles(buffer: &str, aliases: &str, functions: &str) -> Vec<(String, String)> {
        let spans = highlight_buffer(buffer, aliases, functions);
        spans
            .iter()
            .map(|s| {
                let text: String = buffer.chars().skip(s.start).take(s.end - s.start).collect();
                (text, s.style.clone())
            })
            .collect()
    }

    #[test]
    fn highlight_empty_buffer() {
        let spans = highlight_buffer("", "", "");
        assert!(spans.is_empty());
    }

    #[test]
    fn highlight_builtin_cd() {
        let pairs = span_styles("cd", "", "");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "cd");
        assert_eq!(pairs[0].1, "fg=green");
    }

    #[test]
    fn highlight_builtin_echo() {
        let pairs = span_styles("echo hello", "", "");
        assert_eq!(pairs[0].0, "echo");
        assert_eq!(pairs[0].1, "fg=green");
    }

    #[test]
    fn highlight_unknown_command() {
        let pairs = span_styles("zzz_no_such_cmd_12345", "", "");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].1, "fg=red,bold");
    }

    #[test]
    fn highlight_pipe() {
        let pairs = span_styles("echo hello | cd", "", "");
        // "echo" = 組み込み, "|" = 演算子, "cd" = 組み込み
        let pipe = pairs.iter().find(|(t, _)| t == "|");
        assert!(pipe.is_some());
        assert_eq!(pipe.unwrap().1, "fg=magenta");

        // パイプ後の "cd" もコマンドとして分類されるべき
        let cd = pairs.iter().find(|(t, _)| t == "cd");
        assert!(cd.is_some());
        assert_eq!(cd.unwrap().1, "fg=green");
    }

    #[test]
    fn highlight_double_ampersand() {
        let pairs = span_styles("echo ok && cd", "", "");
        let op = pairs.iter().find(|(t, _)| t == "&&");
        assert!(op.is_some());
        assert_eq!(op.unwrap().1, "fg=magenta");
    }

    #[test]
    fn highlight_semicolon() {
        let pairs = span_styles("echo a ; cd", "", "");
        let op = pairs.iter().find(|(t, _)| t == ";");
        assert!(op.is_some());
        assert_eq!(op.unwrap().1, "fg=magenta");
    }

    #[test]
    fn highlight_single_quoted_string() {
        let pairs = span_styles("echo 'hello world'", "", "");
        let sq = pairs.iter().find(|(t, _)| t == "'hello world'");
        assert!(sq.is_some());
        assert_eq!(sq.unwrap().1, "fg=yellow");
    }

    #[test]
    fn highlight_double_quoted_string() {
        let pairs = span_styles("echo \"hello world\"", "", "");
        let dq = pairs.iter().find(|(t, _)| t == "\"hello world\"");
        assert!(dq.is_some());
        assert_eq!(dq.unwrap().1, "fg=yellow");
    }

    #[test]
    fn highlight_variable_dollar_name() {
        let pairs = span_styles("echo $HOME", "", "");
        let var = pairs.iter().find(|(t, _)| t == "$HOME");
        assert!(var.is_some());
        assert_eq!(var.unwrap().1, "fg=cyan");
    }

    #[test]
    fn highlight_variable_braces() {
        let pairs = span_styles("echo ${VAR}", "", "");
        let var = pairs.iter().find(|(t, _)| t == "${VAR}");
        assert!(var.is_some());
        assert_eq!(var.unwrap().1, "fg=cyan");
    }

    #[test]
    fn highlight_zsh特殊パラメータ_question() {
        let pairs = span_styles("echo $?", "", "");
        let var = pairs.iter().find(|(t, _)| t == "$?");
        assert!(var.is_some(), "$? が 1 つの変数スパンとして扱われていない");
        assert_eq!(var.unwrap().1, "fg=cyan");
        assert!(
            pairs.iter().all(|(t, _)| t != "?"),
            "? だけが glob として分離されている: {pairs:?}"
        );
    }

    #[test]
    fn highlight_zsh特殊パラメータ_dollar() {
        let pairs = span_styles("echo $$", "", "");
        let var = pairs.iter().find(|(t, _)| t == "$$");
        assert!(var.is_some(), "$$ が 1 つの変数スパンとして扱われていない");
        assert_eq!(var.unwrap().1, "fg=cyan");
    }

    #[test]
    fn highlight_redirection_gt() {
        let pairs = span_styles("echo foo > out", "", "");
        let redir = pairs.iter().find(|(t, _)| t == ">");
        assert!(redir.is_some());
        assert_eq!(redir.unwrap().1, "fg=magenta");
    }

    #[test]
    fn highlight_redirection_append() {
        let pairs = span_styles("echo foo >> out", "", "");
        let redir = pairs.iter().find(|(t, _)| t == ">>");
        assert!(redir.is_some());
        assert_eq!(redir.unwrap().1, "fg=magenta");
    }

    #[test]
    fn highlight_redirection_stderr() {
        let pairs = span_styles("echo foo 2> err", "", "");
        let redir = pairs.iter().find(|(t, _)| t == "2>");
        assert!(redir.is_some());
        assert_eq!(redir.unwrap().1, "fg=magenta");
    }

    #[test]
    fn highlight_redirection_stderr_append() {
        let pairs = span_styles("echo foo 2>> err", "", "");
        let redir = pairs.iter().find(|(t, _)| t == "2>>");
        assert!(redir.is_some());
        assert_eq!(redir.unwrap().1, "fg=magenta");
    }

    #[test]
    fn highlight_redirection_stdout_stderr() {
        let pairs = span_styles("echo foo &> out", "", "");
        let redir = pairs.iter().find(|(t, _)| t == "&>");
        assert!(redir.is_some());
        assert_eq!(redir.unwrap().1, "fg=magenta");
    }

    #[test]
    fn highlight_redirection_fd_dup_stdout() {
        // `2>&1` は `2>&` を 1 つの演算子トークンにする。
        // 旧バグでは `&` が分離し複製先 fd `1` を未知コマンド(赤)として誤分類していた。
        let pairs = span_styles("ls 2>&1", "", "");
        let redir = pairs.iter().find(|(t, _)| t == "2>&");
        assert!(
            redir.is_some(),
            "`2>&` が 1 トークンになっていない: {pairs:?}"
        );
        assert_eq!(redir.unwrap().1, "fg=magenta");
        assert!(
            !pairs.iter().any(|(t, _)| t == "&"),
            "`&` が独立した演算子トークンに分離している: {pairs:?}"
        );
        assert!(
            !pairs.iter().any(|(t, s)| t == "1" && s.contains("red")),
            "複製先 fd `1` が未知コマンド(赤)として誤分類されている: {pairs:?}"
        );
    }

    #[test]
    fn highlight_redirection_fd_dup_stdin() {
        // 入力側の fd 複製 `0<&3` も `0<&` を 1 トークンにする。
        let pairs = span_styles("cat 0<&3", "", "");
        let redir = pairs.iter().find(|(t, _)| t == "0<&");
        assert!(
            redir.is_some(),
            "`0<&` が 1 トークンになっていない: {pairs:?}"
        );
        assert_eq!(redir.unwrap().1, "fg=magenta");
    }

    #[test]
    fn highlight_option_short() {
        let pairs = span_styles("echo -n", "", "");
        let opt = pairs.iter().find(|(t, _)| t == "-n");
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().1, "fg=cyan");
    }

    #[test]
    fn highlight_option_long() {
        let pairs = span_styles("echo --all", "", "");
        let opt = pairs.iter().find(|(t, _)| t == "--all");
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().1, "fg=cyan");
    }

    #[test]
    fn highlight_alias() {
        let pairs = span_styles("ll", "ll\nla", "");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "ll");
        assert_eq!(pairs[0].1, "fg=green");
    }

    #[test]
    fn highlight_function() {
        let pairs = span_styles("myfunc", "", "myfunc\notherfunc");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "myfunc");
        assert_eq!(pairs[0].1, "fg=green");
    }

    #[test]
    fn highlight_reserved_word_if() {
        let pairs = span_styles("if", "", "");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "if");
        assert_eq!(pairs[0].1, "fg=yellow,bold");
    }

    #[test]
    fn highlight_reserved_word_then() {
        let pairs = span_styles("then", "", "");
        assert_eq!(pairs[0].1, "fg=yellow,bold");
    }

    #[test]
    fn highlight_reserved_word_fi() {
        let pairs = span_styles("fi", "", "");
        assert_eq!(pairs[0].1, "fg=yellow,bold");
    }

    #[test]
    fn highlight_reserved_word_for() {
        let pairs = span_styles("for", "", "");
        assert_eq!(pairs[0].1, "fg=yellow,bold");
    }

    #[test]
    fn highlight_sudo_prefix() {
        let pairs = span_styles("sudo", "", "");
        assert_eq!(pairs[0].1, "fg=green,bold");
    }

    #[test]
    fn highlight_piped_commands_full() {
        // "echo hello | cd /tmp" のハイライト検証
        let pairs = span_styles("echo hello | cd /tmp", "", "");

        // echo は組み込みコマンド
        assert_eq!(pairs[0].0, "echo");
        assert_eq!(pairs[0].1, "fg=green");

        // | は演算子
        let pipe = pairs.iter().find(|(t, _)| t == "|").unwrap();
        assert_eq!(pipe.1, "fg=magenta");

        // パイプ後の cd は組み込みコマンド
        let cd = pairs.iter().find(|(t, _)| t == "cd").unwrap();
        assert_eq!(cd.1, "fg=green");
    }

    #[test]
    fn highlight_multiple_operators() {
        let pairs = span_styles("echo a && echo b || cd", "", "");
        let ops: Vec<_> = pairs
            .iter()
            .filter(|(_, s)| s == "fg=magenta")
            .map(|(t, _)| t.as_str())
            .collect();
        assert!(ops.contains(&"&&"));
        assert!(ops.contains(&"||"));
    }

    // ---------------------------------------------------------------
    // パス存在判定: チルダ展開と存在しないパスのテスト
    // ---------------------------------------------------------------

    #[test]
    fn path_exists_tilde_expansion() {
        // "~" はホームディレクトリに展開され、存在するはず
        assert!(path_exists("~"));
    }

    #[test]
    fn path_exists_tilde_subdir() {
        // "~/.." はホームの親ディレクトリなので存在するはず
        assert!(path_exists("~/.."));
    }

    #[test]
    fn path_exists_nonexistent() {
        // 存在しないパスは false を返す
        assert!(!path_exists("/no_such_path_xyz_99999"));
    }

    #[test]
    fn path_exists_nonexistent_tilde() {
        // チルダ展開後に存在しないサブパス
        assert!(!path_exists("~/no_such_dir_xyz_99999"));
    }

    #[test]
    fn path_exists_current_dir() {
        // "." は常に存在する
        assert!(path_exists("."));
    }

    // ---------------------------------------------------------------
    // Glob パターンのハイライト（引数位置の *, ?, [）
    // ---------------------------------------------------------------

    #[test]
    fn highlight_glob_star() {
        // 引数位置の "*" は Glob としてハイライトされる
        let pairs = span_styles("echo *.txt", "", "");
        let glob = pairs.iter().find(|(t, _)| t == "*.txt");
        assert!(glob.is_some(), "*.txt のスパンが見つからない");
        assert_eq!(glob.unwrap().1, "fg=blue,bold");
    }

    #[test]
    fn highlight_glob_question() {
        // "?" を含む引数は Glob スタイル
        let pairs = span_styles("echo file?.log", "", "");
        let glob = pairs.iter().find(|(t, _)| t == "file?.log");
        assert!(glob.is_some(), "file?.log のスパンが見つからない");
        assert_eq!(glob.unwrap().1, "fg=blue,bold");
    }

    #[test]
    fn highlight_glob_bracket() {
        // "[" を含む引数は Glob スタイル
        let pairs = span_styles("echo [abc].txt", "", "");
        let glob = pairs.iter().find(|(t, _)| t == "[abc].txt");
        assert!(glob.is_some(), "[abc].txt のスパンが見つからない");
        assert_eq!(glob.unwrap().1, "fg=blue,bold");
    }

    #[test]
    fn highlight_glob_double_star() {
        // "**/*.rs" は "/" を含むためパス風として先に判定される
        let pairs = span_styles("echo **/*.rs", "", "");
        let glob = pairs.iter().find(|(t, _)| t == "**/*.rs");
        assert!(glob.is_some(), "**/*.rs のスパンが見つからない");
        assert_eq!(glob.unwrap().1, "fg=magenta");
    }

    #[test]
    fn highlight_glob_no_slash() {
        // スラッシュなしの複合 Glob は Glob スタイルになる
        let pairs = span_styles("echo **.rs", "", "");
        let glob = pairs.iter().find(|(t, _)| t == "**.rs");
        assert!(glob.is_some(), "**.rs のスパンが見つからない");
        assert_eq!(glob.unwrap().1, "fg=blue,bold");
    }

    // ---------------------------------------------------------------
    // バックスラッシュエスケープ: 単語トークン内の \ 処理
    // ---------------------------------------------------------------

    #[test]
    fn highlight_backslash_escape_in_word() {
        // バックスラッシュで空白をエスケープした単語は1トークンとして扱われる
        // "hello\ world" は通常の引数なのでスパンは生成されないが、
        // ";" などの演算子トークンが出ないことで1トークンと確認できる
        let pairs = span_styles("echo hello\\ world", "", "");
        // "world" が独立コマンドとして赤色にならないことを確認
        let world_cmd = pairs.iter().find(|(t, _)| t == "world");
        assert!(
            world_cmd.is_none(),
            "エスケープされた空白の後ろが別トークンになっている: {:?}",
            pairs
        );
        // echo のみがハイライトされる（hello\ world は通常引数でスパンなし）
        assert_eq!(
            pairs.len(),
            1,
            "echo 以外のスパンが生成されている: {:?}",
            pairs
        );
    }

    #[test]
    fn highlight_backslash_escape_special_char() {
        // バックスラッシュで ";" をエスケープすると演算子ではなく単語の一部になる
        let pairs = span_styles("echo foo\\;bar", "", "");
        // ";" が演算子スパンとして出ないことを確認
        let semi = pairs.iter().find(|(t, _)| t == ";");
        assert!(
            semi.is_none(),
            "エスケープされた ; が演算子として認識されている: {:?}",
            pairs
        );
        // "bar" がコマンド位置として扱われないことを確認
        let bar_cmd = pairs.iter().find(|(t, _)| t == "bar");
        assert!(
            bar_cmd.is_none(),
            "エスケープ後の bar が独立トークンになっている: {:?}",
            pairs
        );
    }

    #[test]
    fn highlight_コマンド内部のバックスラッシュを論理語から除く() {
        // zsh では `ec\ho` は builtin `echo` と同じ論理語になる。
        let pairs = span_styles("ec\\ho ok", "", "");
        let command = pairs
            .iter()
            .find(|(text, _)| text == "ec\\ho")
            .expect("コマンドスパンが存在するべき");
        assert_eq!(command.1, "fg=green");
    }

    #[test]
    fn highlight_エスケープされたglob文字をglob扱いしない() {
        // `\\*` はリテラルの `*` であり、zsh のパス名展開対象ではない。
        let pairs = span_styles("echo \\*.rs", "", "");
        assert!(
            !pairs
                .iter()
                .any(|(text, style)| { text == "\\*.rs" && style == "fg=blue,bold" }),
            "エスケープ済みの * を glob として分類している: {pairs:?}"
        );
    }

    // ---------------------------------------------------------------
    // バッファ末尾の孤立した "$"
    // ---------------------------------------------------------------

    #[test]
    fn highlight_lone_dollar_at_end() {
        // バッファ末尾の "$" は変数として扱われず、通常の単語トークンになる
        let pairs = span_styles("echo $", "", "");
        // "$" が cyan（変数色）にならないことを確認
        let dollar = pairs.iter().find(|(t, _)| t == "$");
        if let Some((_, style)) = dollar {
            assert_ne!(
                style, "fg=cyan",
                "孤立した $ が変数としてハイライトされてはいけない"
            );
        }
        // 変数スパンが存在しないことも確認
        let cyan_spans: Vec<_> = pairs
            .iter()
            .filter(|(t, s)| t == "$" && s == "fg=cyan")
            .collect();
        assert!(
            cyan_spans.is_empty(),
            "末尾の $ に変数スタイルが適用されている"
        );
    }

    #[test]
    fn highlight_dollar_mid_word() {
        // 単語の途中にある "$" + 変数名は変数としてハイライトされる
        let pairs = span_styles("echo $HOME rest", "", "");
        let var = pairs.iter().find(|(t, _)| t == "$HOME");
        assert!(var.is_some());
        assert_eq!(var.unwrap().1, "fg=cyan");
    }

    // ---------------------------------------------------------------
    // バッククォート (`) の扱い
    // ---------------------------------------------------------------

    #[test]
    fn highlight_backtick_not_special() {
        // バッククォートは特別扱いされず、通常の単語トークンに含まれる
        // 引数位置の `date` はスパンなし（通常引数）だが、
        // シングルクォートのように独立トークン分割されないことを確認
        let pairs = span_styles("echo `date`", "", "");
        // echo のみがスパンを持つ（`date` は通常引数でハイライトなし）
        assert_eq!(
            pairs.len(),
            1,
            "echo 以外のスパンが生成されている: {:?}",
            pairs
        );
        assert_eq!(pairs[0].0, "echo");
        // シングルクォートとして誤認されていないことを確認
        let sq = pairs.iter().find(|(_, s)| s == "fg=yellow");
        assert!(sq.is_none(), "バッククォートが文字列として扱われている");
    }

    // ---------------------------------------------------------------
    // ダブルクォート末尾バックスラッシュの境界テスト
    // ---------------------------------------------------------------

    #[test]
    fn highlight_double_quote_backslash_at_end() {
        // 未閉じダブルクォートの末尾バックスラッシュ: スパンの end がバッファ長を超えないこと
        let buf = r#"echo "hello\"#;
        let spans = highlight_buffer(buf, "", "");
        for s in &spans {
            assert!(
                s.end <= buf.len(),
                "スパン end ({}) がバッファ長 ({}) を超えている",
                s.end,
                buf.len()
            );
        }
        let pairs = span_styles(buf, "", "");
        // ダブルクォート部分が黄色で出る
        let dq = pairs.iter().find(|(_, s)| s == "fg=yellow");
        assert!(dq.is_some(), "ダブルクォート文字列が検出されていない");
    }

    #[test]
    fn highlight_double_quote_double_backslash_at_end() {
        // 末尾 \\ = エスケープされたバックスラッシュ
        let buf = r#"echo "test\\"#;
        let spans = highlight_buffer(buf, "", "");
        for s in &spans {
            assert!(
                s.end <= buf.len(),
                "スパン end ({}) がバッファ長 ({}) を超えている",
                s.end,
                buf.len()
            );
        }
    }

    #[test]
    fn highlight_backtick_in_command_position() {
        // コマンド位置のバッククォート付き単語はコマンドとして分類される
        let pairs = span_styles("`mycmd`", "", "");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "`mycmd`");
        // 未知コマンドとして赤色になるはず
        assert_eq!(pairs[0].1, "fg=red,bold");
    }

    // ---------------------------------------------------------------
    // コマンド分類: プレフィックスコマンド・各分類の直接テスト
    // ---------------------------------------------------------------

    #[test]
    fn classify_sudo_はプレフィックスコマンド() {
        let path_cmds = HashSet::new();
        let aliases = HashSet::new();
        let functions = HashSet::new();
        assert_eq!(
            classify_command("sudo", &aliases, &functions, &path_cmds),
            "fg=green,bold"
        );
    }

    #[test]
    fn classify_env_はプレフィックスコマンド() {
        let path_cmds = HashSet::new();
        let aliases = HashSet::new();
        let functions = HashSet::new();
        assert_eq!(
            classify_command("env", &aliases, &functions, &path_cmds),
            "fg=green,bold"
        );
    }

    #[test]
    fn classify_組み込みコマンドはgreen() {
        let path_cmds = HashSet::new();
        let aliases = HashSet::new();
        let functions = HashSet::new();
        assert_eq!(
            classify_command("echo", &aliases, &functions, &path_cmds),
            "fg=green"
        );
    }

    #[test]
    fn classify_予約語はyellow_bold() {
        let path_cmds = HashSet::new();
        let aliases = HashSet::new();
        let functions = HashSet::new();
        assert_eq!(
            classify_command("while", &aliases, &functions, &path_cmds),
            "fg=yellow,bold"
        );
    }

    #[test]
    fn classify_エイリアスはgreen() {
        let path_cmds = HashSet::new();
        let aliases = HashSet::from(["ll"]);
        let functions = HashSet::new();
        assert_eq!(
            classify_command("ll", &aliases, &functions, &path_cmds),
            "fg=green"
        );
    }

    #[test]
    fn classify_関数はgreen() {
        let path_cmds = HashSet::new();
        let aliases = HashSet::new();
        let functions = HashSet::from(["myfunc"]);
        assert_eq!(
            classify_command("myfunc", &aliases, &functions, &path_cmds),
            "fg=green"
        );
    }

    #[test]
    fn classify_pathコマンドはgreen() {
        let path_cmds = HashSet::from([OsString::from("rustc")]);
        let aliases = HashSet::new();
        let functions = HashSet::new();
        assert_eq!(
            classify_command("rustc", &aliases, &functions, &path_cmds),
            "fg=green"
        );
    }

    #[test]
    fn classify_不明コマンドはred_bold() {
        let path_cmds = HashSet::new();
        let aliases = HashSet::new();
        let functions = HashSet::new();
        assert_eq!(
            classify_command("no_such_cmd_xyz", &aliases, &functions, &path_cmds),
            "fg=red,bold"
        );
    }

    // ---------------------------------------------------------------
    // バッファ全体のハイライト: 複合的なケースの追加テスト
    // ---------------------------------------------------------------

    #[test]
    fn highlight_envプレフィックス後のコマンド() {
        let pairs = span_styles("env echo hello", "", "");
        assert_eq!(pairs[0].0, "env");
        assert_eq!(pairs[0].1, "fg=green,bold");
        // env の後の echo もコマンドとして分類される
        let echo = pairs.iter().find(|(t, _)| t == "echo");
        assert!(echo.is_some(), "env 後の echo がスパンに含まれていない");
        assert_eq!(
            echo.unwrap().1,
            "fg=green",
            "env 後の echo がコマンドとしてハイライトされていない"
        );
    }

    #[test]
    fn highlight_代入語の後もコマンド位置を維持する() {
        let pairs = span_styles("FOO=bar echo hello", "", "");
        let assignment = pairs.iter().find(|(t, _)| t == "FOO=bar");
        assert!(assignment.is_some(), "代入語がスパンに含まれていない");
        assert_eq!(assignment.unwrap().1, "fg=cyan");

        let echo = pairs.iter().find(|(t, _)| t == "echo");
        assert!(echo.is_some(), "代入語後の echo がスパンに含まれていない");
        assert_eq!(echo.unwrap().1, "fg=green");
    }

    #[test]
    fn highlight_env後の代入語の後もコマンド位置を維持する() {
        let pairs = span_styles("env FOO=bar echo hello", "", "");
        let assignment = pairs.iter().find(|(t, _)| t == "FOO=bar");
        assert!(
            assignment.is_some(),
            "env 後の代入語がスパンに含まれていない"
        );
        assert_eq!(assignment.unwrap().1, "fg=cyan");

        let echo = pairs.iter().find(|(t, _)| t == "echo");
        assert!(echo.is_some(), "代入語後の echo がスパンに含まれていない");
        assert_eq!(echo.unwrap().1, "fg=green");
    }

    #[test]
    fn highlight_sudoプレフィックス後のコマンド() {
        // sudo の後の cd はコマンドとして分類される
        let pairs = span_styles("sudo cd /tmp", "", "");
        assert_eq!(pairs[0].0, "sudo");
        assert_eq!(pairs[0].1, "fg=green,bold");
        let cd = pairs.iter().find(|(t, _)| t == "cd");
        assert!(cd.is_some(), "sudo 後の cd がスパンに含まれていない");
        assert_eq!(
            cd.unwrap().1,
            "fg=green",
            "sudo 後の cd がコマンドとしてハイライトされていない"
        );
    }

    #[test]
    fn highlight_プレフィックスチェーン() {
        // sudo env command ls のようにプレフィックスが連続する場合
        let pairs = span_styles("sudo env echo hello", "", "");
        assert_eq!(pairs[0].0, "sudo");
        assert_eq!(pairs[0].1, "fg=green,bold");
        let env = pairs.iter().find(|(t, _)| t == "env");
        assert!(env.is_some());
        assert_eq!(
            env.unwrap().1,
            "fg=green,bold",
            "sudo 後の env もプレフィックスとしてハイライトされるべき"
        );
        let echo = pairs.iter().find(|(t, _)| t == "echo");
        assert!(echo.is_some());
        assert_eq!(
            echo.unwrap().1,
            "fg=green",
            "env 後の echo がコマンドとしてハイライトされるべき"
        );
    }

    #[test]
    fn highlight_sudoプレフィックス後の不明コマンド() {
        // sudo の後の未知コマンドは赤色になる
        let pairs = span_styles("sudo zzz_no_such_cmd", "", "");
        let unknown = pairs.iter().find(|(t, _)| t == "zzz_no_such_cmd");
        assert!(unknown.is_some());
        assert_eq!(
            unknown.unwrap().1,
            "fg=red,bold",
            "sudo 後の未知コマンドが赤色でない"
        );
    }

    #[test]
    fn highlight_閉じられていないブレース変数() {
        // 閉じられていない ${VAR は末尾まで変数として扱われる
        let pairs = span_styles("echo ${UNCLOSED", "", "");
        let var = pairs.iter().find(|(t, _)| t.starts_with("${"));
        assert!(var.is_some());
        assert_eq!(var.unwrap().1, "fg=cyan");
    }

    #[test]
    fn highlight_閉じられていないシングルクォート() {
        let pairs = span_styles("echo 'unclosed", "", "");
        let sq = pairs.iter().find(|(_, s)| s == "fg=yellow");
        assert!(sq.is_some());
    }

    #[test]
    fn highlight_空白のみのバッファは空() {
        let spans = highlight_buffer("   ", "", "");
        assert!(spans.is_empty());
    }

    #[test]
    fn highlight_バックグラウンド演算子() {
        // "&" は演算子として扱われる
        let pairs = span_styles("echo &", "", "");
        let amp = pairs.iter().find(|(t, _)| t == "&");
        assert!(amp.is_some());
        assert_eq!(amp.unwrap().1, "fg=magenta");
    }

    // ---------------------------------------------------------------
    // プレフィックスコマンド判定のテスト
    // ---------------------------------------------------------------

    #[test]
    fn is_prefix_command_既知のプレフィックス() {
        assert!(is_prefix_command("sudo"));
        assert!(is_prefix_command("env"));
        assert!(is_prefix_command("command"));
        assert!(is_prefix_command("builtin"));
        assert!(is_prefix_command("exec"));
        assert!(is_prefix_command("noglob"));
        assert!(is_prefix_command("nocorrect"));
    }

    #[test]
    fn is_prefix_command_非プレフィックスコマンド() {
        assert!(!is_prefix_command("echo"));
        assert!(!is_prefix_command("ls"));
        assert!(!is_prefix_command("cd"));
        assert!(!is_prefix_command("git"));
        assert!(!is_prefix_command(""));
    }

    // ---------------------------------------------------------------
    // 閉じクォート・リダイレクトの追加テスト
    // ---------------------------------------------------------------

    #[test]
    fn highlight_閉じられていないダブルクォート() {
        let pairs = span_styles("echo \"unclosed", "", "");
        let dq = pairs.iter().find(|(_, s)| s == "fg=yellow");
        assert!(
            dq.is_some(),
            "閉じられていないダブルクォートが検出されるべき"
        );
    }

    #[test]
    fn highlight_リダイレクト入力() {
        let pairs = span_styles("cat < input.txt", "", "");
        let redir = pairs.iter().find(|(t, _)| t == "<");
        assert!(redir.is_some());
        assert_eq!(redir.unwrap().1, "fg=magenta");
    }

    #[test]
    fn highlight_複数行コマンドのセミコロン区切り() {
        // "echo a ; echo b ; echo c" の全 echo がコマンドとしてハイライト
        let pairs = span_styles("echo a ; echo b ; echo c", "", "");
        let echos: Vec<_> = pairs.iter().filter(|(t, _)| t == "echo").collect();
        assert_eq!(echos.len(), 3, "3つの echo がコマンドとして検出されるべき");
        for (_, style) in &echos {
            assert_eq!(style, "fg=green");
        }
    }

    #[test]
    fn highlight_改行後のコマンドをコマンド位置として扱う() {
        let pairs = span_styles("echo a\necho b", "", "");
        let echos: Vec<_> = pairs.iter().filter(|(t, _)| t == "echo").collect();
        assert_eq!(
            echos.len(),
            2,
            "改行後の echo もコマンドとして検出されるべき"
        );
        for (_, style) in &echos {
            assert_eq!(style, "fg=green");
        }
    }

    #[test]
    fn highlight_空のエイリアスと関数リスト() {
        // 空文字列のエイリアス・関数リストでパニックしない
        let pairs = span_styles("echo hello", "", "");
        assert!(!pairs.is_empty());
    }

    #[test]
    fn highlight_複数のエイリアスと関数() {
        let pairs = span_styles("ll", "ll\nla\ngrep", "");
        assert_eq!(pairs[0].1, "fg=green");
        let pairs = span_styles("myfn", "", "myfn\notherfn");
        assert_eq!(pairs[0].1, "fg=green");
    }

    // ---------------------------------------------------------------
    // マルチバイト文字: スパンが文字（コードポイント）単位であることの保証
    // ---------------------------------------------------------------

    #[test]
    fn highlight_マルチバイト文字を含むバッファでパニックしない() {
        // 日本語などのマルチバイト文字を含むコマンドラインでクラッシュしない。
        // 引数にUnicodeを含めても通常通りハイライトが返る。
        let buf = "echo こんにちは";
        let spans = highlight_buffer(buf, "", "");
        assert!(
            !spans.is_empty(),
            "マルチバイト文字でハイライトが空になっている"
        );
    }

    #[test]
    fn highlight_スパンは文字インデックス単位() {
        // zsh の `region_highlight` は文字（コードポイント）単位の位置を期待する。
        // Rust 側で chars() ベースに進めているため、Span::start/end も文字単位でなければならない。
        // ここではマルチバイト文字を含むパイプ演算子の位置が文字数と一致することを確認する。
        let buf = "あい | い";
        let spans = highlight_buffer(buf, "", "");
        let total_chars: usize = buf.chars().count();
        for s in &spans {
            assert!(
                s.end <= total_chars,
                "Span::end ({}) が文字数 ({}) を超えている: span={:?}",
                s.end,
                total_chars,
                s
            );
            assert!(
                s.start <= s.end,
                "Span::start ({}) > end ({}): span={:?}",
                s.start,
                s.end,
                s
            );
        }
        // パイプ演算子のスパンが見つかり、その start は文字単位で3 (`"あい "` の3文字後)
        let pipe = spans.iter().find(|s| s.style == "fg=magenta");
        assert!(pipe.is_some(), "パイプ演算子のスパンが見つからない");
        let pipe = pipe.unwrap();
        assert_eq!(
            pipe.start, 3,
            "パイプ位置が文字単位でない (start={}, expected=3)",
            pipe.start
        );
        assert_eq!(pipe.end, 4, "パイプ end が文字単位でない");
    }

    #[test]
    fn highlight_glob_ブラケットを含むバッファでパニックしない() {
        // shell/init.zsh で `${VAR#"$BUFFER"}` の quote を忘れると、
        // `[`/`]` を含むバッファで zsh パターンエラーになる回帰の Rust 側カバー。
        // Rust 側は単に Glob としてハイライトされ、スパンの境界が一貫していれば OK。
        let buf = "echo [abc] foo";
        let spans = highlight_buffer(buf, "", "");
        let total_chars: usize = buf.chars().count();
        for s in &spans {
            assert!(s.end <= total_chars);
            assert!(s.start <= s.end);
        }
        // `[abc]` は Glob スタイル
        let pairs = span_styles(buf, "", "");
        let glob = pairs.iter().find(|(t, _)| t == "[abc]");
        assert!(glob.is_some(), "[abc] のスパンが見つからない");
        assert_eq!(glob.unwrap().1, "fg=blue,bold");
    }

    // ── 区切り直後のクォート/変数がコマンドを消費するケース ─────────

    #[test]
    fn highlight_セミコロン後の変数に続く単語はコマンド扱いしない() {
        // `$HOME` がコマンド名として消費されるため、`echo` は引数になる。
        // after_pipe_or_semi の残留で `echo` が fg=green になる回帰を防ぐ。
        let pairs = span_styles("true; $HOME echo", "", "");
        let arg = pairs.iter().find(|(t, _)| t == "echo");
        assert!(arg.is_none(), "引数 echo にスパンが付いている: {pairs:?}");
    }

    #[test]
    fn highlight_セミコロン後のシングルクォートに続く単語はオプション扱い() {
        // `'ls'` がコマンド名として消費されるため、`-la` はオプションになる。
        let pairs = span_styles("true; 'ls' -la", "", "");
        let opt = pairs.iter().find(|(t, _)| t == "-la");
        assert!(opt.is_some(), "-la のスパンが見つからない");
        assert_eq!(opt.unwrap().1, "fg=cyan");
    }

    #[test]
    fn highlight_セミコロン後のダブルクォートに続く単語はオプション扱い() {
        let pairs = span_styles("true; \"ls\" -la", "", "");
        let opt = pairs.iter().find(|(t, _)| t == "-la");
        assert!(opt.is_some(), "-la のスパンが見つからない");
        assert_eq!(opt.unwrap().1, "fg=cyan");
    }

    #[test]
    fn highlight_改行後の変数に続く単語はコマンド扱いしない() {
        let pairs = span_styles("true\n$RUNNER arg", "", "");
        let arg = pairs.iter().find(|(t, _)| t == "arg");
        assert!(arg.is_none(), "引数 arg にスパンが付いている: {pairs:?}");
    }

    #[test]
    fn highlight_パイプ後の変数に続く単語はコマンド扱いしない() {
        let pairs = span_styles("ls | $PAGER file", "", "");
        let arg = pairs.iter().find(|(t, _)| t == "file");
        assert!(arg.is_none(), "引数 file にスパンが付いている: {pairs:?}");
    }

    #[test]
    fn highlight_セミコロン直後のリダイレクト先はコマンド扱いしない() {
        // `; > file` の `file` はリダイレクト先（引数）であり、新コマンドではない。
        // 旧バグでは `;` で after_pipe_or_semi=true、リダイレクト演算子が
        // is_command_position=false のみ落として after_pipe_or_semi が残り、
        // 213 行目の `is_command_position || after_pipe_or_semi` で `out` が
        // classify_command に渡されてコマンド扱いされてしまっていた（回帰防止）。
        let pairs = span_styles("ls ; > out rest", "", "");
        let out = pairs.iter().find(|(t, _)| t == "out");
        assert!(
            out.is_none(),
            "リダイレクト先 out にコマンドスパンが付いてはならない: {pairs:?}"
        );
        let rest = pairs.iter().find(|(t, _)| t == "rest");
        assert!(
            rest.is_none(),
            "リダイレクト先の後続単語 rest にコマンドスパンが付いてはならない: {pairs:?}"
        );
    }

    #[test]
    fn highlight_パイプ直後のリダイレクト先はコマンド扱いしない() {
        // `| > file` も `; > file` と同じく後続単語が新コマンドにならないこと。
        let pairs = span_styles("ls | > out rest", "", "");
        let rest = pairs.iter().find(|(t, _)| t == "rest");
        assert!(
            rest.is_none(),
            "パイプ直後のリダイレクト先後続 rest にスパンが付いてはならない: {pairs:?}"
        );
    }

    #[test]
    fn highlight_セミコロン直後のampリダイレクト後もコマンド扱いしない() {
        // `&>` リダイレクトでも同じ挙動になること。
        let pairs = span_styles("ls ; &> out rest", "", "");
        let rest = pairs.iter().find(|(t, _)| t == "rest");
        assert!(
            rest.is_none(),
            "&> リダイレクト先後続 rest にスパンが付いてはならない: {pairs:?}"
        );
    }

    #[test]
    fn highlight_セミコロン直後のfd複製リダイレクト後もコマンド扱いしない() {
        // `2>` 等の fd 指定リダイレクトでも同じ挙動になること。
        let pairs = span_styles("ls ; 2> err rest", "", "");
        let rest = pairs.iter().find(|(t, _)| t == "rest");
        assert!(
            rest.is_none(),
            "fd 指定リダイレクト先後続 rest にスパンが付いてはならない: {pairs:?}"
        );
    }

    // ---------------------------------------------------------------
    // クォート付き代入語・語結合 (バグ修正の回帰テスト)
    // ---------------------------------------------------------------

    /// (start, end, style) のスパンを検索するヘルパ
    fn find_span(spans: &[Span], start: usize, end: usize) -> Option<&Span> {
        spans.iter().find(|s| s.start == start && s.end == end)
    }

    #[test]
    fn highlight_ダブルクォート代入値の後もコマンド位置が続く() {
        // `FOO="bar baz" echo hi` — クォート値でコマンド位置を失わないこと
        let spans = highlight_buffer("FOO=\"bar baz\" echo hi", "", "");
        let assign = find_span(&spans, 0, 13).expect("代入語スパンが必要");
        assert_eq!(assign.style, "fg=cyan");
        let quote = find_span(&spans, 4, 13).expect("クォートセグメントスパンが必要");
        assert_eq!(quote.style, "fg=yellow");
        let cmd = find_span(&spans, 14, 18).expect("echo のスパンが必要");
        assert_eq!(cmd.style, "fg=green");
    }

    #[test]
    fn highlight_シングルクォート代入値の後もコマンド位置が続く() {
        let spans = highlight_buffer("FOO='bar baz' true", "", "");
        let cmd = find_span(&spans, 14, 18).expect("true のスパンが必要");
        assert_eq!(cmd.style, "fg=green");
    }

    #[test]
    fn highlight_クォート結合語はひとつのコマンドとして分類される() {
        // `ec'ho' hi` は zsh では echo として実行される
        let spans = highlight_buffer("ec'ho' hi", "", "");
        let cmd = find_span(&spans, 0, 6).expect("結合語全体のスパンが必要");
        assert_eq!(cmd.style, "fg=green");
        let quote = find_span(&spans, 2, 6).expect("クォートセグメントスパンが必要");
        assert_eq!(quote.style, "fg=yellow");
    }

    #[test]
    fn highlight_クォート付きオプションはシアン() {
        // `--file='a b'` は 1 語のオプション
        let spans = highlight_buffer("true --file='a b'", "", "");
        let opt = find_span(&spans, 5, 17).expect("オプションスパンが必要");
        assert_eq!(opt.style, "fg=cyan");
        let quote = find_span(&spans, 12, 17).expect("クォートセグメントスパンが必要");
        assert_eq!(quote.style, "fg=yellow");
    }

    // ---------------------------------------------------------------
    // プレフィックスコマンド直後のオプション
    // ---------------------------------------------------------------

    #[test]
    fn highlight_sudoの直後のオプションは未知コマンド扱いしない() {
        let spans = highlight_buffer("sudo -u root ls", "", "");
        let sudo = find_span(&spans, 0, 4).expect("sudo のスパンが必要");
        assert_eq!(sudo.style, "fg=green,bold");
        let opt = find_span(&spans, 5, 7).expect("-u のスパンが必要");
        assert_eq!(opt.style, "fg=cyan");
        assert!(
            !spans.iter().any(|s| s.style == "fg=red,bold"),
            "赤スパンが存在してはならない: {spans:?}"
        );
    }

    #[test]
    fn highlight_セミコロン直後のハイフン語は引き続き未知コマンド扱い() {
        // オプション救済はプレフィックスコマンド直後に限定する
        // (`-foo` 単体はコマンドとして実行されず command not found になるため)
        let spans = highlight_buffer("true; -foo", "", "");
        let word = find_span(&spans, 6, 10).expect("-foo のスパンが必要");
        assert_eq!(word.style, "fg=red,bold");
    }

    // ---------------------------------------------------------------
    // 予約語後のコマンド位置維持
    // ---------------------------------------------------------------

    #[test]
    fn highlight_if_then内のコマンドがハイライトされる() {
        let spans = highlight_buffer("if true; then echo hi; fi", "", "");
        assert_eq!(find_span(&spans, 0, 2).unwrap().style, "fg=yellow,bold"); // if
        assert_eq!(find_span(&spans, 3, 7).unwrap().style, "fg=green"); // true
        assert_eq!(find_span(&spans, 9, 13).unwrap().style, "fg=yellow,bold"); // then
        assert_eq!(find_span(&spans, 14, 18).unwrap().style, "fg=green"); // echo
        assert_eq!(find_span(&spans, 23, 25).unwrap().style, "fg=yellow,bold"); // fi
    }

    #[test]
    fn highlight_time_の後続はコマンド位置() {
        let spans = highlight_buffer("time true", "", "");
        assert_eq!(find_span(&spans, 0, 4).unwrap().style, "fg=yellow,bold");
        assert_eq!(find_span(&spans, 5, 9).unwrap().style, "fg=green");
    }

    #[test]
    fn highlight_while_の後続はコマンド位置() {
        let spans = highlight_buffer("while true; do echo x; done", "", "");
        assert_eq!(find_span(&spans, 6, 10).unwrap().style, "fg=green"); // true
        assert_eq!(find_span(&spans, 15, 19).unwrap().style, "fg=green"); // echo
    }

    // ---------------------------------------------------------------
    // 組み込み・予約語テーブルの補完
    // ---------------------------------------------------------------

    #[test]
    fn highlight_コロンとドットは組み込み() {
        let spans = highlight_buffer(": true", "", "");
        assert_eq!(find_span(&spans, 0, 1).unwrap().style, "fg=green");
        let spans = highlight_buffer(". ./script", "", "");
        assert_eq!(find_span(&spans, 0, 1).unwrap().style, "fg=green");
    }

    #[test]
    fn highlight_二重ブラケットと単一ブラケットは正当な構文() {
        let spans = highlight_buffer("[[ -f x ]]", "", "");
        assert_eq!(find_span(&spans, 0, 2).unwrap().style, "fg=yellow,bold");
        let spans = highlight_buffer("[ -f x ]", "", "");
        assert_eq!(find_span(&spans, 0, 1).unwrap().style, "fg=green");
    }

    #[test]
    fn highlight_ブレースと否定は予約語() {
        let spans = highlight_buffer("{ echo hi; }", "", "");
        assert_eq!(find_span(&spans, 0, 1).unwrap().style, "fg=yellow,bold"); // {
        assert_eq!(find_span(&spans, 2, 6).unwrap().style, "fg=green"); // echo (位置維持)
        assert_eq!(find_span(&spans, 11, 12).unwrap().style, "fg=yellow,bold"); // }
        let spans = highlight_buffer("! true", "", "");
        assert_eq!(find_span(&spans, 0, 1).unwrap().style, "fg=yellow,bold"); // !
        assert_eq!(find_span(&spans, 2, 6).unwrap().style, "fg=green"); // true (位置維持)
    }

    // ---------------------------------------------------------------
    // チルダ展開・CLOBBER・エイリアスバイパス
    // ---------------------------------------------------------------

    #[test]
    fn highlight_チルダ始まりの実在パスは実行可能パス扱い() {
        // `~/` はホームディレクトリとして必ず実在する
        let spans = highlight_buffer("~/ x", "", "");
        let cmd = find_span(&spans, 0, 2).expect("チルダパスのスパンが必要");
        assert_eq!(cmd.style, "fg=green,underline");
    }

    #[test]
    fn highlight_clobberリダイレクトの後続をコマンド扱いしない() {
        // `>|` の `|` はパイプではなく CLOBBER 演算子の一部
        let spans = highlight_buffer("echo hi >| out", "", "");
        let op = find_span(&spans, 8, 10).expect(">| は 1 トークン");
        assert_eq!(op.style, "fg=magenta");
        assert!(
            !spans.iter().any(|s| s.start == 11),
            "リダイレクト先 out にスパンが付いてはならない: {spans:?}"
        );
    }

    #[test]
    fn highlight_append_clobberも1トークン() {
        let spans = highlight_buffer("true >>| f", "", "");
        let op = find_span(&spans, 5, 8).expect(">>| は 1 トークン");
        assert_eq!(op.style, "fg=magenta");
    }

    #[test]
    fn highlight_fd付きclobberも1トークン() {
        let spans = highlight_buffer("true 2>| f", "", "");
        let op = find_span(&spans, 5, 8).expect("2>| は 1 トークン");
        assert_eq!(op.style, "fg=magenta");
    }

    #[test]
    fn highlight_バックスラッシュ付きコマンドはエイリアスバイパス() {
        // `\echo` は zsh でエイリアス展開を抑止して echo を実行する常用イディオム
        let spans = highlight_buffer("\\echo hi", "", "");
        let cmd = find_span(&spans, 0, 5).expect("\\echo のスパンが必要");
        assert_eq!(cmd.style, "fg=green");
    }

    #[test]
    fn highlight_バックスラッシュ付き未知コマンドは赤() {
        let spans = highlight_buffer("\\zt_no_such_cmd_x", "", "");
        assert_eq!(spans[0].style, "fg=red,bold");
    }

    #[test]
    fn highlight_バックスラッシュ付き予約語は予約語として扱わない() {
        // `\if` は予約語解釈が無効化されるため通常のコマンド検索になる
        let spans = highlight_buffer("\\if x", "", "");
        let cmd = find_span(&spans, 0, 3).expect("\\if のスパンが必要");
        assert_ne!(cmd.style, "fg=yellow,bold");
    }

    #[cfg(unix)]
    #[test]
    fn build_path_cache_from_シンボリックリンクの実行ファイルも登録する() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("zt-real");
        std::fs::write(&real, "").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&real, dir.path().join("zt-link")).unwrap();
        std::os::unix::fs::symlink(
            dir.path().join("zt-missing"),
            dir.path().join("zt-dangling"),
        )
        .unwrap();

        let set = build_path_cache_from(dir.path().to_str().unwrap());

        assert!(
            set.contains(OsString::from("zt-real").as_os_str()),
            "実体の実行ファイルが登録されるべき"
        );
        assert!(
            set.contains(OsString::from("zt-link").as_os_str()),
            "symlink された実行ファイル (Homebrew 形式) が登録されるべき"
        );
        assert!(
            !set.contains(OsString::from("zt-dangling").as_os_str()),
            "壊れた symlink は登録しない"
        );
    }

    // ---------------------------------------------------------------
    // 未対応構文の挙動ピン留め (パニック・範囲逸脱がないこと)
    // ---------------------------------------------------------------

    #[test]
    fn highlight_コマンド置換とプロセス置換はスパン範囲内に収まる() {
        for buf in [
            "echo $(ls -la)",
            "diff <(ls) <(ls -a)",
            "cat << EOF\nhello world\nEOF",
        ] {
            let spans = highlight_buffer(buf, "", "");
            let total = buf.chars().count();
            for s in &spans {
                assert!(
                    s.start <= s.end && s.end <= total,
                    "範囲逸脱: {s:?} in {buf:?}"
                );
            }
        }
    }
}
