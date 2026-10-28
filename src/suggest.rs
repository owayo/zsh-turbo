use crate::{project_tasks, task_usage};
use std::collections::{HashMap, hash_map::Entry};
use std::path::{Path, PathBuf};

/// サジェストの検索戦略
pub enum Strategy {
    Prefix,
    Substring,
    Fuzzy,
}

impl Strategy {
    pub fn from_str(s: &str) -> Self {
        match s {
            "substring" => Self::Substring,
            "fuzzy" => Self::Fuzzy,
            _ => Self::Prefix,
        }
    }
}

/// metafy の復元結果を同じバッファへ詰め、履歴全体の複製を避ける。
fn unmetafy(mut bytes: Vec<u8>) -> Vec<u8> {
    let Some(first) = memchr::memchr(0x83, &bytes) else {
        return bytes;
    };
    let (mut read, mut write) = (first, first);
    while read < bytes.len() {
        let b = bytes[read];
        read += 1;
        bytes[write] = if b == 0x83 {
            let Some(&next) = bytes.get(read) else {
                break;
            };
            read += 1;
            next ^ 0x20
        } else {
            b
        };
        write += 1;
    }
    bytes.truncate(write);
    bytes
}

/// zsh が追記中の、改行で終わらない最終行を捨てる。
fn drop_partial_tail(mut bytes: Vec<u8>) -> Vec<u8> {
    if !bytes.ends_with(b"\n") {
        let end = memchr::memrchr(b'\n', &bytes).map_or(0, |i| i + 1);
        bytes.truncate(end);
    }
    bytes
}

/// 全履歴を同じ基準で順位付けするため、末尾だけでなく全体を読む。
fn read_history(path: &Path) -> Option<Vec<u8>> {
    Some(drop_partial_tail(unmetafy(std::fs::read(path).ok()?)))
}

/// zsh は複数行コマンドの内部改行を `\`+LF で保存する。
fn continues_to_next_line(line: &[u8]) -> bool {
    line.ends_with(b"\\")
}

/// 履歴の 1 行からコマンドを取り出す。zsh は `\` (と空白) で終わるコマンドに空白を 1 つ
/// 足して書くため、その空白だけを外して `touch file\ ` の末尾の空白を残す。
/// 外すのは ASCII の空白だけにする。CR などの制御文字は残して候補の除外に任せ、
/// 全角空白も zsh と同じくコマンドの一部として扱う。
fn history_command(line: &str) -> &str {
    let command = parse_history_line(line).trim_start_matches(' ');
    let body = command.trim_end_matches(' ');
    if body.len() < command.len() && body.ends_with('\\') {
        &command[..command.len() - 1]
    } else {
        body
    }
}

/// 履歴テキストを新しい順に走査し、複数行エントリ (継続行) を除いた
/// 単独行の履歴コマンドだけを返す。複数行エントリは改行を含み、
/// 1 行プロトコル (zsh 側の `read -r`) で運べないため候補にしない。
/// zsh の readhistline と同じく LF だけで分ける (`lines()` は行末の CR も削り、
/// `\`+CR の行を継続行と取り違える)。
fn history_candidates_rev(text: &[u8]) -> impl Iterator<Item = &str> {
    let mut remaining = text.strip_suffix(b"\n").unwrap_or(text);
    std::iter::from_fn(move || {
        while !remaining.is_empty() {
            let separator = memchr::memrchr(b'\n', remaining);
            // ファイル順で 1 つ前の物理行が `\` 終端なら、この行は継続部分
            let prev_continues = separator.is_some_and(|i| i > 0 && remaining[i - 1] == b'\\');
            let line = &remaining[separator.map_or(0, |i| i + 1)..];
            remaining = &remaining[..separator.unwrap_or(0)];
            if prev_continues || continues_to_next_line(line) {
                continue;
            }
            // 不正な UTF-8 を別の文字列に置き換えて採用しない。
            let Ok(line) = std::str::from_utf8(line) else {
                continue;
            };
            return Some(history_command(line));
        }
        None
    })
}

/// 指定された戦略でサジェストを 1 件取得する。
/// カレントディレクトリのタスクがあれば、一覧で最初に選ぶタスクを返す。
/// なければ履歴から prefix → substring → fuzzy の順でフォールバックする。
/// 同程度の一致では使用頻度と新しさを優先する。
pub fn get_suggestion(
    query: &str,
    history_file: Option<&str>,
    strategy: &Strategy,
    task_usage: Option<&Path>,
) -> Option<String> {
    if query.is_empty() {
        return None;
    }

    let tasks = project_tasks::labeled_candidates(query, project_tasks::LIST_LIMIT);
    if !tasks.is_empty() {
        let index = preferred_task(query, &tasks, history_file, task_usage).unwrap_or(0);
        return tasks.into_iter().nth(index).map(|(_, command)| command);
    }

    get_history_suggestion(query, history_file, strategy)
}

/// タスク一覧 (`project_tasks::labeled_candidates` の結果) で最初に選ぶ位置。
/// このディレクトリでの利用記録と、履歴全体での呼び出しから決める (`task_usage::preferred`)
pub fn preferred_task(
    query: &str,
    tasks: &[(String, String)],
    history_file: Option<&str>,
    task_usage: Option<&Path>,
) -> Option<usize> {
    // 候補が 1 つなら選ぶまでもないので、記録と履歴を読まずに済ませる
    if tasks.len() < 2 {
        return None;
    }
    let command = project_tasks::command_of(query);
    let local = match (task_usage, std::env::current_dir()) {
        (Some(path), Ok(cwd)) => task_usage::usage(path, &cwd, command, task_usage::now()),
        _ => HashMap::new(),
    };
    let history = resolve_history_path(history_file)
        .and_then(|path| read_history(&path))
        .map(|text| task_calls(command, &text))
        .unwrap_or_default();
    task_usage::preferred(
        tasks.iter().map(|(name, _)| name.as_str()),
        &local,
        &history,
    )
}

/// 履歴全体から `command` のタスクの呼び出しをタスク名ごとに数える。
/// 呼び出しの判定は利用記録と同じ規則 (`project_tasks::invocation`) を使う。
fn task_calls(command: &str, text: &[u8]) -> HashMap<String, task_usage::HistoryUse> {
    let mut calls = HashMap::<String, task_usage::HistoryUse>::new();
    for (newest, line) in history_candidates_rev(text).enumerate() {
        if !line.starts_with(command) {
            continue;
        }
        let Some((called, name)) = project_tasks::invocation(line) else {
            continue;
        };
        if called != command {
            continue;
        }
        calls
            .entry(name.to_owned())
            .and_modify(|call| call.count += 1)
            .or_insert(task_usage::HistoryUse { count: 1, newest });
    }
    calls
}

pub fn get_history_suggestion(
    query: &str,
    history_file: Option<&str>,
    strategy: &Strategy,
) -> Option<String> {
    if query.is_empty() {
        return None;
    }

    let history_path = resolve_history_path(history_file)?;

    let text = read_history(&history_path)?;
    search_text(query, &text, strategy)
}

fn search_text(query: &str, text: &[u8], strategy: &Strategy) -> Option<String> {
    ranked_candidates(query, text, strategy, 1)
        .into_iter()
        .next()
}

fn ranked_candidates(query: &str, text: &[u8], strategy: &Strategy, max: usize) -> Vec<String> {
    if max == 0 || query.chars().any(char::is_control) {
        return Vec::new();
    }
    let mut matches = HashMap::<&str, ((u8, usize), usize, usize)>::new();
    let query_chars = query.chars().count();
    for (recency, cmd) in history_candidates_rev(text).enumerate() {
        if cmd.is_empty() || cmd == query {
            continue;
        }
        let prefix = cmd.starts_with(query);
        if !prefix && matches!(strategy, Strategy::Prefix) {
            continue;
        }
        let slot = match matches.entry(cmd) {
            Entry::Occupied(mut seen) => {
                let frequency = &mut seen.get_mut().1;
                *frequency = frequency.saturating_add(1);
                continue;
            }
            Entry::Vacant(slot) => slot,
        };
        let quality = if prefix {
            (0, 0)
        } else if let Some(at) = cmd.find(query) {
            (1, cmd[..at].chars().count())
        } else if matches!(strategy, Strategy::Fuzzy) && fuzzy_match(query, cmd) {
            (2, cmd.chars().count().saturating_sub(query_chars))
        } else {
            continue;
        };
        if cmd.chars().any(char::is_control) {
            continue;
        }
        slot.insert((quality, 1, recency));
    }
    let rank = |(_, (quality, frequency, recency)): &(&str, ((u8, usize), usize, usize))| {
        (*quality, std::cmp::Reverse(*frequency), *recency)
    };
    if max == 1 {
        return matches
            .into_iter()
            .min_by_key(rank)
            .map(|(cmd, _)| vec![cmd.to_owned()])
            .unwrap_or_default();
    }
    let mut matches: Vec<_> = matches.into_iter().collect();
    if matches.len() > max {
        matches.select_nth_unstable_by_key(max, rank);
        matches.truncate(max);
    }
    matches.sort_unstable_by_key(rank);
    matches
        .into_iter()
        .take(max)
        .map(|(cmd, _)| cmd.to_owned())
        .collect()
}

pub fn get_completions(prefix: &str, history_file: Option<&str>, max: usize) -> Vec<String> {
    get_completions_with_strategy(prefix, history_file, &Strategy::Prefix, max)
}

pub fn get_completions_with_strategy(
    query: &str,
    history_file: Option<&str>,
    strategy: &Strategy,
    max: usize,
) -> Vec<String> {
    if max == 0 || (query.is_empty() && !matches!(strategy, Strategy::Fuzzy)) {
        return Vec::new();
    }
    let mut project = project_tasks::candidates(query, max);
    if project.len() == max {
        return project;
    }
    let Some(text) = resolve_history_path(history_file).and_then(|path| read_history(&path)) else {
        return project;
    };
    for candidate in ranked_candidates(query, &text, strategy, max) {
        if !project.contains(&candidate) {
            project.push(candidate);
            if project.len() == max {
                break;
            }
        }
    }
    project
}

/// ファジーマッチ: クエリの全文字がターゲット内に順序通りに出現するか判定。
fn fuzzy_match(query: &str, target: &str) -> bool {
    let mut target_chars = target.chars();
    for qc in query.chars() {
        let qc_lower = qc.to_ascii_lowercase();
        loop {
            match target_chars.next() {
                Some(tc) if tc.to_ascii_lowercase() == qc_lower => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

/// 検索する履歴ファイル。呼び出し側 (main の `resolve_history_file`) が決めたパスだけを使い、
/// 指定が無ければ履歴を使わない (実行場所のない共通の履歴へ暗黙に切り替えない)。
fn resolve_history_path(history_file: Option<&str>) -> Option<PathBuf> {
    history_file.filter(|s| !s.is_empty()).map(PathBuf::from)
}

/// zsh 拡張履歴形式をパース: ": <timestamp>:<duration>;<command>"
/// `<timestamp>` と `<duration>` は数字のみで構成される必要がある。
/// 形式に合致しない場合は元の行をそのまま返す（例: `: echo a;git` のような通常履歴）。
fn parse_history_line(line: &str) -> &str {
    let Some(rest) = line.strip_prefix(": ") else {
        // 非拡張形式では `:` で始まるコマンドを `\:` と書く
        return line
            .strip_prefix('\\')
            .filter(|rest| rest.starts_with(':'))
            .unwrap_or(line);
    };
    let Some(colon_idx) = rest.find(':') else {
        return line;
    };
    let timestamp = &rest[..colon_idx];
    if timestamp.is_empty() || !timestamp.bytes().all(|b| b.is_ascii_digit()) {
        return line;
    }
    let after_colon = &rest[colon_idx + 1..];
    let Some(semi_idx) = after_colon.find(';') else {
        return line;
    };
    let duration = &after_colon[..semi_idx];
    if duration.is_empty() || !duration.bytes().all(|b| b.is_ascii_digit()) {
        return line;
    }
    &after_colon[semi_idx + 1..]
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------
    // 履歴行パースのテスト
    // -------------------------------------------------------

    #[test]
    fn parse_history_line_extended_format() {
        assert_eq!(parse_history_line(": 1234567890:0;ls -la"), "ls -la");
    }

    #[test]
    fn parse_history_line_extended_format_with_duration() {
        assert_eq!(
            parse_history_line(": 1700000000:123;git push origin main"),
            "git push origin main"
        );
    }

    #[test]
    fn parse_history_line_command_containing_semicolon() {
        // 最初のセミコロンのみがデリミタ。残りはコマンドの一部。
        assert_eq!(
            parse_history_line(": 1234567890:0;echo hello; echo world"),
            "echo hello; echo world"
        );
    }

    #[test]
    fn parse_history_line_regular_line_passes_through() {
        assert_eq!(parse_history_line("echo hello"), "echo hello");
    }

    #[test]
    fn parse_history_line_empty_string() {
        assert_eq!(parse_history_line(""), "");
    }

    #[test]
    fn parse_history_line_colon_space_but_no_semicolon() {
        // ": " で始まるがセミコロンなし — 通常の行として扱われる。
        assert_eq!(
            parse_history_line(": no_semicolon_here"),
            ": no_semicolon_here"
        );
    }

    #[test]
    fn parse_history_line_only_prefix() {
        // セミコロン直後が空のミニマルな拡張形式。
        assert_eq!(parse_history_line(": 0:0;"), "");
    }

    #[test]
    fn parse_history_line_does_not_match_without_space_after_colon() {
        // ": " プレフィックスがないため、そのまま返される。
        let input = ":1234567890:0;cmd";
        assert_eq!(parse_history_line(input), input);
    }

    #[test]
    fn parse_history_line_single_char_command() {
        assert_eq!(parse_history_line(": 9:0;x"), "x");
    }

    // -------------------------------------------------------
    // あいまい一致のテスト
    // -------------------------------------------------------

    #[test]
    fn fuzzy_match_basic_match() {
        assert!(fuzzy_match("gp", "git push"));
    }

    #[test]
    fn fuzzy_match_exact_match() {
        assert!(fuzzy_match("ls", "ls"));
    }

    #[test]
    fn fuzzy_match_case_insensitive() {
        assert!(fuzzy_match("GP", "git push"));
    }

    #[test]
    fn fuzzy_match_case_insensitive_target() {
        assert!(fuzzy_match("gp", "Git Push"));
    }

    #[test]
    fn fuzzy_match_empty_query_matches_anything() {
        assert!(fuzzy_match("", "anything"));
    }

    #[test]
    fn fuzzy_match_empty_query_and_empty_target() {
        assert!(fuzzy_match("", ""));
    }

    #[test]
    fn fuzzy_match_non_empty_query_empty_target() {
        assert!(!fuzzy_match("a", ""));
    }

    #[test]
    fn fuzzy_match_single_char_present() {
        assert!(fuzzy_match("g", "git"));
    }

    #[test]
    fn fuzzy_match_single_char_absent() {
        assert!(!fuzzy_match("z", "git"));
    }

    #[test]
    fn fuzzy_match_all_chars_present_but_wrong_order() {
        // "ba" は 'b' が 'a' より先に必要だが、"abc" では 'a' が 'b' より前にある。
        assert!(!fuzzy_match("ba", "abc"));
    }

    #[test]
    fn fuzzy_match_repeated_chars_in_query() {
        assert!(fuzzy_match("oo", "foobar"));
    }

    #[test]
    fn fuzzy_match_repeated_chars_insufficient_in_target() {
        assert!(!fuzzy_match("ooo", "foobar"));
    }

    #[test]
    fn fuzzy_match_query_longer_than_target() {
        assert!(!fuzzy_match("abcdef", "abc"));
    }

    #[test]
    fn fuzzy_match_scattered_chars() {
        assert!(fuzzy_match("dkr", "docker run"));
    }

    #[test]
    fn fuzzy_match_no_match() {
        assert!(!fuzzy_match("xyz", "docker run"));
    }

    // -------------------------------------------------------
    // 戦略文字列パースのテスト
    // -------------------------------------------------------

    #[test]
    fn strategy_from_str_prefix() {
        assert!(matches!(Strategy::from_str("prefix"), Strategy::Prefix));
    }

    #[test]
    fn strategy_from_str_substring() {
        assert!(matches!(
            Strategy::from_str("substring"),
            Strategy::Substring
        ));
    }

    #[test]
    fn strategy_from_str_fuzzy() {
        assert!(matches!(Strategy::from_str("fuzzy"), Strategy::Fuzzy));
    }

    #[test]
    fn strategy_from_str_unknown_defaults_to_prefix() {
        assert!(matches!(Strategy::from_str("unknown"), Strategy::Prefix));
    }

    #[test]
    fn strategy_from_str_empty_defaults_to_prefix() {
        assert!(matches!(Strategy::from_str(""), Strategy::Prefix));
    }

    #[test]
    fn strategy_from_str_case_sensitive() {
        // 大文字の "Fuzzy" は認識されず、Prefix にフォールバックする。
        assert!(matches!(Strategy::from_str("Fuzzy"), Strategy::Prefix));
    }

    #[test]
    fn strategy_from_str_with_whitespace() {
        // 先頭スペースありの " fuzzy" はマッチしない
        assert!(matches!(Strategy::from_str(" fuzzy"), Strategy::Prefix));
    }

    // -------------------------------------------------------
    // 候補検索のテスト
    // -------------------------------------------------------

    #[test]
    fn search_text_prefix戦略で先頭一致を返す() {
        let text = "git status\ngit push\nls -la";
        let result = search_text("git", text.as_bytes(), &Strategy::Prefix);
        assert!(result.is_some());
        assert!(result.unwrap().starts_with("git"));
    }

    #[test]
    fn search_text_完全一致はスキップする() {
        let text = "git\ngit push";
        let result = search_text("git", text.as_bytes(), &Strategy::Prefix);
        // "git" と完全一致する行はスキップし、"git push" を返す
        assert_eq!(result, Some("git push".to_string()));
    }

    #[test]
    fn search_text_マッチなしでnone() {
        let text = "ls -la\ncd /tmp";
        let result = search_text("git", text.as_bytes(), &Strategy::Prefix);
        assert!(result.is_none());
    }

    #[test]
    fn search_text_substring戦略で部分一致を返す() {
        let text = "docker run nginx\nkubectl get pods";
        // "nginx" は prefix マッチしないが substring マッチする
        let result = search_text("nginx", text.as_bytes(), &Strategy::Substring);
        assert_eq!(result, Some("docker run nginx".to_string()));
    }

    #[test]
    fn search_text_fuzzy戦略でファジーマッチを返す() {
        let text = "git push origin main";
        let result = search_text("gpom", text.as_bytes(), &Strategy::Fuzzy);
        assert_eq!(result, Some("git push origin main".to_string()));
    }

    #[test]
    fn search_text_空テキストでnone() {
        let result = search_text("git", b"", &Strategy::Prefix);
        assert!(result.is_none());
    }

    // -------------------------------------------------------
    // 履歴パス解決のテスト
    // -------------------------------------------------------

    #[test]
    fn resolve_history_path_明示的パスを優先() {
        let path = resolve_history_path(Some("/tmp/test_history"));
        assert_eq!(path, Some(PathBuf::from("/tmp/test_history")));
    }

    #[test]
    fn resolve_history_path_指定が無ければ共通の履歴へ切り替えない() {
        assert_eq!(resolve_history_path(None), None);
    }

    // -------------------------------------------------------
    // 履歴読み込みのテスト
    // -------------------------------------------------------

    #[test]
    fn read_history_空ファイルは空の履歴() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty_history");
        std::fs::write(&path, "").unwrap();
        assert_eq!(read_history(&path), Some(Vec::new()));
    }

    #[test]
    fn read_history_小さいファイルを読める() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small_history");
        std::fs::write(&path, "ls\ncd /tmp\ngit status\n").unwrap();
        let result = read_history(&path);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.windows(10).any(|s| s == b"git status"));
    }

    #[test]
    fn read_history_ファイル全体を読める() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("full_history");
        std::fs::write(&path, "first\nlast\n").unwrap();
        let result = read_history(&path);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.starts_with(b"first\n"));
        assert!(content.ends_with(b"last\n"));
    }

    #[test]
    fn read_history_存在しないファイルはnone() {
        let path = PathBuf::from("/tmp/nonexistent_history_xyz_999");
        assert!(read_history(&path).is_none());
    }

    // -------------------------------------------------------
    // サジェスト取得の統合テスト
    // -------------------------------------------------------

    #[test]
    fn get_suggestion_空クエリはnone() {
        assert!(get_suggestion("", None, &Strategy::Prefix, None).is_none());
    }

    #[test]
    fn get_suggestion_履歴ファイルから候補を返す() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, "git status\ngit push\nls -la\n").unwrap();
        let result = get_suggestion("git", Some(path.to_str().unwrap()), &Strategy::Prefix, None);
        assert!(result.is_some());
        assert!(result.unwrap().starts_with("git"));
    }

    // -------------------------------------------------------
    // 補完候補取得の統合テスト
    // -------------------------------------------------------

    #[test]
    fn get_completions_空prefixは空vec() {
        assert!(get_completions("", None, 10).is_empty());
    }

    #[test]
    fn get_completions_重複を除外する() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, "git push\ngit push\ngit push\n").unwrap();
        let results = get_completions("git", Some(path.to_str().unwrap()), 10);
        assert_eq!(results.len(), 1, "重複が除外されていない");
    }

    #[test]
    fn get_completions_max制限を守る() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, "git push\ngit pull\ngit status\ngit log\n").unwrap();
        let results = get_completions("git", Some(path.to_str().unwrap()), 2);
        assert!(results.len() <= 2, "max を超えている: {}", results.len());
    }

    #[test]
    fn get_completions_max_zero_returns_empty() {
        // max=0 のときは空 Vec を返す（max 制約の境界）
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, "git push\ngit pull\n").unwrap();
        let results = get_completions("git", Some(path.to_str().unwrap()), 0);
        assert!(
            results.is_empty(),
            "max=0 で結果が返っている: {:?}",
            results
        );
    }

    #[test]
    fn read_history_長い行も途中から切らず全体を読む() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        let content = format!("long-{}\nvalid-cmd\n", "x".repeat(80_000));
        std::fs::write(&path, &content).unwrap();
        assert_eq!(read_history(&path).unwrap(), content.as_bytes());
    }

    // -------------------------------------------------------
    // 補完候補取得: 大きい履歴の全体検索
    // -------------------------------------------------------

    #[test]
    fn get_completions_大きい履歴の先頭も候補にする() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        // 64 KiB より大きいファイルを作成し、先頭に一意なコマンドを配置
        let unique_cmd = "git unique-first-command";
        let mut content = format!("{unique_cmd}\n");
        // 64KB を超えるために行を追加
        let filler = "git filler-command-xxxxxxxxxxxxxxxxxxxxxxxxxx\n";
        while content.len() < 70_000 {
            content.push_str(filler);
        }
        std::fs::write(&path, &content).unwrap();
        // max を大きくして全候補を取得
        let results = get_completions("git", Some(path.to_str().unwrap()), 100);
        // 先頭のコマンドも取得できる
        assert!(
            results.iter().any(|r| r == unique_cmd),
            "先頭のコマンドが取得されていない: {:?}",
            results
        );
    }

    // -------------------------------------------------------
    // サジェスト取得: 各戦略のフォールバック動作
    // -------------------------------------------------------

    #[test]
    fn get_suggestion_substring戦略でprefixマッチも返す() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, "git push\n").unwrap();
        let result = get_suggestion(
            "git",
            Some(path.to_str().unwrap()),
            &Strategy::Substring,
            None,
        );
        // prefix マッチが先に試されるため "git push" が返る
        assert_eq!(result, Some("git push".to_string()));
    }

    #[test]
    fn get_suggestion_fuzzy戦略でsubstringマッチも返す() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, "docker run nginx\n").unwrap();
        let result = get_suggestion(
            "nginx",
            Some(path.to_str().unwrap()),
            &Strategy::Fuzzy,
            None,
        );
        // substring マッチにより "docker run nginx" が返る
        assert_eq!(result, Some("docker run nginx".to_string()));
    }

    #[test]
    fn get_suggestion_存在しないファイルはnone() {
        let result = get_suggestion(
            "git",
            Some("/tmp/nonexistent_history_xyz_999"),
            &Strategy::Prefix,
            None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn get_completions_存在しないファイルは空vec() {
        let results = get_completions("git", Some("/tmp/nonexistent_history_xyz_999"), 10);
        assert!(results.is_empty());
    }

    // -------------------------------------------------------
    // 履歴行パース: 追加の境界テスト
    // -------------------------------------------------------

    #[test]
    fn parse_history_line_複数セミコロンは最初のみがデリミタ() {
        // ": 0:0;a;b;c" のとき "a;b;c" が返る
        assert_eq!(parse_history_line(": 0:0;a;b;c"), "a;b;c");
    }

    #[test]
    fn parse_history_line_空白付きコマンド() {
        assert_eq!(
            parse_history_line(": 0:0;  leading spaces"),
            "  leading spaces"
        );
    }

    #[test]
    fn parse_history_line_数字でないtimestampは通常履歴扱い() {
        // ": echo a;git status" のような通常履歴は拡張形式と誤認しない
        let input = ": echo a;git status";
        assert_eq!(parse_history_line(input), input);
    }

    #[test]
    fn parse_history_line_数字でないdurationは通常履歴扱い() {
        // duration 部が非数字なら拡張形式扱いしない
        let input = ": 1700000000:abc;cmd";
        assert_eq!(parse_history_line(input), input);
    }

    #[test]
    fn parse_history_line_2つ目のコロンが無いと通常履歴扱い() {
        let input = ": 1700000000;cmd";
        assert_eq!(parse_history_line(input), input);
    }

    #[test]
    fn parse_history_line_timestamp空は通常履歴扱い() {
        let input = ": :0;cmd";
        assert_eq!(parse_history_line(input), input);
    }

    // -------------------------------------------------------
    // マルチバイト文字を含むケースの境界テスト
    // -------------------------------------------------------

    #[test]
    fn search_text_マルチバイト文字を含むクエリ() {
        let text = "echo こんにちは\necho さようなら";
        let result = search_text("echo こ", text.as_bytes(), &Strategy::Prefix);
        assert_eq!(result, Some("echo こんにちは".to_string()));
    }

    #[test]
    fn fuzzy_match_マルチバイト文字() {
        // ASCII 範囲外でも文字単位で評価できる
        assert!(fuzzy_match("猫犬", "猫鳥犬"));
        assert!(!fuzzy_match("犬猫", "猫鳥犬"));
    }

    #[test]
    fn parse_history_line_拡張形式でマルチバイトコマンド() {
        assert_eq!(
            parse_history_line(": 1700000000:0;echo こんにちは"),
            "echo こんにちは"
        );
    }

    /// zsh が histfile 書き込み時に行う metafy を再現する (テスト用)。
    /// 0x83 (Meta) を含む特殊域のバイトを `0x83 + byte^0x20` にエスケープする。
    pub(super) fn metafy(raw: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for &b in raw.as_bytes() {
            if (0x83..=0xa2).contains(&b) {
                out.push(0x83);
                out.push(b ^ 0x20);
            } else {
                out.push(b);
            }
        }
        out
    }

    #[test]
    fn read_history_メタファイされた日本語を復元する() {
        // 末尾に多バイト文字を含むファイルでもパニック・データ破損しない。
        // zsh は非 ASCII を metafy して保存するため、実際のディスク形式で書く。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, metafy("ls\necho 日本語コマンド\n")).unwrap();
        let result = read_history(&path).expect("読み込みは成功すべき");
        assert!(
            String::from_utf8(result)
                .unwrap()
                .contains("日本語コマンド")
        );
    }

    #[test]
    fn get_completions_重複検出の正確性() {
        // 同一コマンドが複数回出現しても 1 件しか返さない
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(
            &path,
            "git status\ngit push\ngit status\ngit pull\ngit push\n",
        )
        .unwrap();
        let results = get_completions("git", Some(path.to_str().unwrap()), 10);
        assert_eq!(results.len(), 3, "重複除外後は 3 件: {:?}", results);
    }

    // ---------------------------------------------------------------
    // zsh 履歴ファイルの Meta エスケープ復元
    // ---------------------------------------------------------------

    #[test]
    fn unmetafy_メタペアを復元する() {
        // 「日」(E6 97 A5) はディスク上 E6 83 B7 A5 と保存される
        assert_eq!(
            unmetafy(vec![0xe6, 0x83, 0xb7, 0xa5]),
            vec![0xe6, 0x97, 0xa5]
        );
    }

    #[test]
    fn unmetafy_プレーンなバイト列はそのまま() {
        assert_eq!(unmetafy(b"echo hello".to_vec()), b"echo hello".to_vec());
    }

    #[test]
    fn unmetafy_末尾の孤立メタは捨てる() {
        assert_eq!(unmetafy(vec![0x61, 0x83]), vec![0x61]);
    }

    #[test]
    fn get_suggestion_メタファイされた日本語履歴にマッチする() {
        // 実際の zsh のディスク形式 (metafied) で日本語クエリが一致すること
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, metafy(": 1700000000:0;echo 日本語テスト\n")).unwrap();
        let result = get_suggestion(
            "echo 日",
            Some(path.to_str().unwrap()),
            &Strategy::Prefix,
            None,
        );
        assert_eq!(result, Some("echo 日本語テスト".to_string()));
    }

    // ---------------------------------------------------------------
    // 複数行コマンド (継続行) の扱い
    // ---------------------------------------------------------------

    #[test]
    fn continues_to_next_line_の判定規則() {
        assert!(continues_to_next_line(b"echo foo\\"));
        // zsh の readhistline は `\\` 終端も継続とみなす (`echo first \` + 改行 + 続き を
        // 入力すると、改行の前の `\` に継続の `\` が足されて `echo first \\` と書かれる)
        assert!(continues_to_next_line(b"echo foo\\\\"));
        // 実際に `\` で終わるコマンドは `\ `+LF で書かれる
        assert!(!continues_to_next_line(b"echo foo\\ "));
        assert!(!continues_to_next_line(b"echo foo"));
    }

    #[test]
    fn 行継続を含む複数行の履歴は断片を候補にしない() {
        // zsh 5.9 で `echo first \` Enter `second-part` と入力したときの書き方
        let text = ": 1:0;echo first \\\\\nsecond-part\n: 2:0;git status\n";
        assert_eq!(search_text("sec", text.as_bytes(), &Strategy::Prefix), None);
        assert_eq!(
            search_text("echo first", text.as_bytes(), &Strategy::Prefix),
            None
        );
        assert_eq!(
            search_text("git", text.as_bytes(), &Strategy::Prefix).as_deref(),
            Some("git status")
        );
    }

    #[test]
    fn バックスラッシュと空白で終わるコマンドは書き足された空白だけを外す() {
        // `touch file\ ` は zsh が空白を 1 つ足して `touch file\  ` と書く
        assert_eq!(history_command(": 1:0;touch file\\  "), "touch file\\ ");
        assert_eq!(history_command(": 1:0;echo foo\\ "), "echo foo\\");
        assert_eq!(history_command(": 1:0;  ls -la  "), "ls -la");
    }

    #[test]
    fn crの直前のバックスラッシュは行継続とみなさず次のエントリを失わない() {
        // zsh 5.9 が `print -sr -- $'echo slash-cr\\\r'` などを `fc -W` で書いた内容
        let text = ": 1:0;echo slash-cr\\\r\n: 2:0;echo next-entry\n: 3:0;echo crlf\r\n";
        assert_eq!(
            search_text("echo n", text.as_bytes(), &Strategy::Prefix).as_deref(),
            Some("echo next-entry")
        );
    }

    #[test]
    fn crを含むエントリは制御文字を含む履歴として候補にしない() {
        let text = ": 1:0;echo crlf\r\n: 2:0;ls\n";
        assert_eq!(
            search_text("echo c", text.as_bytes(), &Strategy::Prefix),
            None
        );
        assert_eq!(history_command(": 1:0;echo crlf\r"), "echo crlf\r");
    }

    #[test]
    fn 全角空白はコマンドの一部として外さない() {
        // zsh が外すのは書き足した ASCII の空白だけで、`\` と全角空白で終わる行は継続しない
        assert_eq!(
            history_command(": 1:0;echo dir\\\u{3000}"),
            "echo dir\\\u{3000}"
        );
    }

    #[test]
    fn 非拡張形式のコロン始まりのコマンドはエスケープを外す() {
        assert_eq!(parse_history_line("\\: colon-cmd arg"), ": colon-cmd arg");
        assert_eq!(parse_history_line("\\ls"), "\\ls");
        let text = "\\: > truncated.log\nls\n";
        assert_eq!(
            search_text(": >", text.as_bytes(), &Strategy::Prefix).as_deref(),
            Some(": > truncated.log")
        );
    }

    #[test]
    fn search_text_複数行コマンドの断片を候補にしない() {
        // `echo foo\` + `echo bar` は 1 つの複数行エントリ
        let text = "echo foo\\\necho bar\ngit status\n";
        assert_eq!(
            search_text("echo f", text.as_bytes(), &Strategy::Prefix),
            None
        );
        // 継続の 2 行目 (body) も候補にしない
        assert_eq!(
            search_text("echo b", text.as_bytes(), &Strategy::Prefix),
            None
        );
        // 単独行エントリは通常どおり候補になる
        assert_eq!(
            search_text("git s", text.as_bytes(), &Strategy::Prefix),
            Some("git status".to_string())
        );
    }

    #[test]
    fn search_text_forループ断片を候補にしない() {
        let text = "for i in 1 2 3\\\ndo echo $i\\\ndone\nls -la\n";
        assert_eq!(search_text("do", text.as_bytes(), &Strategy::Prefix), None);
        assert_eq!(search_text("don", text.as_bytes(), &Strategy::Prefix), None);
        assert_eq!(
            search_text("ls", text.as_bytes(), &Strategy::Prefix),
            Some("ls -la".to_string())
        );
    }

    #[test]
    fn get_completions_複数行コマンドの断片を返さない() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, "echo foo\\\necho bar\necho baz\n").unwrap();
        let results = get_completions("echo", Some(path.to_str().unwrap()), 10);
        assert_eq!(results, vec!["echo baz".to_string()]);
    }

    // ---------------------------------------------------------------
    // 書きかけ最終行 (追記中の torn read) の除外
    // ---------------------------------------------------------------

    #[test]
    fn drop_partial_tail_改行終端でない最終行を捨てる() {
        assert_eq!(drop_partial_tail(b"a\nb".to_vec()), b"a\n");
        assert_eq!(drop_partial_tail(b"a\nb\n".to_vec()), b"a\nb\n");
        assert_eq!(drop_partial_tail(b"ab".to_vec()), b"");
        assert_eq!(drop_partial_tail(Vec::new()), b"");
    }

    #[test]
    fn get_suggestion_書きかけの最終行を候補にしない() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        // 最終行が改行なし = zsh が追記中
        std::fs::write(&path, "git status\ngit push --forc").unwrap();
        let result = get_suggestion(
            "git pu",
            Some(path.to_str().unwrap()),
            &Strategy::Prefix,
            None,
        );
        assert_eq!(result, None);
        let result = get_suggestion(
            "git s",
            Some(path.to_str().unwrap()),
            &Strategy::Prefix,
            None,
        );
        assert_eq!(result, Some("git status".to_string()));
    }

    // ---------------------------------------------------------------
    // 履歴パス解決の空文字列扱い
    // ---------------------------------------------------------------

    #[test]
    fn resolve_history_path_空文字列指定は未設定として扱う() {
        // 空パスをそのまま開こうとしないこと
        assert_eq!(resolve_history_path(Some("")), None);
    }

    // ---------------------------------------------------------------
    // 大きい履歴の全体検索
    // ---------------------------------------------------------------

    #[test]
    fn get_suggestion_大きい履歴の先頭も検索する() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        // 先頭に一意な行、その後 64KB 超の filler
        let mut content = String::from("git unique-head-cmd\n");
        while content.len() < 65_536 + 4096 {
            content.push_str("ls -la\n");
        }
        std::fs::write(&path, &content).unwrap();
        let result = get_suggestion(
            "git uni",
            Some(path.to_str().unwrap()),
            &Strategy::Prefix,
            None,
        );
        assert_eq!(result, Some("git unique-head-cmd".to_string()));
    }

    #[test]
    fn get_suggestion_古い前方一致を新しい部分一致より優先する() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        let content = format!("git target\n{}\necho git recent\n", "x".repeat(65_536));
        std::fs::write(&path, content).unwrap();

        let result = get_suggestion(
            "git",
            Some(path.to_str().unwrap()),
            &Strategy::Substring,
            None,
        );
        assert_eq!(result, Some("git target".to_string()));
    }

    #[test]
    fn get_suggestion_長い複数行コマンドの断片を候補にしない() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        let content = format!("{}\\\ngit fake\n", "x".repeat(65_536));
        std::fs::write(&path, content).unwrap();

        let result = get_suggestion("git", Some(path.to_str().unwrap()), &Strategy::Prefix, None);
        assert_eq!(result, None);
        let completions = get_completions("git", Some(path.to_str().unwrap()), 10);
        assert!(completions.is_empty());
    }
}

#[cfg(test)]
mod ranking_tests {
    use super::*;

    #[test]
    fn 不正なutf8の行は置換せず除外し有効な履歴と置換文字自身は残す() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hist");
        let mut bytes = tests::metafy("cat valid\ncat �.txt\n");
        bytes.extend_from_slice(b"cat \xff\xfe.txt\n");
        std::fs::write(&path, bytes).unwrap();
        let file = Some(path.to_str().unwrap());
        assert_eq!(
            get_history_suggestion("cat ", file, &Strategy::Prefix).as_deref(),
            Some("cat �.txt")
        );
        assert_eq!(
            get_completions("cat ", file, 10),
            ["cat �.txt", "cat valid"]
        );
    }

    #[test]
    fn 部分一致の位置は文字数で比較する() {
        let history = "abcdefgh foo\n日本語 foo\n";
        assert_eq!(
            ranked_candidates("foo", history.as_bytes(), &Strategy::Substring, 10),
            ["日本語 foo", "abcdefgh foo"]
        );
    }

    #[test]
    fn 不正なutf8の継続行を候補にもタスクの利用回数にも含めない() {
        let history = b"make good\nmake \xff\\\nmake fake\nmake \xfe\nmake other\n";
        assert_eq!(
            ranked_candidates("make", history, &Strategy::Prefix, 10),
            ["make other", "make good"]
        );
        let calls = task_calls("make", history);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls["good"].count, 1);
        assert_eq!(calls["other"].count, 1);
    }

    #[test]
    fn 曖昧一致の長さはバイト数ではなく文字数で比較する() {
        let history = "éXab\né🐱ab\néXYZab\n";
        assert_eq!(
            ranked_candidates("éa", history.as_bytes(), &Strategy::Fuzzy, 10),
            ["é🐱ab", "éXab", "éXYZab"]
        );
        assert!(ranked_candidates("éa", history.as_bytes(), &Strategy::Substring, 10).is_empty());
    }

    #[test]
    fn 候補を絞っても頻度と新しさによる上位順を保つ() {
        let mut history = (0..40)
            .map(|i| format!("git show {i}\n"))
            .collect::<String>();
        history.push_str("git show 7\ngit show 3\ngit show 7\n");
        assert_eq!(
            ranked_candidates("git", history.as_bytes(), &Strategy::Prefix, 5),
            [
                "git show 7",
                "git show 3",
                "git show 39",
                "git show 38",
                "git show 37"
            ]
        );
        assert_eq!(
            ranked_candidates("git", history.as_bytes(), &Strategy::Prefix, 1),
            ["git show 7"]
        );
        assert_eq!(
            ranked_candidates("git", history.as_bytes(), &Strategy::Prefix, usize::MAX).len(),
            40
        );
    }

    #[test]
    fn 一致度を優先し同程度なら頻度と新しさで並べる() {
        let history = "zsh-turbo configure\nzsh-turbo configure\nzsh-turbo doctor\nsudo zsh-turbo configure\nsudo zsh-turbo configure\nsudo zsh-turbo configure\n";
        assert_eq!(
            ranked_candidates("zsh-turbo", history.as_bytes(), &Strategy::Fuzzy, 10),
            [
                "zsh-turbo configure",
                "zsh-turbo doctor",
                "sudo zsh-turbo configure"
            ]
        );
        assert_eq!(
            search_text("zsh-turbo", history.as_bytes(), &Strategy::Fuzzy).as_deref(),
            Some("zsh-turbo configure")
        );
        let tied = "git status\ngit diff\n";
        assert_eq!(
            ranked_candidates("git", tied.as_bytes(), &Strategy::Prefix, 10),
            ["git diff", "git status"]
        );
    }

    #[test]
    fn 制御文字を含む履歴は表示候補にせず日本語は保持する() {
        let history = "echo \u{1b}[2J\necho a\u{7}\necho 日本語\n";
        assert_eq!(
            ranked_candidates("echo", history.as_bytes(), &Strategy::Prefix, 10),
            ["echo 日本語"]
        );
        assert!(ranked_candidates("echo\n", history.as_bytes(), &Strategy::Fuzzy, 10).is_empty());
    }

    #[test]
    fn 空の曖昧検索は全履歴から順位付けし重複を除く() {
        let history = "echo alpha\necho alpha\necho beta\n";
        assert_eq!(
            ranked_candidates("", history.as_bytes(), &Strategy::Fuzzy, 10),
            ["echo alpha", "echo beta"]
        );
        assert_eq!(
            ranked_candidates("ealp", history.as_bytes(), &Strategy::Fuzzy, 10),
            ["echo alpha"]
        );
    }

    #[test]
    fn 履歴からタスクの呼び出しを数え最後に使った位置を覚える() {
        let history = ": 1700000000:0;make install\nmake test\nmake install PREFIX=/x\nmake -C sub build\nmake install && make test\nnpm run build\nmakefoo x\nmake test\\\nmake build\n";
        let calls = task_calls("make", history.as_bytes());
        assert_eq!(
            calls["install"],
            task_usage::HistoryUse {
                count: 2,
                newest: 4
            }
        );
        assert_eq!(
            calls["test"],
            task_usage::HistoryUse {
                count: 1,
                newest: 5
            }
        );
        // ディレクトリを変える呼び出し・複数行の断片・別のコマンドは数えない
        assert_eq!(calls.len(), 2, "{calls:?}");
        assert_eq!(task_calls("npm", history.as_bytes()).len(), 1);
    }
}
