use crate::project_tasks;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

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

const CHUNK_SIZE: u64 = 65536; // ファイル末尾から 64KB ずつ読む

/// zsh histfile の metafy エスケープ (0x83 + byte^0x20) を復元する。
/// zsh は特定の特殊バイトを Meta (0x83) エスケープして保存するため、非 ASCII
/// コマンドはディスク上では有効な UTF-8 でなく、復元しないと候補が文字化けし
/// 生 UTF-8 のクエリとも一致しない。
/// Meta ペアの 2 バイト目は必ず 0x80 以上で改行になり得ないため、
/// 行分割の前後どちらで呼んでも安全。
fn unmetafy(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut iter = bytes.iter().copied();
    while let Some(b) = iter.next() {
        if b == 0x83 {
            match iter.next() {
                Some(next) => out.push(next ^ 0x20),
                None => break, // 末尾の孤立 Meta (書きかけ) は捨てる
            }
        } else {
            out.push(b);
        }
    }
    out
}

/// 末尾が改行で終わらない場合、zsh が追記中の書きかけ最終行を捨てる。
/// zsh は 1 エントリを必ず改行終端で書くため、完全なファイルでは何もしない。
fn drop_partial_tail(mut text: String) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        match text.rfind('\n') {
            Some(idx) => text.truncate(idx + 1),
            None => text.clear(),
        }
    }
    text
}

/// 履歴ファイルを末尾からチャンク単位で読み込む。
/// 全ファイル読み込みに比べ、直近のマッチ検索が高速。
fn read_history_chunks(path: &PathBuf) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();
    if file_len == 0 {
        return None;
    }
    let read_start = file_len.saturating_sub(CHUNK_SIZE);
    // 境界直前の 2 バイトも読み、直前の物理行が継続行か判定する。
    let prefix_len = read_start.min(2) as usize;
    file.seek(SeekFrom::Start(read_start - prefix_len as u64))
        .ok()?;
    let read_len = (file_len - read_start) as usize + prefix_len;
    let mut buf = vec![0u8; read_len];
    file.read_exact(&mut buf).ok()?;
    // ファイル途中から読んだ場合、先頭の不完全な行と、その継続部分を捨てる。
    let slice: &[u8] = if read_start > 0 {
        let Some(first_end_offset) = buf[prefix_len..].iter().position(|&b| b == b'\n') else {
            return Some(String::new());
        };
        let first_end = prefix_len + first_end_offset;
        let mut start = first_end + 1;
        let mut continued = continues_to_next_line_bytes(&buf[..first_end]);
        while continued {
            let Some(end_offset) = buf[start..].iter().position(|&b| b == b'\n') else {
                return Some(String::new());
            };
            let end = start + end_offset;
            continued = continues_to_next_line_bytes(&buf[start..end]);
            start = end + 1;
        }
        &buf[start..]
    } else {
        &buf
    };
    Some(drop_partial_tail(
        String::from_utf8_lossy(&unmetafy(slice)).into_owned(),
    ))
}

/// 履歴ファイルを全読み込みする（末尾チャンクでマッチしなかった場合のフォールバック）。
fn read_history_full(path: &PathBuf) -> Option<String> {
    let content = std::fs::read(path).ok()?;
    Some(drop_partial_tail(
        String::from_utf8_lossy(&unmetafy(&content)).into_owned(),
    ))
}

/// zsh histfile の物理行が次の物理行へ継続する (`\` 終端) かどうか。
/// zsh は複数行コマンドの改行を `\`+LF で書く。`\\` 終端は継続ではない
/// (zsh readhistline と同じ規則)。実際に `\` で終わる単一行コマンドは
/// `\ `+LF で書かれるため誤検出しない。
fn continues_to_next_line(line: &str) -> bool {
    continues_to_next_line_bytes(line.as_bytes())
}

fn continues_to_next_line_bytes(line: &[u8]) -> bool {
    line.ends_with(b"\\") && !line.ends_with(b"\\\\")
}

/// 履歴テキストを新しい順に走査し、複数行エントリ (継続行) を除いた
/// 単独行の履歴コマンドだけを返す。複数行エントリは改行を含み、
/// 1 行プロトコル (zsh 側の `read -r`) で運べないため候補にしない。
fn history_candidates_rev(text: &str) -> impl Iterator<Item = &str> {
    let mut lines = text.lines().rev().peekable();
    std::iter::from_fn(move || {
        while let Some(line) = lines.next() {
            // ファイル順で 1 つ前の物理行が `\` 終端なら、この行は継続部分
            let prev_continues = lines
                .peek()
                .is_some_and(|prev| continues_to_next_line(prev));
            if prev_continues || continues_to_next_line(line) {
                continue;
            }
            return Some(parse_history_line(line).trim());
        }
        None
    })
}

/// 指定された戦略でサジェストを 1 件取得する。
/// prefix → substring → fuzzy の順でフォールバックする。
/// 同程度の一致では使用頻度と新しさを優先する。
pub fn get_suggestion(
    query: &str,
    history_file: Option<&str>,
    strategy: &Strategy,
) -> Option<String> {
    if query.is_empty() {
        return None;
    }

    if let Some(candidate) = project_tasks::candidates(query, 1).into_iter().next() {
        return Some(candidate);
    }

    let history_path = resolve_history_path(history_file)?;

    let text = read_ranked_history(&history_path)?;
    search_text(query, &text, strategy)
}

fn read_ranked_history(path: &PathBuf) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > CHUNK_SIZE {
        read_history_full(path)
    } else {
        read_history_chunks(path)
    }
}

fn search_text(query: &str, text: &str, strategy: &Strategy) -> Option<String> {
    ranked_candidates(query, text, strategy, 1)
        .into_iter()
        .next()
}

fn ranked_candidates(query: &str, text: &str, strategy: &Strategy, max: usize) -> Vec<String> {
    if max == 0 || query.chars().any(char::is_control) {
        return Vec::new();
    }
    let mut matches = HashMap::<&str, ((u8, usize), usize, usize)>::new();
    let query_chars = query.chars().count();
    for (recency, cmd) in history_candidates_rev(text).enumerate() {
        if cmd.is_empty() || cmd == query {
            continue;
        }
        if let Some((_, frequency, _)) = matches.get_mut(cmd) {
            *frequency = frequency.saturating_add(1);
            continue;
        }
        let quality = if cmd.starts_with(query) {
            (0, 0)
        } else if matches!(strategy, Strategy::Substring | Strategy::Fuzzy) && cmd.contains(query) {
            (1, cmd.find(query).unwrap())
        } else if matches!(strategy, Strategy::Fuzzy) && fuzzy_match(query, cmd) {
            (2, cmd.chars().count().saturating_sub(query_chars))
        } else {
            continue;
        };
        if cmd.chars().any(char::is_control) {
            continue;
        }
        matches.insert(cmd, (quality, 1, recency));
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
    let Some(text) = resolve_history_path(history_file).and_then(|path| read_ranked_history(&path))
    else {
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

fn resolve_history_path(history_file: Option<&str>) -> Option<PathBuf> {
    // 空文字列の指定・HISTFILE は未設定として扱う (XDG_CONFIG_HOME 等と同じ規約)
    history_file
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HISTFILE")
                .ok()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| dirs::home_dir().map(|h| h.join(".zsh_history")))
}

/// zsh 拡張履歴形式をパース: ": <timestamp>:<duration>;<command>"
/// `<timestamp>` と `<duration>` は数字のみで構成される必要がある。
/// 形式に合致しない場合は元の行をそのまま返す（例: `: echo a;git` のような通常履歴）。
fn parse_history_line(line: &str) -> &str {
    let Some(rest) = line.strip_prefix(": ") else {
        return line;
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
        let result = search_text("git", text, &Strategy::Prefix);
        assert!(result.is_some());
        assert!(result.unwrap().starts_with("git"));
    }

    #[test]
    fn search_text_完全一致はスキップする() {
        let text = "git\ngit push";
        let result = search_text("git", text, &Strategy::Prefix);
        // "git" と完全一致する行はスキップし、"git push" を返す
        assert_eq!(result, Some("git push".to_string()));
    }

    #[test]
    fn search_text_マッチなしでnone() {
        let text = "ls -la\ncd /tmp";
        let result = search_text("git", text, &Strategy::Prefix);
        assert!(result.is_none());
    }

    #[test]
    fn search_text_substring戦略で部分一致を返す() {
        let text = "docker run nginx\nkubectl get pods";
        // "nginx" は prefix マッチしないが substring マッチする
        let result = search_text("nginx", text, &Strategy::Substring);
        assert_eq!(result, Some("docker run nginx".to_string()));
    }

    #[test]
    fn search_text_fuzzy戦略でファジーマッチを返す() {
        let text = "git push origin main";
        let result = search_text("gpom", text, &Strategy::Fuzzy);
        assert_eq!(result, Some("git push origin main".to_string()));
    }

    #[test]
    fn search_text_空テキストでnone() {
        let result = search_text("git", "", &Strategy::Prefix);
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
    fn resolve_history_path_noneでもフォールバック() {
        // HISTFILE 環境変数またはデフォルトパスにフォールバック
        let path = resolve_history_path(None);
        assert!(path.is_some(), "フォールバックパスが返されるべき");
    }

    // -------------------------------------------------------
    // 分割読み込みと全体読み込みのテスト
    // -------------------------------------------------------

    #[test]
    fn read_history_chunks_空ファイルはnone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty_history");
        std::fs::write(&path, "").unwrap();
        assert!(read_history_chunks(&path).is_none());
    }

    #[test]
    fn read_history_chunks_小さいファイルを読める() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small_history");
        std::fs::write(&path, "ls\ncd /tmp\ngit status\n").unwrap();
        let result = read_history_chunks(&path);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains("git status"));
    }

    #[test]
    fn read_history_full_ファイル全体を読める() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("full_history");
        std::fs::write(&path, "first\nlast\n").unwrap();
        let result = read_history_full(&path);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains("first"));
        assert!(content.contains("last"));
    }

    #[test]
    fn read_history_full_存在しないファイルはnone() {
        let path = PathBuf::from("/tmp/nonexistent_history_xyz_999");
        assert!(read_history_full(&path).is_none());
    }

    // -------------------------------------------------------
    // サジェスト取得の統合テスト
    // -------------------------------------------------------

    #[test]
    fn get_suggestion_空クエリはnone() {
        assert!(get_suggestion("", None, &Strategy::Prefix).is_none());
    }

    #[test]
    fn get_suggestion_履歴ファイルから候補を返す() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, "git status\ngit push\nls -la\n").unwrap();
        let result = get_suggestion("git", Some(path.to_str().unwrap()), &Strategy::Prefix);
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
    fn read_history_chunks_先頭の不完全な行を捨てる() {
        // CHUNK_SIZE (64KB) を超えるファイル先頭に長いガード行を置き、
        // 末尾チャンクの先頭が不完全な行になる状況を再現する。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        // 行頭に必ず判別可能な接頭辞 "broken-" を含む長い 1 行を先頭に置く
        let broken_line = format!("broken-{}\n", "x".repeat(80_000));
        let valid_line = "valid-cmd\n";
        let content = format!("{broken_line}{valid_line}");
        std::fs::write(&path, &content).unwrap();

        let result = read_history_chunks(&path).expect("チャンク読み込みは成功すべき");
        // 末尾の有効な行は必ず含まれる
        assert!(
            result.contains("valid-cmd"),
            "末尾の有効な行が読み込まれていない"
        );
        // 先頭の不完全な行（broken-xxxxx... の途中）は除外されている
        assert!(
            !result.contains("broken-"),
            "不完全な先頭行が含まれている: {:?}",
            result.chars().take(50).collect::<String>()
        );
    }

    // -------------------------------------------------------
    // 補完候補取得: ファイル全体読み込みへのフォールバック
    // -------------------------------------------------------

    #[test]
    fn get_completions_フルファイルフォールバックで追加候補を取得() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        // CHUNK_SIZE (64KB) より大きいファイルを作成し、先頭に一意なコマンドを配置
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
        // フォールバックにより先頭のコマンドも取得できる
        assert!(
            results.iter().any(|r| r == unique_cmd),
            "フルファイルフォールバックで先頭のコマンドが取得されていない: {:?}",
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
        let result = get_suggestion("git", Some(path.to_str().unwrap()), &Strategy::Substring);
        // prefix マッチが先に試されるため "git push" が返る
        assert_eq!(result, Some("git push".to_string()));
    }

    #[test]
    fn get_suggestion_fuzzy戦略でsubstringマッチも返す() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, "docker run nginx\n").unwrap();
        let result = get_suggestion("nginx", Some(path.to_str().unwrap()), &Strategy::Fuzzy);
        // substring マッチにより "docker run nginx" が返る
        assert_eq!(result, Some("docker run nginx".to_string()));
    }

    #[test]
    fn get_suggestion_存在しないファイルはnone() {
        let result = get_suggestion(
            "git",
            Some("/tmp/nonexistent_history_xyz_999"),
            &Strategy::Prefix,
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
        let result = search_text("echo こ", text, &Strategy::Prefix);
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
    fn metafy(raw: &str) -> Vec<u8> {
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
    fn read_history_chunks_マルチバイト末尾でも壊れない() {
        // 末尾に多バイト文字を含むファイルでもパニック・データ破損しない。
        // zsh は非 ASCII を metafy して保存するため、実際のディスク形式で書く。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, metafy("ls\necho 日本語コマンド\n")).unwrap();
        let result = read_history_chunks(&path).expect("読み込みは成功すべき");
        assert!(result.contains("日本語コマンド"));
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
        assert_eq!(unmetafy(&[0xe6, 0x83, 0xb7, 0xa5]), vec![0xe6, 0x97, 0xa5]);
    }

    #[test]
    fn unmetafy_プレーンなバイト列はそのまま() {
        assert_eq!(unmetafy(b"echo hello"), b"echo hello".to_vec());
    }

    #[test]
    fn unmetafy_末尾の孤立メタは捨てる() {
        assert_eq!(unmetafy(&[0x61, 0x83]), vec![0x61]);
    }

    #[test]
    fn get_suggestion_メタファイされた日本語履歴にマッチする() {
        // 実際の zsh のディスク形式 (metafied) で日本語クエリが一致すること
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, metafy(": 1700000000:0;echo 日本語テスト\n")).unwrap();
        let result = get_suggestion("echo 日", Some(path.to_str().unwrap()), &Strategy::Prefix);
        assert_eq!(result, Some("echo 日本語テスト".to_string()));
    }

    // ---------------------------------------------------------------
    // 複数行コマンド (継続行) の扱い
    // ---------------------------------------------------------------

    #[test]
    fn continues_to_next_line_の判定規則() {
        assert!(continues_to_next_line("echo foo\\"));
        // `\\` (二重バックスラッシュ) 終端は継続ではない (zsh readhistline と同じ)
        assert!(!continues_to_next_line("echo foo\\\\"));
        // 実際に `\` で終わるコマンドは `\ `+LF で書かれる
        assert!(!continues_to_next_line("echo foo\\ "));
        assert!(!continues_to_next_line("echo foo"));
    }

    #[test]
    fn search_text_複数行コマンドの断片を候補にしない() {
        // `echo foo\` + `echo bar` は 1 つの複数行エントリ
        let text = "echo foo\\\necho bar\ngit status\n";
        assert_eq!(search_text("echo f", text, &Strategy::Prefix), None);
        // 継続の 2 行目 (body) も候補にしない
        assert_eq!(search_text("echo b", text, &Strategy::Prefix), None);
        // 単独行エントリは通常どおり候補になる
        assert_eq!(
            search_text("git s", text, &Strategy::Prefix),
            Some("git status".to_string())
        );
    }

    #[test]
    fn search_text_forループ断片を候補にしない() {
        let text = "for i in 1 2 3\\\ndo echo $i\\\ndone\nls -la\n";
        assert_eq!(search_text("do", text, &Strategy::Prefix), None);
        assert_eq!(search_text("don", text, &Strategy::Prefix), None);
        assert_eq!(
            search_text("ls", text, &Strategy::Prefix),
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
        assert_eq!(drop_partial_tail("a\nb".to_string()), "a\n");
        assert_eq!(drop_partial_tail("a\nb\n".to_string()), "a\nb\n");
        assert_eq!(drop_partial_tail("ab".to_string()), "");
        assert_eq!(drop_partial_tail(String::new()), "");
    }

    #[test]
    fn get_suggestion_書きかけの最終行を候補にしない() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        // 最終行が改行なし = zsh が追記中
        std::fs::write(&path, "git status\ngit push --forc").unwrap();
        let result = get_suggestion("git pu", Some(path.to_str().unwrap()), &Strategy::Prefix);
        assert_eq!(result, None);
        let result = get_suggestion("git s", Some(path.to_str().unwrap()), &Strategy::Prefix);
        assert_eq!(result, Some("git status".to_string()));
    }

    // ---------------------------------------------------------------
    // 履歴パス解決の空文字列扱い
    // ---------------------------------------------------------------

    #[test]
    fn resolve_history_path_空文字列指定は未設定として扱う() {
        // 空の --history-file (HISTFILE 未設定の init.zsh 経由) で
        // 空パスをそのまま開こうとしないこと
        let resolved = resolve_history_path(Some(""));
        assert_ne!(resolved, Some(PathBuf::from("")));
    }

    // ---------------------------------------------------------------
    // 全ファイルフォールバック (末尾チャンク不一致時)
    // ---------------------------------------------------------------

    #[test]
    fn get_suggestion_チャンク外の古い履歴にフォールバックする() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        // 先頭に一意な行、その後 64KB 超の filler
        let mut content = String::from("git unique-head-cmd\n");
        while content.len() < (CHUNK_SIZE as usize) + 4096 {
            content.push_str("ls -la\n");
        }
        std::fs::write(&path, &content).unwrap();
        let result = get_suggestion("git uni", Some(path.to_str().unwrap()), &Strategy::Prefix);
        assert_eq!(result, Some("git unique-head-cmd".to_string()));
    }

    #[test]
    fn get_suggestion_古い前方一致を新しい部分一致より優先する() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        let content = format!(
            "git target\n{}\necho git recent\n",
            "x".repeat(CHUNK_SIZE as usize)
        );
        std::fs::write(&path, content).unwrap();

        let result = get_suggestion("git", Some(path.to_str().unwrap()), &Strategy::Substring);
        assert_eq!(result, Some("git target".to_string()));
    }

    #[test]
    fn get_suggestion_チャンク境界の継続行を候補にしない() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        let content = format!("{}\\\ngit fake\n", "x".repeat(CHUNK_SIZE as usize));
        std::fs::write(&path, content).unwrap();

        let result = get_suggestion("git", Some(path.to_str().unwrap()), &Strategy::Prefix);
        assert_eq!(result, None);
        let completions = get_completions("git", Some(path.to_str().unwrap()), 10);
        assert!(completions.is_empty());
    }
}

#[cfg(test)]
mod ranking_tests {
    use super::*;

    #[test]
    fn 候補を絞っても頻度と新しさによる上位順を保つ() {
        let mut history = (0..40)
            .map(|i| format!("git show {i}\n"))
            .collect::<String>();
        history.push_str("git show 7\ngit show 3\ngit show 7\n");
        assert_eq!(
            ranked_candidates("git", &history, &Strategy::Prefix, 5),
            [
                "git show 7",
                "git show 3",
                "git show 39",
                "git show 38",
                "git show 37"
            ]
        );
        assert_eq!(
            ranked_candidates("git", &history, &Strategy::Prefix, 1),
            ["git show 7"]
        );
        assert_eq!(
            ranked_candidates("git", &history, &Strategy::Prefix, usize::MAX).len(),
            40
        );
    }

    #[test]
    fn 一致度を優先し同程度なら頻度と新しさで並べる() {
        let history = "zsh-turbo configure\nzsh-turbo configure\nzsh-turbo doctor\nsudo zsh-turbo configure\nsudo zsh-turbo configure\nsudo zsh-turbo configure\n";
        assert_eq!(
            ranked_candidates("zsh-turbo", history, &Strategy::Fuzzy, 10),
            [
                "zsh-turbo configure",
                "zsh-turbo doctor",
                "sudo zsh-turbo configure"
            ]
        );
        assert_eq!(
            search_text("zsh-turbo", history, &Strategy::Fuzzy).as_deref(),
            Some("zsh-turbo configure")
        );
        let tied = "git status\ngit diff\n";
        assert_eq!(
            ranked_candidates("git", tied, &Strategy::Prefix, 10),
            ["git diff", "git status"]
        );
    }

    #[test]
    fn 制御文字を含む履歴は表示候補にせず日本語は保持する() {
        let history = "echo \u{1b}[2J\necho a\u{7}\necho 日本語\n";
        assert_eq!(
            ranked_candidates("echo", history, &Strategy::Prefix, 10),
            ["echo 日本語"]
        );
        assert!(ranked_candidates("echo\n", history, &Strategy::Fuzzy, 10).is_empty());
    }

    #[test]
    fn 空の曖昧検索は全履歴から順位付けし重複を除く() {
        let history = "echo alpha\necho alpha\necho beta\n";
        assert_eq!(
            ranked_candidates("", history, &Strategy::Fuzzy, 10),
            ["echo alpha", "echo beta"]
        );
        assert_eq!(
            ranked_candidates("ealp", history, &Strategy::Fuzzy, 10),
            ["echo alpha"]
        );
    }
}
