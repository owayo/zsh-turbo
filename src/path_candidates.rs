//! 入力中のパス引数に対するディレクトリ内の候補を返す。

use std::cmp::Ordering;
use std::ffi::OsString;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use unicode_normalization::UnicodeNormalization;

/// read_dir が戻らない (NFS 等) ときに結果を待つ上限
const SCAN_TIMEOUT: Duration = Duration::from_millis(500);
/// 走査を打ち切る経過時間
const SCAN_BUDGET: Duration = Duration::from_millis(150);
/// 走査を打ち切るエントリ数
const MAX_SCAN_ENTRIES: usize = 50_000;

/// 一覧に表示する 1 件の候補
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathCandidate {
    /// 一覧に表示する名前 (NFC 正規化済み、ディレクトリは末尾 `/`)
    pub label: String,
    /// 候補を採用した後の BUFFER 全体
    pub buffer: String,
}

/// パス候補の一覧
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathListing {
    pub candidates: Vec<PathCandidate>,
    /// 走査で見つかった一致件数 (`complete` が false のときは下限)
    pub total: usize,
    /// ディレクトリを最後まで走査したか
    pub complete: bool,
}

/// BUFFER 末尾の語がパスなら、その親ディレクトリの候補を最大 `limit` 件返す。
/// 対象外の入力なら None。
pub fn list(buffer: &str, limit: usize) -> Option<PathListing> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let home = dirs::home_dir();
    let var = |name: &str| std::env::var_os(name);
    let env = Env {
        cwd: &cwd,
        home: home.as_deref(),
        var: &var,
    };
    let request = prepare(buffer, limit, &env)?;
    // NFS 等で read_dir が戻らなくても ZLE を待たせないよう別スレッドで走査する
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .spawn(move || {
            let _ = sender.send(request.scan());
        })
        .ok()?;
    receiver.recv_timeout(SCAN_TIMEOUT).ok().flatten()
}

/// パスの解決に使う環境
struct Env<'a> {
    cwd: &'a Path,
    home: Option<&'a Path>,
    var: &'a dyn Fn(&str) -> Option<OsString>,
}

/// 走査するディレクトリと候補の組み立て方
struct Request {
    dir: PathBuf,
    /// 最後の `/` までの BUFFER (元の表記のまま残す)
    head: String,
    /// 照合する basename
    base: String,
    /// 最後の `/` より後ろの BUFFER (入力されたままの表記)
    raw_base: String,
    /// 最後の `/` のクォート文脈
    quote: Quote,
    /// 実行可能ファイルとディレクトリだけを候補にする
    executables: bool,
    limit: usize,
}

/// BUFFER を解析して走査条件を組み立てる。対象外なら None。
fn prepare(buffer: &str, limit: usize, env: &Env) -> Option<Request> {
    if limit == 0 || buffer.chars().any(char::is_control) {
        return None;
    }
    let (word, command) = last_word(buffer)?;
    if word.unsupported {
        return None;
    }
    let start = path_start(&word.pieces);
    let path = &word.pieces[start..];
    // `=cmd` (EQUALS 展開) は再現しない
    if path.first().is_some_and(|piece| piece.is_bare('=')) {
        return None;
    }
    // `~` は直後が `/` のときだけ HOME にする (`~user` `~+` `~1` などは対象外)
    let tilde = path.first().is_some_and(|piece| piece.is_bare('~'));
    if tilde && path.get(1).is_some_and(|piece| !piece.is_char('/')) {
        return None;
    }
    let slash = path.iter().rposition(|piece| piece.is_char('/'))?;
    // glob とブレース展開は再現しない
    if path
        .iter()
        .any(|piece| ['*', '?', '[', '{'].into_iter().any(|c| piece.is_bare(c)))
    {
        return None;
    }
    let (dir, base) = path.split_at(slash + 1);
    let base = base
        .iter()
        .map(|piece| match piece {
            Piece::Lit { ch, .. } => Some(*ch),
            Piece::Var(_) => None,
        })
        .collect::<Option<String>>()?;
    let mut dir_path = OsString::new();
    for (index, piece) in dir.iter().enumerate() {
        match piece {
            _ if index == 0 && tilde => dir_path.push(env.home?),
            Piece::Lit { ch, .. } => dir_path.push(ch.encode_utf8(&mut [0; 4])),
            // 未設定なら export されていないシェル変数かもしれず、正しく解決できない
            Piece::Var(name) => dir_path.push((env.var)(name)?),
        }
    }
    let Piece::Lit { end, quote, .. } = &dir[slash] else {
        return None;
    };
    Some(Request {
        dir: env.cwd.join(dir_path),
        head: buffer[..*end].to_owned(),
        base,
        raw_base: buffer[*end..].to_owned(),
        quote: *quote,
        executables: command && start == 0,
        limit,
    })
}

impl Request {
    /// 候補を採用しても、入力済みの表記の後ろに空白が付くだけか
    fn adds_only_space(&self, name: &str) -> bool {
        if self.quote != Quote::None {
            return false;
        }
        let mut escaped = String::new();
        push_escaped(&mut escaped, name, Quote::None);
        escaped == self.raw_base
    }

    fn scan(&self) -> Option<PathListing> {
        self.scan_with(MAX_SCAN_ENTRIES, SCAN_BUDGET)
    }

    fn scan_with(&self, max_entries: usize, budget: Duration) -> Option<PathListing> {
        let started = Instant::now();
        let entries = fs::read_dir(&self.dir).ok()?;
        let base = nfc(&self.base);
        let folded_base = base.to_lowercase();
        let mut found = Vec::new();
        let mut complete = true;
        for (index, entry) in entries.enumerate() {
            if index == max_entries || started.elapsed() > budget {
                complete = false;
                break;
            }
            let Ok(entry) = entry else {
                continue;
            };
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.chars().any(char::is_control)
                || name == "."
                || name == ".."
                || (name.starts_with('.') && !base.starts_with('.'))
            {
                continue;
            }
            // macOS のファイル名は NFD が多く、IME の入力は NFC のため正規化して照合する
            let normalized = nfc(&name);
            let folded = normalized.to_lowercase();
            if !folded.starts_with(&folded_base) {
                continue;
            }
            let directory = is_directory(&entry);
            // 挿入しても末尾に空白を足すだけの完全一致は出さない。開いたクォートを閉じる、
            // ディスク上の表記 (NFD など) へ直す候補は残す
            if !directory
                && (self.adds_only_space(&name)
                    || (self.executables && !is_executable(&entry.path())))
            {
                continue;
            }
            found.push(Found {
                exact: normalized.starts_with(&base),
                directory,
                folded,
                normalized,
                name,
            });
        }
        let total = found.len();
        if total > self.limit {
            found.select_nth_unstable_by(self.limit, Found::order);
            found.truncate(self.limit);
        }
        found.sort_by(Found::order);
        let candidates = found
            .into_iter()
            .map(|found| self.candidate(found))
            .collect();
        Some(PathListing {
            candidates,
            total,
            complete,
        })
    }

    fn candidate(&self, found: Found) -> PathCandidate {
        let mut label = found.normalized;
        let mut buffer = self.head.clone();
        // Linux などでも開けるよう、挿入するのはディスク上の名前そのもの
        push_escaped(&mut buffer, &found.name, self.quote);
        if found.directory {
            // クォートは開いたままにして、続けて中へ入力できるようにする
            label.push('/');
            buffer.push('/');
        } else {
            match self.quote {
                Quote::None => {}
                Quote::Single => buffer.push('\''),
                Quote::Double => buffer.push('"'),
            }
            buffer.push(' ');
        }
        PathCandidate { label, buffer }
    }
}

struct Found {
    /// ディスク上の名前
    name: String,
    normalized: String,
    folded: String,
    directory: bool,
    /// 大文字小文字まで一致する前方一致
    exact: bool,
}

impl Found {
    fn order(a: &Self, b: &Self) -> Ordering {
        b.exact
            .cmp(&a.exact)
            .then(b.directory.cmp(&a.directory))
            .then_with(|| a.folded.cmp(&b.folded))
            .then_with(|| a.name.cmp(&b.name))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

enum Piece {
    /// 論理文字 1 つ。`end` は BUFFER 上で生テキストが終わるバイト位置
    Lit {
        ch: char,
        end: usize,
        quote: Quote,
        escaped: bool,
    },
    /// `$NAME` / `${NAME}`
    Var(String),
}

impl Piece {
    fn is_char(&self, c: char) -> bool {
        matches!(*self, Piece::Lit { ch, .. } if ch == c)
    }

    /// クォート外かつエスケープされていない `c` か
    fn is_bare(&self, c: char) -> bool {
        matches!(
            *self,
            Piece::Lit { ch, quote: Quote::None, escaped: false, .. } if ch == c
        )
    }
}

#[derive(Default)]
struct Word {
    pieces: Vec<Piece>,
    /// 展開を再現できない構文を含む
    unsupported: bool,
}

impl Word {
    fn push(&mut self, ch: char, end: usize, quote: Quote, escaped: bool) {
        self.pieces.push(Piece::Lit {
            ch,
            end,
            quote,
            escaped,
        });
    }

    /// 展開を含まない語の論理テキスト
    fn text(&self) -> Option<String> {
        if self.unsupported {
            return None;
        }
        self.pieces
            .iter()
            .map(|piece| match piece {
                Piece::Lit { ch, .. } => Some(*ch),
                Piece::Var(_) => None,
            })
            .collect()
    }

    /// クォート外の `NAME=` で始まる代入語か
    fn is_assignment(&self) -> bool {
        let mut name = String::new();
        for piece in &self.pieces {
            match *piece {
                Piece::Lit {
                    ch: '=',
                    quote: Quote::None,
                    escaped: false,
                    ..
                } => return is_name(&name),
                Piece::Lit {
                    ch,
                    quote: Quote::None,
                    escaped: false,
                    ..
                } => name.push(ch),
                _ => return false,
            }
        }
        false
    }
}

/// BUFFER を文字単位で読む
struct Text {
    chars: Vec<(usize, char)>,
    len: usize,
}

impl Text {
    fn new(buffer: &str) -> Self {
        Self {
            chars: buffer.char_indices().collect(),
            len: buffer.len(),
        }
    }

    fn get(&self, i: usize) -> Option<char> {
        self.chars.get(i).map(|&(_, c)| c)
    }

    /// `i` 文字目の開始バイト位置 (範囲外は BUFFER 長)
    fn offset(&self, i: usize) -> usize {
        self.chars.get(i).map_or(self.len, |&(offset, _)| offset)
    }

    fn count(&self) -> usize {
        self.chars.len()
    }

    fn collect(&self, range: Range<usize>) -> String {
        range.filter_map(|i| self.get(i)).collect()
    }
}

/// 次の語がコマンド位置かを追跡する
struct Position {
    command: bool,
    /// 直前の語がプレフィックスコマンド
    after_prefix: bool,
    /// リダイレクト先の語を読み終えたら戻す状態
    redirect: Option<(bool, bool)>,
}

impl Position {
    fn is_command(&self) -> bool {
        self.command && self.redirect.is_none()
    }

    fn end_word(&mut self, word: &Word) {
        if let Some((command, after_prefix)) = self.redirect.take() {
            self.command = command;
            self.after_prefix = after_prefix;
            return;
        }
        if !self.command {
            return;
        }
        // プレフィックス直後のオプションはコマンド位置を消費する
        let option =
            self.after_prefix && word.pieces.first().is_some_and(|piece| piece.is_char('-'));
        let prefix = !option && word.text().is_some_and(|text| is_prefix_word(&text));
        self.command = !option && (prefix || word.is_assignment());
        self.after_prefix = prefix;
    }

    /// `i` 文字目から演算子を読み、次の位置を返す
    fn operator(&mut self, text: &Text, i: usize) -> usize {
        let c = text.get(i);
        let redirect =
            matches!(c, Some('<' | '>')) || (c == Some('&') && text.get(i + 1) == Some('>'));
        if !redirect {
            self.command = c != Some(')');
            self.after_prefix = false;
            self.redirect = None;
            return i + 1;
        }
        // `>>` `>&` `&>` `>|` `<<<` などは 1 つのリダイレクト演算子として読む
        let mut j = i + 1;
        while matches!(text.get(j), Some('<' | '>' | '&' | '|')) {
            j += 1;
        }
        // zsh の `>!` / `>>!`
        if text.get(j - 1) == Some('>') && text.get(j) == Some('!') {
            j += 1;
        }
        if self.redirect.is_none() {
            self.redirect = Some((self.command, self.after_prefix));
        }
        j
    }
}

/// BUFFER の末尾で終わる最後の語と、それがコマンド位置にあるかを返す
fn last_word(buffer: &str) -> Option<(Word, bool)> {
    let text = Text::new(buffer);
    let mut position = Position {
        command: true,
        after_prefix: false,
        redirect: None,
    };
    let mut word: Option<Word> = None;
    let mut quote = Quote::None;
    let mut i = 0;
    while let Some(c) = text.get(i) {
        let end = text.offset(i + 1);
        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.get_or_insert_default()
                        .push(c, end, Quote::Single, false);
                }
                i += 1;
            }
            Quote::Double => {
                let current = word.get_or_insert_default();
                match (c, text.get(i + 1)) {
                    ('"', _) => {
                        quote = Quote::None;
                        i += 1;
                    }
                    // 対話モードの zsh では `\!` もエスケープになる
                    ('\\', Some(next @ ('$' | '`' | '"' | '\\' | '!'))) => {
                        current.push(next, text.offset(i + 2), Quote::Double, true);
                        i += 2;
                    }
                    ('$', _) => i = read_dollar(&text, i, Quote::Double, current)?,
                    ('`', _) => {
                        current.unsupported = true;
                        i = skip_quoted(&text, i + 1, '`')?;
                    }
                    _ => {
                        current.push(c, end, Quote::Double, false);
                        i += 1;
                    }
                }
            }
            Quote::None => match c {
                _ if is_blank(c) || is_operator(c) => {
                    if let Some(done) = word.take() {
                        position.end_word(&done);
                    }
                    i = if is_blank(c) {
                        i + 1
                    } else {
                        position.operator(&text, i)
                    };
                }
                '\'' | '"' => {
                    word.get_or_insert_default();
                    quote = if c == '\'' {
                        Quote::Single
                    } else {
                        Quote::Double
                    };
                    i += 1;
                }
                '\\' => {
                    // エスケープ対象の無い末尾の `\` は対象外
                    let next = text.get(i + 1)?;
                    word.get_or_insert_default()
                        .push(next, text.offset(i + 2), Quote::None, true);
                    i += 2;
                }
                '$' => i = read_dollar(&text, i, Quote::None, word.get_or_insert_default())?,
                '`' => {
                    word.get_or_insert_default().unsupported = true;
                    i = skip_quoted(&text, i + 1, '`')?;
                }
                _ => {
                    word.get_or_insert_default()
                        .push(c, end, Quote::None, false);
                    i += 1;
                }
            },
        }
    }
    let command = position.is_command();
    word.map(|word| (word, command))
}

/// `i` 文字目の `$` から始まる展開を読み、次の位置を返す。閉じていないコマンド置換は None。
fn read_dollar(text: &Text, i: usize, quote: Quote, word: &mut Word) -> Option<usize> {
    match text.get(i + 1) {
        Some('(') => {
            word.unsupported = true;
            skip_substitution(text, i + 2)
        }
        Some('{') => {
            let Some(close) = (i + 2..text.count()).find(|&j| text.get(j) == Some('}')) else {
                word.unsupported = true;
                return Some(text.count());
            };
            let name = text.collect(i + 2..close);
            if is_name(&name) {
                word.pieces.push(Piece::Var(name));
            } else {
                word.unsupported = true;
            }
            Some(close + 1)
        }
        // ANSI-C クォート
        Some('\'') if quote == Quote::None => {
            word.unsupported = true;
            let mut j = i + 2;
            while let Some(c) = text.get(j) {
                match c {
                    '\\' => j += 2,
                    '\'' => return Some(j + 1),
                    _ => j += 1,
                }
            }
            Some(text.count())
        }
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {
            let mut j = i + 2;
            while text
                .get(j)
                .is_some_and(|c| c == '_' || c.is_ascii_alphanumeric())
            {
                j += 1;
            }
            word.pieces.push(Piece::Var(text.collect(i + 1..j)));
            Some(j)
        }
        // `$1` `$?` `$#` などの特殊パラメータ
        Some(c) if !is_blank(c) && !is_operator(c) && !matches!(c, '\'' | '"' | '\\' | '`') => {
            word.unsupported = true;
            Some(i + 2)
        }
        // 単独の `$` (区切りやクォートは読み進めない)
        _ => {
            word.unsupported = true;
            Some(i + 1)
        }
    }
}

/// `$(` の中身を読み飛ばし、対応する `)` の次の位置を返す。閉じていなければ None。
fn skip_substitution(text: &Text, mut i: usize) -> Option<usize> {
    let mut depth = 1;
    while let Some(c) = text.get(i) {
        i = match c {
            '\\' => i + 2,
            '\'' => (i + 1..text.count()).find(|&j| text.get(j) == Some('\''))? + 1,
            '"' | '`' => skip_quoted(text, i + 1, c)?,
            '(' => {
                depth += 1;
                i + 1
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
                i + 1
            }
            _ => i + 1,
        };
    }
    None
}

/// `\` のエスケープを除いて `close` を探し、その次の位置を返す。閉じていなければ None。
fn skip_quoted(text: &Text, mut i: usize, close: char) -> Option<usize> {
    while let Some(c) = text.get(i) {
        if c == close {
            return Some(i + 1);
        }
        i += if c == '\\' { 2 } else { 1 };
    }
    None
}

/// `--opt=` `-o=` `NAME=` で始まる語は `=` の後ろからをパスとする
fn path_start(pieces: &[Piece]) -> usize {
    let mut name = String::new();
    // 名前の部分もクォート・エスケープされていないこと (`absent\=./dir` はファイル名の `=`)
    let mut plain = true;
    for (index, piece) in pieces.iter().enumerate() {
        let Piece::Lit {
            ch, quote, escaped, ..
        } = *piece
        else {
            return 0;
        };
        let bare = quote == Quote::None && !escaped;
        if ch == '=' && bare {
            let key = plain && (is_name(&name) || is_option_name(&name));
            return if key { index + 1 } else { 0 };
        }
        plain &= bare;
        name.push(ch);
    }
    0
}

/// `[A-Za-z_][A-Za-z0-9_]*`
fn is_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// `--?[A-Za-z0-9][A-Za-z0-9_-]*`
fn is_option_name(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("--").or_else(|| text.strip_prefix('-')) else {
        return false;
    };
    let mut chars = rest.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// 後続の語もコマンド位置として扱う語
fn is_prefix_word(word: &str) -> bool {
    matches!(
        word,
        "sudo"
            | "env"
            | "command"
            | "builtin"
            | "exec"
            | "noglob"
            | "nocorrect"
            | "time"
            | "nohup"
            | "if"
            | "then"
            | "else"
            | "elif"
            | "do"
            | "while"
            | "until"
            | "!"
            | "{"
    )
}

/// 語の区切り。zsh と同じく全角空白などは語の一部として扱う
fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n')
}

fn is_operator(c: char) -> bool {
    matches!(c, '|' | '&' | ';' | '<' | '>' | '(' | ')')
}

/// クォート文脈に合わせて名前をエスケープする (zsh 標準補完と同じ表記)
fn push_escaped(out: &mut String, name: &str, quote: Quote) {
    for c in name.chars() {
        match quote {
            Quote::None
                if matches!(
                    c,
                    ' ' | '\\'
                        | '\''
                        | '"'
                        | '`'
                        | '$'
                        | '!'
                        | '&'
                        | '|'
                        | ';'
                        | '<'
                        | '>'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '*'
                        | '?'
                        | '#'
                        | '~'
                        | '^'
                ) =>
            {
                out.push('\\');
            }
            Quote::Double if matches!(c, '"' | '\\' | '$' | '`' | '!') => out.push('\\'),
            Quote::Single if c == '\'' => {
                out.push_str("'\\''");
                continue;
            }
            _ => {}
        }
        out.push(c);
    }
}

fn nfc(text: &str) -> String {
    if text.is_ascii() {
        text.to_owned()
    } else {
        text.nfc().collect()
    }
}

/// symlink は辿って判定し、壊れた symlink はファイル扱いにする
fn is_directory(entry: &fs::DirEntry) -> bool {
    match entry.file_type() {
        Ok(kind) if !kind.is_symlink() => kind.is_dir(),
        _ => fs::metadata(entry.path()).is_ok_and(|metadata| metadata.is_dir()),
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn no_var(_: &str) -> Option<OsString> {
        None
    }

    /// 環境を注入して同期的に一覧を得る
    fn list_with(buffer: &str, limit: usize, env: &Env) -> Option<PathListing> {
        prepare(buffer, limit, env)?.scan()
    }

    /// cwd と HOME を `root` にして一覧を得る
    fn list_in(root: &Path, buffer: &str) -> Option<PathListing> {
        let env = Env {
            cwd: root,
            home: Some(root),
            var: &no_var,
        };
        list_with(buffer, 256, &env)
    }

    fn candidate(label: &str, buffer: &str) -> PathCandidate {
        PathCandidate {
            label: label.to_owned(),
            buffer: buffer.to_owned(),
        }
    }

    fn labels(listing: &PathListing) -> Vec<&str> {
        listing
            .candidates
            .iter()
            .map(|candidate| candidate.label.as_str())
            .collect()
    }

    fn buffers(listing: &PathListing) -> Vec<&str> {
        listing
            .candidates
            .iter()
            .map(|candidate| candidate.buffer.as_str())
            .collect()
    }

    fn touch(path: &Path) {
        fs::write(path, "").unwrap();
    }

    #[cfg(unix)]
    fn executable(path: &Path) {
        fs::write(path, "#!/bin/sh\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// 対話モードの zsh に各行を入力し、実行後の `$#:$1` を行ごとに返す
    #[cfg(unix)]
    fn run_in_zsh(lines: &[String]) -> Vec<String> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        // `"\!"` の `\` はヒストリ展開が有効な対話モードでだけ外れるため `-i` で読ませる
        let mut child = Command::new("zsh")
            .args(["-f", "-i"])
            .env("LC_ALL", "en_US.UTF-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        for line in lines {
            writeln!(stdin, "{line}; print -r -- \"$#:$1\"").unwrap();
        }
        drop(stdin);
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn チルダ配下の全エントリをディレクトリ優先で返す() {
        let home = tempfile::tempdir().unwrap();
        let books = home.path().join("books");
        fs::create_dir_all(books.join("Zeta")).unwrap();
        fs::create_dir_all(books.join("sub dir")).unwrap();
        touch(&books.join("alpha.md"));
        touch(&books.join("Beta.txt"));
        touch(&books.join(".hidden"));
        let cwd = tempfile::tempdir().unwrap();
        let env = Env {
            cwd: cwd.path(),
            home: Some(home.path()),
            var: &no_var,
        };
        assert_eq!(
            list_with("ls -l ~/books/", 256, &env),
            Some(PathListing {
                candidates: vec![
                    candidate("sub dir/", "ls -l ~/books/sub\\ dir/"),
                    candidate("Zeta/", "ls -l ~/books/Zeta/"),
                    candidate("alpha.md", "ls -l ~/books/alpha.md "),
                    candidate("Beta.txt", "ls -l ~/books/Beta.txt "),
                ],
                total: 4,
                complete: true,
            })
        );
        let limited = list_with("ls -l ~/books/", 3, &env).unwrap();
        assert_eq!(labels(&limited), ["sub dir/", "Zeta/", "alpha.md"]);
        assert_eq!((limited.total, limited.complete), (4, true));
        // 隠しファイルは `.` から入力したときだけ出す
        assert_eq!(
            labels(&list_with("ls -l ~/books/.", 256, &env).unwrap()),
            [".hidden"]
        );
        assert_eq!(list_with("ls -l ~/books/", 0, &env), None);
        // HOME が分からなければ `~` を展開できない
        let no_home = Env { home: None, ..env };
        assert_eq!(list_with("ls -l ~/books/", 256, &no_home), None);
    }

    #[test]
    fn 大文字小文字を無視して前方一致し表記まで一致するものを先に並べる() {
        let tmp = tempfile::tempdir().unwrap();
        let books = tmp.path().join("books");
        fs::create_dir_all(books.join("ALPINE")).unwrap();
        touch(&books.join("Alpha Beta.txt"));
        touch(&books.join("alpha.md"));
        touch(&books.join("beta"));
        assert_eq!(
            list_in(tmp.path(), "ls books/al").unwrap(),
            PathListing {
                candidates: vec![
                    candidate("alpha.md", "ls books/alpha.md "),
                    candidate("ALPINE/", "ls books/ALPINE/"),
                    candidate("Alpha Beta.txt", "ls books/Alpha\\ Beta.txt "),
                ],
                total: 3,
                complete: true,
            }
        );
    }

    #[test]
    fn nfdのファイル名をnfcの入力で拾いディスク上の名前で挿入する() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("資料");
        fs::create_dir(&docs).unwrap();
        let file: String = "パンフレット.pdf".nfd().collect();
        let dir: String = "ガイド".nfd().collect();
        assert_ne!(file, "パンフレット.pdf");
        touch(&docs.join(&file));
        fs::create_dir(docs.join(&dir)).unwrap();
        assert_eq!(
            list_in(tmp.path(), "ls 資料/パ").unwrap().candidates,
            [candidate("パンフレット.pdf", &format!("ls 資料/{file} "))]
        );
        // NFD で入力されても同じ候補になる
        let typed: String = "ls 資料/ガ".nfd().collect();
        assert_eq!(
            list_in(tmp.path(), &typed).unwrap().candidates,
            [candidate("ガイド/", &format!("ls 資料/{dir}/"))]
        );
    }

    #[cfg(unix)]
    #[test]
    fn 生成したbufferを対話モードのzshが元の名前に戻す() {
        let tmp = tempfile::tempdir().unwrap();
        let qt = tmp.path().join("qt");
        fs::create_dir(&qt).unwrap();
        let mut files: Vec<String> = [
            "bar baz.txt",
            "x!y.txt",
            "a~b",
            "cost$HOME.txt",
            "it's.txt",
            "say \"hi\".txt",
            "paren(1).txt",
            "br[a]c{e}.txt",
            "#hash.txt",
            "a&b;c|d<e>f.txt",
            "back\\slash.txt",
            "tick`s.txt",
            "star*q?.txt",
            "caret^.txt",
            "eq=pct%,.txt",
            "日本語 ファイル.txt",
            "全角　空白（括弧）！.txt",
        ]
        .map(str::to_owned)
        .to_vec();
        files.push("パ行.txt".nfd().collect());
        let dirs = ["dir one", "d'q\"$!"];
        for name in &files {
            touch(&qt.join(name));
        }
        for name in dirs {
            fs::create_dir(qt.join(name)).unwrap();
            touch(&qt.join(name).join("inner.txt"));
        }
        let env = Env {
            cwd: tmp.path(),
            home: None,
            var: &no_var,
        };
        let mut lines = Vec::new();
        let mut expected = Vec::new();
        for (open, close) in [("", ""), ("\"", "\""), ("'", "'")] {
            let listing = list_in(tmp.path(), &format!("set -- {open}qt/")).unwrap();
            assert_eq!(listing.total, files.len() + dirs.len());
            for candidate in &listing.candidates {
                let label = candidate.label.trim_end_matches('/');
                let name = files
                    .iter()
                    .map(String::as_str)
                    .chain(dirs)
                    .find(|name| nfc(name) == label)
                    .unwrap();
                if candidate.label.ends_with('/') {
                    // 自前の字句解析でもそのディレクトリの中へ進める
                    let inner = list_in(tmp.path(), &candidate.buffer).unwrap();
                    assert_eq!(
                        buffers(&inner),
                        [format!("{}inner.txt{close} ", candidate.buffer)]
                    );
                    lines.push(format!("{}{close}", candidate.buffer));
                    expected.push(format!("1:qt/{name}/"));
                } else {
                    // 挿入した語を自前の字句解析に戻すと同じ名前になる
                    let typed = candidate.buffer.strip_suffix(&format!("{close} ")).unwrap();
                    let request = prepare(typed, 256, &env).unwrap();
                    assert_eq!((request.dir, request.base.as_str()), (qt.clone(), name));
                    lines.push(candidate.buffer.clone());
                    expected.push(format!("1:qt/{name}"));
                }
            }
        }
        assert_eq!(run_in_zsh(&lines), expected);
    }

    #[test]
    fn クォート文脈に合わせてエスケープし閉じる() {
        let tmp = tempfile::tempdir().unwrap();
        let qt = tmp.path().join("qt");
        fs::create_dir_all(qt.join("dir one")).unwrap();
        for name in ["bar baz.txt", "x!y.txt", "a~b", "it's.txt"] {
            touch(&qt.join(name));
        }
        for (input, expected) in [
            ("ls \"qt/bar", "ls \"qt/bar baz.txt\" "),
            ("ls \"qt/di", "ls \"qt/dir one/"),
            ("ls qt/x", "ls qt/x\\!y.txt "),
            ("ls \"qt/x", "ls \"qt/x\\!y.txt\" "),
            ("ls qt/a", "ls qt/a\\~b "),
            ("ls 'qt/ba", "ls 'qt/bar baz.txt' "),
            ("ls 'qt/it", "ls 'qt/it'\\''s.txt' "),
            ("ls 'qt/di", "ls 'qt/dir one/"),
            // クォートが basename の途中から始まるなら、クォート外の表記に置き換える
            ("ls qt/\"ba", "ls qt/bar\\ baz.txt "),
            ("ls \"qt\"/ba", "ls \"qt\"/bar\\ baz.txt "),
            // `/` がクォート内なら、そのクォートの中として続ける
            ("ls 'qt/'ba", "ls 'qt/bar baz.txt' "),
            ("ls \"qt/\"b", "ls \"qt/bar baz.txt\" "),
            ("ls \"qt/\"", "ls \"qt/dir one/"),
        ] {
            assert_eq!(
                buffers(&list_in(tmp.path(), input).unwrap())[0],
                expected,
                "{input}"
            );
        }
    }

    #[test]
    fn 等号の後ろをパスとして扱う() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("dir");
        fs::create_dir(&dir).unwrap();
        touch(&dir.join("conf.toml"));
        for (input, expected) in [
            ("cmd --out=./dir/", "cmd --out=./dir/conf.toml "),
            ("cmd -o=./dir/c", "cmd -o=./dir/conf.toml "),
            ("cmd --out=~/dir/", "cmd --out=~/dir/conf.toml "),
            // コマンド位置の代入語でも `=` の後ろは実行ファイルに絞らない
            ("VAR=./dir/", "VAR=./dir/conf.toml "),
            ("make VAR=./dir/", "make VAR=./dir/conf.toml "),
            ("dd if=./dir/", "dd if=./dir/conf.toml "),
        ] {
            assert_eq!(
                buffers(&list_in(tmp.path(), input).unwrap()),
                [expected],
                "{input}"
            );
        }
        // クォートされた `=` や名前の形でない語は分割しない
        assert_eq!(list_in(tmp.path(), "ls a'='./dir/"), None);
        assert_eq!(list_in(tmp.path(), "ls ./x=./dir/"), None);
    }

    #[cfg(unix)]
    #[test]
    fn リダイレクト先はコマンド位置として扱わない() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("dir/sub")).unwrap();
        touch(&root.join("dir/file.txt"));
        executable(&root.join("dir/run.sh"));
        let all = ["dir/sub/", "dir/file.txt ", "dir/run.sh "];
        for head in [
            "cat <./",
            "cat < ./",
            "echo x >./",
            "echo x >> ./",
            "echo x >| ./",
            "echo x >! ./",
            "echo x &>./",
            "echo x 2>&1 ./",
            "echo x > log ./",
            "; >./",
        ] {
            assert_eq!(
                buffers(&list_in(root, &format!("{head}dir/")).unwrap()),
                all.map(|tail| format!("{head}{tail}")),
                "{head}"
            );
        }
        assert_eq!(
            buffers(&list_in(root, "echo x >./dir/f").unwrap()),
            ["echo x >./dir/file.txt "]
        );
        // リダイレクト先を読み終えたらリダイレクト前の状態に戻る
        assert_eq!(
            buffers(&list_in(root, ">log ./dir/").unwrap()),
            [">log ./dir/sub/", ">log ./dir/run.sh "]
        );
    }

    #[cfg(unix)]
    #[test]
    fn コマンド位置では実行可能ファイルとディレクトリだけを返す() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("bin")).unwrap();
        executable(&root.join("run.sh"));
        touch(&root.join("data.txt"));
        for head in [
            "",
            "sudo ",
            "FOO=1 ",
            "FOO=$(pwd) ",
            "env FOO=1 ",
            "sudo env ",
            "echo a; ",
            "echo a | ",
            "echo a && ",
            "echo a & ",
            "(",
            "if ",
            "then ",
            "time ",
            "nohup ",
            "{ ",
            "! ",
            "~/",
        ] {
            assert_eq!(
                buffers(&list_in(root, &format!("{head}./")).unwrap()),
                [format!("{head}./bin/"), format!("{head}./run.sh ")],
                "{head}"
            );
        }
        for head in [
            "ls ",
            "echo a ",
            "sudo -E ",
            "$EDITOR ",
            "\"FOO\"=1 ",
            "(echo a) ",
        ] {
            assert_eq!(
                buffers(&list_in(root, &format!("{head}./")).unwrap()),
                [
                    format!("{head}./bin/"),
                    format!("{head}./data.txt "),
                    format!("{head}./run.sh ")
                ],
                "{head}"
            );
        }
    }

    #[test]
    fn 変数を展開して走査し元の表記を保つ() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("dir")).unwrap();
        touch(&root.join("dir/file.txt"));
        let var = |name: &str| match name {
            "HOME" => Some(root.as_os_str().to_owned()),
            "EMPTY" => Some(OsString::new()),
            _ => None,
        };
        let cwd = tempfile::tempdir().unwrap();
        let env = Env {
            cwd: cwd.path(),
            home: None,
            var: &var,
        };
        for (input, expected) in [
            ("ls $HOME/dir/", "ls $HOME/dir/file.txt "),
            ("ls ${HOME}/dir/", "ls ${HOME}/dir/file.txt "),
            ("ls \"$HOME/dir/", "ls \"$HOME/dir/file.txt\" "),
            ("ls \"${HOME}\"/dir/f", "ls \"${HOME}\"/dir/file.txt "),
            ("ls $HOME/${EMPTY}dir/", "ls $HOME/${EMPTY}dir/file.txt "),
        ] {
            assert_eq!(
                buffers(&list_with(input, 256, &env).unwrap()),
                [expected],
                "{input}"
            );
        }
        for input in [
            "ls $UNSET/dir/",
            "ls $1/",
            "ls $?/",
            "ls ${#HOME}/",
            "ls ${HOME:-x}/",
            "ls $HOME/dir/$HOME",
            "ls ${HOME",
            "ls $",
            "ls $HOME/dir/$",
        ] {
            assert_eq!(list_with(input, 256, &env), None, "{input}");
        }
    }

    #[test]
    fn 対象外の入力はnoneを返す() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("dir")).unwrap();
        touch(&root.join("dir/file.txt"));
        for input in [
            "",
            "ls",
            "ls dir",
            "ls ~",
            "ls dir/ ",
            "ls dir/|",
            "ls dir/ ;",
            "ls dir/*",
            "ls dir/f*",
            "ls di?/",
            "ls [d]ir/",
            "ls {dir,x}/",
            "ls dir/{a,b}",
            "ls $(pwd)/",
            "ls \"$(pwd)/",
            "ls \"$(pwd)\"/dir/",
            "ls `pwd`/",
            "ls $'dir'/",
            "ls ~user/",
            "ls ~+/",
            "ls ~-/",
            "ls ~1/",
            "ls =dir/",
            "ls dir/\\",
            "ls \\",
            "ls dir/\n",
            "ls\tdir/",
            "ls dir/\u{7f}",
            "ls nope/",
            "ls dir/file.txt/",
            "ls $(pwd",
            "ls $(echo ')' ",
            "ls `pwd",
            "ls \"`pwd",
        ] {
            assert_eq!(list_in(root, input), None, "{input:?}");
        }
        // 前の語の置換やクォートは最後の語に影響しない
        for input in [
            "ls $(pwd) dir/",
            "echo $((1 + (2))) dir/",
            "echo \"a b\" 'c' `d` dir/",
            "ls \\  dir/",
        ] {
            assert_eq!(
                labels(&list_in(root, input).unwrap()),
                ["file.txt"],
                "{input}"
            );
        }
    }

    #[test]
    fn 完全一致したファイルは除きディレクトリは残す() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("Documents")).unwrap();
        fs::create_dir(root.join("dir")).unwrap();
        touch(&root.join("dir/file.txt"));
        touch(&root.join("dir/file.txt.bak"));
        assert_eq!(
            buffers(&list_in(root, "ls dir/file.txt").unwrap()),
            ["ls dir/file.txt.bak "]
        );
        assert_eq!(
            list_in(root, "ls ~/Documents").unwrap().candidates,
            [candidate("Documents/", "ls ~/Documents/")]
        );
        // 表記だけ違うファイルは挿入で表記が直るため残す
        assert_eq!(
            buffers(&list_in(root, "ls dir/FILE.TXT").unwrap()),
            ["ls dir/file.txt ", "ls dir/file.txt.bak "]
        );
        assert_eq!(
            list_in(root, "ls dir/zzz"),
            Some(PathListing {
                candidates: vec![],
                total: 0,
                complete: true,
            })
        );
        // 開いたクォートを閉じる候補は、名前が完全一致していても残す
        assert_eq!(
            buffers(&list_in(root, "ls \"dir/file.txt").unwrap()),
            ["ls \"dir/file.txt\" ", "ls \"dir/file.txt.bak\" "]
        );
        // NFC で打った名前を、ディスク上の NFD の表記へ直す候補も残す
        let nfd = "\u{30cf}\u{309a}\u{30f3}.txt";
        touch(&root.join("dir").join(nfd));
        assert_eq!(
            buffers(&list_in(root, "ls dir/\u{30d1}\u{30f3}.txt").unwrap()),
            [format!("ls dir/{nfd} ")]
        );
    }

    #[test]
    fn エスケープやクォートされた等号は区切りにしない() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        touch(&root.join("src/main.rs"));
        // `absent=.` というディレクトリ配下を指すので、`./src/` を一覧にしない
        assert_eq!(list_in(root, "cat absent\\=./src/ma"), None);
        assert_eq!(list_in(root, "cat 'absent='./src/ma"), None);
        assert_eq!(
            buffers(&list_in(root, "cat --in=./src/ma").unwrap()),
            ["cat --in=./src/main.rs "]
        );
    }

    #[cfg(unix)]
    #[test]
    fn ディレクトリへのsymlinkはディレクトリとして扱う() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("real")).unwrap();
        executable(&root.join("real/tool"));
        symlink(root.join("real"), root.join("link")).unwrap();
        symlink(root.join("missing"), root.join("broken")).unwrap();
        symlink(root.join("real/tool"), root.join("tool-link")).unwrap();
        assert_eq!(
            list_in(root, "ls ./l").unwrap().candidates,
            [candidate("link/", "ls ./link/")]
        );
        assert_eq!(buffers(&list_in(root, "ls ./b").unwrap()), ["ls ./broken "]);
        // コマンド位置では辿った先で判定し、壊れた symlink は除く
        assert_eq!(
            buffers(&list_in(root, "./").unwrap()),
            ["./link/", "./real/", "./tool-link "]
        );
    }

    #[cfg(unix)]
    #[test]
    fn 制御文字やutf8でない名前は候補にしない() {
        use std::os::unix::ffi::OsStrExt;

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("dir");
        fs::create_dir(&dir).unwrap();
        touch(&dir.join("bad\nname"));
        touch(&dir.join("tab\tname"));
        touch(&dir.join("good"));
        // macOS (APFS) は UTF-8 でない名前を作れない
        let _ = fs::write(dir.join(std::ffi::OsStr::from_bytes(b"bad\xffname")), "");
        assert_eq!(
            list_in(tmp.path(), "ls dir/"),
            Some(PathListing {
                candidates: vec![candidate("good", "ls dir/good ")],
                total: 1,
                complete: true,
            })
        );
    }

    #[test]
    fn 走査上限に達したら打ち切りを示す() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["a1", "a2", "a3", "a4"] {
            touch(&tmp.path().join(name));
        }
        let env = Env {
            cwd: tmp.path(),
            home: None,
            var: &no_var,
        };
        let request = prepare("ls ./a", 256, &env).unwrap();
        let listing = request.scan_with(2, SCAN_BUDGET).unwrap();
        assert_eq!(
            (listing.candidates.len(), listing.total, listing.complete),
            (2, 2, false)
        );
        let listing = request.scan_with(4, SCAN_BUDGET).unwrap();
        assert_eq!((listing.total, listing.complete), (4, true));
    }

    #[test]
    fn 実環境版は対象外の入力でnoneを返す() {
        assert_eq!(list("ls", 256), None);
        assert_eq!(list("ls /", 0), None);
        assert_eq!(list("ls $(pwd", 256), None);
        assert_eq!(list("ls /zsh-turbo-nonexistent-dir/", 256), None);
    }

    #[test]
    fn 実環境版は別スレッドで走査した結果を返す() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("entry.txt"));
        let Some(dir) = tmp.path().to_str() else {
            return;
        };
        let mut buffer = "ls ".to_owned();
        push_escaped(&mut buffer, dir, Quote::None);
        buffer.push('/');
        assert_eq!(
            list(&buffer, 256).unwrap().candidates,
            [candidate("entry.txt", &format!("{buffer}entry.txt "))]
        );
    }
}
