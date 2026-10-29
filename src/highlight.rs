use crate::executable::is_executable_file;
use crate::shell_words::{is_blank, numeric_glob_len};
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// ハイライトスパン: 開始位置、終了位置、スタイル文字列
#[derive(Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub style: String,
}

/// zsh から受け取るコマンド名の一覧 (いずれも改行区切り)。
#[derive(Debug, Default, Clone, Copy)]
pub struct ShellNames<'a> {
    pub aliases: &'a str,
    pub functions: &'a str,
    /// zsh の `${(k)builtins}` (読み込んだモジュールの組み込みを含む)。空なら既定の一覧を使う
    /// (組み込みを渡さない旧版の init.zsh から呼ばれた場合)。
    pub builtins: &'a str,
}

/// 予約語の O(1) 検索用 HashSet（事前計算）。
static RESERVED_SET: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| RESERVED_WORDS.iter().copied().collect());

/// 次の語をコマンド名として分類するかを決める走査状態。
/// フラグを個別に書き換えず、トークンの種類ごとの遷移メソッドだけで更新する
/// (区切りやリダイレクトの後で一部のフラグを落とし忘れると、後続の語を誤分類する)。
#[derive(Debug, Clone, Copy)]
struct CommandPosition {
    is_command_position: bool,
    after_pipe_or_semi: bool,
    // 直前の語が sudo/env 等のプレフィックスコマンドだったか。
    // プレフィックス直後のオプション (`sudo -u` 等) の扱いにのみ使う。
    after_prefix_command: bool,
}

impl CommandPosition {
    fn initial() -> Self {
        Self {
            is_command_position: true,
            after_pipe_or_semi: false,
            after_prefix_command: false,
        }
    }

    fn expects_command(&self) -> bool {
        self.is_command_position || self.after_pipe_or_semi
    }

    fn after_prefix(&self) -> bool {
        self.after_prefix_command
    }

    /// 改行・`;`・`|`・`&` 等の区切りの後は、新しいコマンドが始まる。
    fn separator(&mut self) {
        *self = Self {
            is_command_position: true,
            after_pipe_or_semi: true,
            after_prefix_command: false,
        };
    }

    /// 変数展開や全体がクォートの語など、コマンド名として消費された語の後。
    fn consume_command_name(&mut self) {
        *self = Self {
            is_command_position: false,
            after_pipe_or_semi: false,
            after_prefix_command: false,
        };
    }

    /// リダイレクトの後の語はリダイレクト先で、新しいコマンドではない。
    fn redirect(&mut self) {
        self.consume_command_name();
    }

    /// 代入語の後もコマンド位置が続く。プレフィックス直後の状態は保持する
    /// (`env FOO=bar -u value` の `-u` をオプションとして扱う)。
    fn assignment(&mut self) {
        self.is_command_position = true;
        self.after_pipe_or_semi = false;
    }

    /// コマンド名を分類した後。プレフィックスコマンドや後続がコマンドの予約語なら
    /// 次の語もコマンド位置として扱う。
    fn command_word(&mut self, logical: &str, quoted: bool) {
        self.is_command_position = keeps_command_position(logical, quoted);
        self.after_prefix_command = is_prefix_command(logical);
        self.after_pipe_or_semi = false;
    }
}

/// シェルコマンドラインのトークンを分類し、ハイライトスパンを返す。
pub fn highlight_buffer(buffer: &str, names: ShellNames) -> Vec<Span> {
    if buffer.is_empty() {
        return Vec::new();
    }
    Highlighter::new(buffer, Classifier::new(names, path_dirs())).run()
}

/// 入力 1 件を先頭から走査してスパンを集める。トークンの優先順位は `run` のループに並べる。
struct Highlighter<'a> {
    chars: Vec<char>,
    pos: usize,
    spans: Vec<Span>,
    classifier: Classifier<'a>,
    position: CommandPosition,
    // `( … )` サブシェルの深さ。0 より大きい間は `)` で語を区切る
    subshell_depth: usize,
    // `[[ … ]]` の中か。中の `&&`/`||` は条件の演算子でコマンド区切りではない
    in_condition: bool,
}

impl<'a> Highlighter<'a> {
    fn new(buffer: &str, classifier: Classifier<'a>) -> Self {
        Self {
            chars: buffer.chars().collect(),
            pos: 0,
            spans: Vec::new(),
            classifier,
            position: CommandPosition::initial(),
            subshell_depth: 0,
            in_condition: false,
        }
    }

    fn run(mut self) -> Vec<Span> {
        while self.pos < self.chars.len() {
            let start = self.pos;
            // 各 try_consume_* は対象外なら何も変えずに false を返す。並び順が優先順位になる
            if !(self.try_consume_layout()
                || self.try_consume_variable()
                || self.try_consume_redirect()
                || self.try_consume_operator()
                || self.try_consume_paren())
            {
                self.consume_word();
            }
            debug_assert!(self.pos > start, "{start} 文字目から走査が進まない");
        }
        self.spans
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn push_span(&mut self, start: usize, end: usize, style: impl Into<String>) {
        self.spans.push(Span {
            start,
            end,
            style: style.into(),
        });
    }

    /// クォートセグメントを文字列色スパンとして追加する。
    /// 単語全体のスパンより後に push することで region_highlight 上で優先される。
    fn push_quote_segs(&mut self, segs: &[(usize, usize)]) {
        for &(start, end) in segs {
            self.push_span(start, end, "fg=yellow");
        }
    }

    /// 行継続・改行・空白を読み進める。
    fn try_consume_layout(&mut self) -> bool {
        match self.chars[self.pos] {
            // `\` + 改行は行継続。zsh は両方を取り除くため、区切りにも語にもしない
            '\\' if self.peek(1) == Some('\n') => self.pos += 2,
            // 改行はシェル構文上のコマンド区切りとして扱う。
            '\n' => {
                self.pos += 1;
                self.position.separator();
            }
            c if is_blank(c) => self.pos += 1,
            _ => return false,
        }
        true
    }

    /// 変数展開 (`$name`・`${…}`・`$?` など) を 1 つのスパンにする。
    fn try_consume_variable(&mut self) -> bool {
        let Some(end) = variable_end(&self.chars, self.pos) else {
            return false;
        };
        self.push_span(self.pos, end, "fg=cyan");
        self.pos = end;
        // 変数展開がコマンド名として消費されるため、区切り直後フラグも落とす。
        self.position.consume_command_name();
        true
    }

    /// リダイレクト: >, >>, <, 2>, &>, 2>&, 0<&
    fn try_consume_redirect(&mut self) -> bool {
        let Some(end) = redirect_end(&self.chars, self.pos) else {
            return false;
        };
        self.push_span(self.pos, end, "fg=magenta");
        self.pos = end;
        // `;`/`|` 直後にリダイレクトが来た場合、次の単語はリダイレクト先
        // (ファイル名等) であって新コマンドではないため、区切り直後の状態も
        // 落とす (例: `; > file nextcmd` の file をコマンドと誤認しない)。
        self.position.redirect();
        true
    }

    /// 演算子: |, ||, &&, ;, &
    fn try_consume_operator(&mut self) -> bool {
        let ch = self.chars[self.pos];
        if !matches!(ch, '|' | '&' | ';') {
            return false;
        }
        let start = self.pos;
        self.pos += 1;
        if self.peek(0) == Some(ch) {
            self.pos += 1; // || または && の 2 文字目
        }
        self.push_span(start, self.pos, "fg=magenta");
        // `[[ … ]]` の中の `&&`/`||` は条件の論理演算子で、次の語はコマンドではない
        if !(self.in_condition && ch != ';') {
            self.position.separator();
        }
        true
    }

    /// コマンド位置の `(` はサブシェル、`((` は算術コマンド。サブシェルの中では `)` で閉じる。
    fn try_consume_paren(&mut self) -> bool {
        match self.chars[self.pos] {
            '(' if self.position.expects_command() => {
                if self.peek(1) == Some('(') {
                    self.consume_arithmetic();
                } else {
                    self.push_span(self.pos, self.pos + 1, "fg=yellow,bold");
                    self.pos += 1;
                    self.subshell_depth += 1;
                    self.position.separator();
                }
            }
            ')' if self.subshell_depth > 0 => {
                self.push_span(self.pos, self.pos + 1, "fg=yellow,bold");
                self.pos += 1;
                self.subshell_depth -= 1;
                self.position.consume_command_name();
            }
            _ => return false,
        }
        true
    }

    /// `((` … `))` の算術コマンド。中身は分類しない。
    fn consume_arithmetic(&mut self) {
        let open = self.pos;
        self.push_span(open, open + 2, "fg=yellow,bold");
        let end = match arithmetic_close(&self.chars, open + 2) {
            Some(close) => {
                self.push_span(close, close + 2, "fg=yellow,bold");
                close + 2
            }
            None => self.chars.len(),
        };
        self.pos = end;
        self.position.consume_command_name();
    }

    /// 通常の単語トークンを読み、分類してスパンを付ける。
    fn consume_word(&mut self) {
        let word = scan_word(
            &self.chars,
            self.pos,
            WordScanContext {
                expects_command: self.position.expects_command(),
                in_subshell: self.subshell_depth > 0,
            },
        );
        self.pos = word.end;
        if self.try_consume_function_definition(&word) || self.try_close_condition(&word) {
            return;
        }
        if self.position.expects_command() {
            self.classify_command_word(&word);
        } else {
            self.classify_argument(&word);
        }
    }

    /// 関数定義 `name()`/`name ()`: 名前は分類せず、`()` の後の本体をコマンド位置にする
    fn try_consume_function_definition(&mut self, word: &Word) -> bool {
        if !self.position.expects_command()
            || word.logical.is_empty()
            || word.logical.ends_with('=')
            || !word.quote_segs.is_empty()
        {
            return false;
        }
        let mut next = word.end;
        while next < self.chars.len() && matches!(self.chars[next], ' ' | '\t') {
            next += 1;
        }
        if self.chars.get(next) != Some(&'(') || self.chars.get(next + 1) != Some(&')') {
            return false;
        }
        self.push_span(next, next + 2, "fg=yellow,bold");
        self.pos = next + 2;
        self.position.separator();
        true
    }

    /// `[[ … ]]` を閉じる `]]`。
    fn try_close_condition(&mut self, word: &Word) -> bool {
        if !self.in_condition || word.logical != "]]" || word.is_quoted() {
            return false;
        }
        self.push_span(word.start, word.end, "fg=yellow,bold");
        self.in_condition = false;
        self.position.consume_command_name();
        true
    }

    /// コマンド位置の語を分類する。
    fn classify_command_word(&mut self, word: &Word) {
        // `!!`・`!$`・`!vim` は対話 zsh (BANG_HIST) の履歴展開。展開後のコマンドは
        // 分からないため分類せず、コマンド名として消費する
        if word.is_history_expansion(&self.chars) {
            self.push_quote_segs(&word.quote_segs);
            self.position.consume_command_name();
            return;
        }

        if word.bare.is_empty() && !word.quote_segs.is_empty() {
            // 全体がクォートの語 (`; 'ls' -la` 等) はコマンド名として消費される。
            // 中身は分類せず文字列色のみ付け、後続をコマンド誤分類しない。
            self.push_quote_segs(&word.quote_segs);
            self.position.consume_command_name();
            return;
        }

        if !word.has_quoted_name(&self.chars) && is_assignment_word(&word.logical) {
            self.push_span(word.start, word.end, "fg=cyan");
            self.push_quote_segs(&word.quote_segs);
            self.position.assignment();
            return;
        }

        if self.position.after_prefix() && word.logical.starts_with('-') {
            // プレフィックスコマンド直後のオプション (`sudo -u` 等) は
            // 未知コマンド (赤) ではなくオプションとして扱う。値付きオプションを
            // 区別できないため、以降はコマンド位置を消費する (誤検出ゼロ優先)。
            self.push_span(word.start, word.end, "fg=cyan");
            self.position.consume_command_name();
            return;
        }

        let quoted = word.is_quoted();
        let style = self.classifier.classify(&word.logical, quoted);
        self.push_span(word.start, word.end, style);
        self.push_quote_segs(&word.quote_segs);
        if !quoted && word.logical == "[[" {
            self.in_condition = true;
        }
        self.position.command_word(&word.logical, quoted);
    }

    /// 引数位置の語を分類する。判定はクォート外の文字で行い、クォートには文字列色を付ける。
    fn classify_argument(&mut self, word: &Word) {
        let style = if word.bare.starts_with('-') {
            // オプション
            Some("fg=cyan")
        } else if word.bare.contains('/')
            || word.bare.starts_with('.')
            || word.bare.starts_with('~')
        {
            // パス風。実在判定はクォートを剥いだ論理語で行う。
            Some(if path_exists(&word.logical) {
                "fg=magenta,underline"
            } else {
                "fg=magenta"
            })
        } else if word.bare.contains(['*', '?', '[', '<']) {
            // glob パターン (クォート内の `*` 等は対象外)
            Some("fg=blue,bold")
        } else {
            // 通常の引数はハイライトなし（端末のデフォルト色）
            None
        };
        if let Some(style) = style {
            self.push_span(word.start, word.end, style);
        }
        self.push_quote_segs(&word.quote_segs);
    }
}

/// 走査した 1 語。位置は文字単位。
#[derive(Debug)]
struct Word {
    start: usize,
    end: usize,
    /// クォートを剥いだ論理語 (コマンド分類・代入判定用)
    logical: String,
    /// クォート外の文字のみ (引数位置でのオプション/パス/glob 判定用)。
    /// エスケープした文字の代わりに `\` を置く
    bare: String,
    /// クォートセグメントの範囲 (文字列色で塗る)
    quote_segs: Vec<(usize, usize)>,
}

impl Word {
    /// 語の一部でもクォート・エスケープされているか (エイリアス展開と予約語解釈を抑止する)。
    fn is_quoted(&self) -> bool {
        !self.quote_segs.is_empty() || self.bare.contains('\\')
    }

    /// 対話 zsh (BANG_HIST) の履歴展開 (`!!`・`!$`・`!vim`) か。`!=` と `!(` は除く。
    fn is_history_expansion(&self, chars: &[char]) -> bool {
        chars[self.start] == '!'
            && self.end - self.start > 1
            && !matches!(chars[self.start + 1], '=' | '(')
    }

    /// 代入の `=` より前 (`=` が無ければ語全体) にクォート・エスケープがあるか。
    fn has_quoted_name(&self, chars: &[char]) -> bool {
        chars[self.start..self.end]
            .iter()
            .take_while(|&&c| c != '=')
            .any(|&c| matches!(c, '\'' | '"' | '\\'))
    }
}

/// 語の走査に使う、走査を始めた時点の文脈。
#[derive(Debug, Clone, Copy)]
struct WordScanContext {
    /// 語がコマンド位置にあるか (配列代入と関数定義の判定に使う)
    expects_command: bool,
    /// サブシェルの中か (中では `)` で語を区切る)
    in_subshell: bool,
}

/// `start` から通常の語を読む。クォートは shell の語結合と同様に単語の一部として
/// 閉じクォートまで消費する (`FOO='a b' cmd` や `ec'ho'` を 1 語に保つ)。
fn scan_word(chars: &[char], start: usize, context: WordScanContext) -> Word {
    let mut word = Word {
        start,
        end: start,
        logical: String::new(),
        bare: String::new(),
        quote_segs: Vec::new(),
    };
    let mut pos = start;
    // 配列代入 `name=( … )` の括弧の深さ。中の空白・改行・演算子は語の一部
    let mut array_depth = 0usize;
    // 語中のグロブの括弧 `(a|b)` の深さ。中の `|` はパイプではなく選択肢
    let mut glob_depth = 0usize;
    while pos < chars.len()
        && (array_depth > 0 || !ends_word(chars, pos, &word.logical, glob_depth, context))
    {
        let c = chars[pos];
        if c == '<'
            && let Some(len) = numeric_glob_len(chars[pos..].iter().copied())
        {
            word.logical.extend(&chars[pos..pos + len]);
            word.bare.extend(&chars[pos..pos + len]);
            pos += len;
            continue;
        }
        if c == '(' {
            if array_depth > 0 || opens_array(chars, start, pos, &word.logical, context) {
                array_depth += 1;
            } else {
                glob_depth += 1;
            }
        } else if c == ')' && array_depth > 0 {
            array_depth -= 1;
        } else if c == ')' && glob_depth > 0 {
            glob_depth -= 1;
        }
        match c {
            // 語中の行継続は取り除いて前後を連結する (`ec\<改行>ho` は echo)
            '\\' if chars.get(pos + 1) == Some(&'\n') => pos += 2,
            '\'' | '"' => {
                let end = if c == '\'' {
                    scan_single_quoted(chars, pos, &mut word.logical)
                } else {
                    scan_double_quoted(chars, pos, &mut word.logical)
                };
                word.quote_segs.push((pos, end));
                pos = end;
            }
            '\\' if pos + 1 < chars.len() => {
                // 先頭の `\cmd` だけはエイリアスを抑止する意味を分類時まで保持する。
                // それ以外のバックスラッシュは shell と同様に論理語から取り除く。
                if pos == start {
                    word.logical.push('\\');
                }
                // エスケープ対象はクォート済みなので glob/option/path 判定から除外する。
                // `\\` を番兵として残し、後続文字が誤って語頭扱いされるのを防ぐ。
                word.bare.push('\\');
                word.logical.push(chars[pos + 1]);
                pos += 2;
            }
            _ => {
                word.logical.push(c);
                word.bare.push(c);
                pos += 1;
            }
        }
    }
    word.end = pos;
    word
}

/// 括弧の外で `pos` の文字が語を終えるか (空白・演算子・リダイレクト・サブシェルの `)`・関数定義の `()`)。
fn ends_word(
    chars: &[char],
    pos: usize,
    logical: &str,
    glob_depth: usize,
    context: WordScanContext,
) -> bool {
    let c = chars[pos];
    is_blank(c)
        || matches!(c, '&' | ';' | '>')
        || (c == '<' && numeric_glob_len(chars[pos..].iter().copied()).is_none())
        || (c == '|' && glob_depth == 0)
        || (c == ')' && context.in_subshell && glob_depth == 0)
        // 関数定義 `name()` の名前はここで切る
        || (c == '('
            && chars.get(pos + 1) == Some(&')')
            && context.expects_command
            && !logical.is_empty()
            && !logical.ends_with('='))
}

/// 語頭から `pos` の `(` の手前までが、クォート・エスケープの無い代入 `NAME=` だけなら
/// 配列代入の始まり。
fn opens_array(
    chars: &[char],
    start: usize,
    pos: usize,
    logical: &str,
    context: WordScanContext,
) -> bool {
    context.expects_command
        && logical.find('=') == Some(logical.len().saturating_sub(1))
        && is_assignment_word(logical)
        && !chars[start..pos]
            .iter()
            .any(|&c| matches!(c, '\'' | '"' | '\\'))
}

/// `start` の `'` から閉じクォートの次 (閉じていなければ末尾) までを読み、中身を論理語に足す。
fn scan_single_quoted(chars: &[char], start: usize, logical: &mut String) -> usize {
    let mut pos = start + 1;
    while pos < chars.len() && chars[pos] != '\'' {
        logical.push(chars[pos]);
        pos += 1;
    }
    if pos < chars.len() {
        pos += 1;
    }
    pos
}

/// `start` の `"` から閉じクォートの次 (閉じていなければ末尾) までを読み、中身を論理語に足す。
fn scan_double_quoted(chars: &[char], start: usize, logical: &mut String) -> usize {
    let mut pos = start + 1;
    while pos < chars.len() && chars[pos] != '"' {
        if chars[pos] == '\\' && pos + 1 < chars.len() {
            let next = chars[pos + 1];
            // 行継続は除き、ダブルクォートでエスケープできる文字だけ `\` を外す。
            if next == '\n' {
                pos += 2;
                continue;
            }
            if matches!(next, '$' | '`' | '"' | '\\' | '!') {
                pos += 1;
            }
        }
        logical.push(chars[pos]);
        pos += 1;
    }
    if pos < chars.len() {
        pos += 1;
    }
    pos
}

/// `start` の `$` から始まる変数展開の終端 (排他) を返す。`$` でないか末尾の `$` なら None。
fn variable_end(chars: &[char], start: usize) -> Option<usize> {
    if chars[start] != '$' || start + 1 >= chars.len() {
        return None;
    }
    let mut pos = start + 1;
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
    Some(pos)
}

/// `start` から始まるリダイレクト (`2>`・`&>`・`>`・`<` など) の終端 (排他) を返す。
/// `<1-10>` などの数値 glob はリダイレクトにしない。
fn redirect_end(chars: &[char], start: usize) -> Option<usize> {
    let c = chars[start];
    // zsh で記述子になるのは 1 桁だけ。`12>file` の `12` はコマンド名や引数になる。
    let fd_end = start + 1;
    if c.is_ascii_digit()
        && fd_end < chars.len()
        && matches!(chars[fd_end], '>' | '<')
        && !(chars[fd_end] == '<' && numeric_glob_len(chars[fd_end..].iter().copied()).is_some())
    {
        return Some(redirect_operator_end(chars, fd_end));
    }
    if c == '&' && chars.get(start + 1) == Some(&'>') {
        let mut pos = start + 2;
        if chars.get(pos) == Some(&'>') {
            pos += 1;
        }
        // `&>|`・`&>!`・`&>>|`・`&>>!` (CLOBBER) の `|`/`!` も演算子の一部
        if matches!(chars.get(pos), Some('|' | '!')) {
            pos += 1;
        }
        return Some(pos);
    }
    if c == '>' || (c == '<' && numeric_glob_len(chars[start..].iter().copied()).is_none()) {
        return Some(redirect_operator_end(chars, start));
    }
    None
}

/// `start` (`((` の直後) から、算術コマンドを閉じる `))` の位置を返す。中の括弧は対応を数える。
fn arithmetic_close(chars: &[char], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (pos, &c) in chars.iter().enumerate().skip(start) {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            ')' if chars.get(pos + 1) == Some(&')') => return Some(pos),
            _ => {}
        }
    }
    None
}

/// 後続の単語をコマンド位置として扱い続ける語。
/// プレフィックスコマンドに加え、後続がコマンドである予約語を含む。
fn keeps_command_position(word: &str, quoted: bool) -> bool {
    is_prefix_command(word)
        || (!quoted
            && matches!(
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
            ))
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
    // `name+=value` は追記代入
    let name = name.strip_suffix('+').unwrap_or(name);
    // `name[添字]=value` は要素への代入
    let name = match name.split_once('[') {
        Some((base, _)) if name.ends_with(']') => base,
        Some(_) => return false,
        None => name,
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

/// `>`/`<` から始まるリダイレクト演算子の終端 (排他) を返す。
/// `>>`・`>&`・`>>&` (標準出力とエラーの追記)・CLOBBER の `|`/`!`、`<&`・`<>` を 1 トークンにする。
fn redirect_operator_end(chars: &[char], start: usize) -> usize {
    let mut pos = start + 1;
    if chars[start] == '>' {
        if chars.get(pos) == Some(&'>') {
            pos += 1;
        }
        if chars.get(pos) == Some(&'&') {
            pos += 1;
        }
        if matches!(chars.get(pos), Some('|' | '!')) {
            pos += 1;
        }
    } else if matches!(chars.get(pos), Some('&' | '>')) {
        pos += 1;
    }
    pos
}

/// zsh が組み込みの一覧を渡さないとき (旧版の init.zsh) に使う既定の組み込みコマンド
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
    "bye",
    "cd",
    "chdir",
    "command",
    "compadd",
    "compctl",
    "compdef",
    "continue",
    "declare",
    "dirs",
    "disable",
    "disown",
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
    "pushln",
    "pwd",
    "r",
    "read",
    "readonly",
    "rehash",
    "return",
    "sched",
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

/// コマンド位置の語を分類する。打鍵ごとに起動されるため、PATH の一覧も全エントリの stat もせず、
/// 分類が必要な名前だけを PATH の各ディレクトリで探し、結果 (見つからなかった場合も) を名前ごとに覚える。
struct Classifier<'a> {
    aliases: HashSet<&'a str>,
    functions: HashSet<&'a str>,
    builtins: HashSet<&'a str>,
    path_dirs: Vec<PathBuf>,
    path_found: HashMap<String, bool>,
}

impl<'a> Classifier<'a> {
    fn new(names: ShellNames<'a>, path_dirs: Vec<PathBuf>) -> Self {
        let lines = |list: &'a str| -> HashSet<&'a str> {
            list.split('\n').filter(|name| !name.is_empty()).collect()
        };
        Self {
            aliases: lines(names.aliases),
            functions: lines(names.functions),
            builtins: if names.builtins.is_empty() {
                BUILTINS.iter().copied().collect()
            } else {
                lines(names.builtins)
            },
            path_dirs,
            path_found: HashMap::new(),
        }
    }

    fn classify(&mut self, word: &str, quoted: bool) -> String {
        // `\cmd` はエイリアス展開・予約語解釈を抑止する zsh の常用イディオム。
        // 分類は `\` を剥いだ語で行うが、予約語・エイリアスには一致させない。
        if let Some(lookup) = word.strip_prefix('\\') {
            if self.builtins.contains(lookup)
                || self.functions.contains(lookup)
                || self.in_path(lookup)
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

        if !quoted && RESERVED_SET.contains(word) {
            return "fg=yellow,bold".into();
        }

        if self.builtins.contains(word) {
            return "fg=green".into();
        }

        if (!quoted && self.aliases.contains(word)) || self.functions.contains(word) {
            return "fg=green".into();
        }

        if self.in_path(word) {
            return "fg=green".into();
        }

        // 実行可能ファイルへのパスか確認 (`~/bin/x` のチルダ展開込み)
        if word.contains('/') && path_exists(word) {
            return "fg=green,underline".into();
        }

        // 不明なコマンド
        "fg=red,bold".into()
    }

    /// PATH の外部コマンドか。zsh と同じく、実行できる通常ファイルがある最初のディレクトリで決まる
    /// (手前の同名ディレクトリ・実行できないファイル・壊れた symlink は飛ばす)。
    fn in_path(&mut self, cmd: &str) -> bool {
        if cmd.is_empty() || cmd.contains('/') {
            return false;
        }
        if let Some(&found) = self.path_found.get(cmd) {
            return found;
        }
        let found = self
            .path_dirs
            .iter()
            .any(|dir| is_executable_file(&dir.join(cmd)));
        self.path_found.insert(cmd.to_owned(), found);
        found
    }
}

/// $PATH の検索ディレクトリ。空要素は zsh と同じくカレントディレクトリを表す。
fn path_dirs() -> Vec<PathBuf> {
    let current_dir = env::current_dir().ok();
    path_dirs_from_in(
        &env::var_os("PATH").unwrap_or_default(),
        current_dir.as_deref(),
    )
}

fn path_dirs_from_in(path_env: &std::ffi::OsStr, current_dir: Option<&Path>) -> Vec<PathBuf> {
    env::split_paths(path_env)
        .filter_map(|dir| {
            if dir.as_os_str().is_empty() {
                current_dir.map(Path::to_path_buf)
            } else {
                Some(dir)
            }
        })
        .collect()
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

    /// テスト補助: 実行可能ファイル `names` を置いた一時ディレクトリを返す
    #[cfg(unix)]
    fn dir_with_executables(names: &[&str]) -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        for name in names {
            let path = dir.path().join(name);
            std::fs::write(&path, "").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    /// テスト補助: `dirs` を PATH として外部コマンドを探す
    fn in_path(cmd: &str, dirs: &[PathBuf]) -> bool {
        Classifier::new(ShellNames::default(), dirs.to_vec()).in_path(cmd)
    }

    #[test]
    fn command_in_path_empty_cmd() {
        assert!(!in_path("", &[]));
    }

    #[cfg(unix)]
    #[test]
    fn command_in_path_with_slash() {
        let dir = dir_with_executables(&["foo"]);
        assert!(!in_path("/usr/bin/foo", &[dir.path().to_path_buf()]));
    }

    #[cfg(unix)]
    #[test]
    fn command_in_path_found() {
        let dir = dir_with_executables(&["rustc"]);
        assert!(in_path("rustc", &[dir.path().to_path_buf()]));
    }

    #[cfg(unix)]
    #[test]
    fn command_in_path_not_found() {
        let dir = dir_with_executables(&["rustc"]);
        assert!(!in_path("nonexistent_xyz", &[dir.path().to_path_buf()]));
    }

    #[cfg(unix)]
    #[test]
    fn build_path_cache_from_実行可能ファイルだけを登録する() {
        use std::os::unix::fs::PermissionsExt;

        let dir = dir_with_executables(&["zt-executable"]);
        let plain = dir.path().join("zt-plain");
        std::fs::write(&plain, "").unwrap();
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();

        let dirs = path_dirs_from_in(dir.path().as_os_str(), None);

        assert!(in_path("zt-executable", &dirs));
        assert!(!in_path("zt-plain", &dirs));
    }

    #[cfg(unix)]
    #[test]
    fn command_in_path_一覧できないディレクトリでもstatで見つける() {
        use std::os::unix::fs::PermissionsExt;

        // PATH を一覧せず、分類する名前だけを stat することを確かめる
        let dir = dir_with_executables(&["zt-hidden"]);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o111)).unwrap();
        let found = in_path("zt-hidden", &[dir.path().to_path_buf()]);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(found);
    }

    #[cfg(unix)]
    #[test]
    fn build_path_cache_from_空のpath要素はカレントディレクトリを探す() {
        let dir = dir_with_executables(&["zt-local-command"]);
        let dirs = path_dirs_from_in(
            std::ffi::OsStr::new(":/path/that/does/not/exist"),
            Some(dir.path()),
        );
        assert!(in_path("zt-local-command", &dirs));
    }

    // ---------------------------------------------------------------
    // バッファ全体のハイライトテスト
    // ---------------------------------------------------------------

    /// テスト補助: トークンテキストとスタイルのペアを収集する。
    fn span_styles(buffer: &str, aliases: &str, functions: &str) -> Vec<(String, String)> {
        let spans = highlight_buffer(
            buffer,
            ShellNames {
                aliases,
                functions,
                ..ShellNames::default()
            },
        );
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
        let spans = highlight_buffer("", ShellNames::default());
        assert!(spans.is_empty());
    }

    #[test]
    fn 全角空白と非改行空白はコマンド名の一部として分類する() {
        for blank in ['\u{3000}', '\u{00a0}', '\u{2003}'] {
            let command = format!("work{blank}tool");
            let buffer = format!("{command} --flag");
            assert_eq!(
                span_styles(&buffer, &command, ""),
                [
                    (command.clone(), "fg=green".into()),
                    ("--flag".into(), "fg=cyan".into())
                ]
            );
            let spans = highlight_buffer(
                &buffer,
                ShellNames {
                    functions: &command,
                    ..ShellNames::default()
                },
            );
            assert_eq!((spans[0].start, spans[0].end), (0, command.chars().count()));
            assert_eq!(spans[0].style, "fg=green");
        }
    }

    #[test]
    fn 半角空白とタブだけで語を区切り改行でコマンド位置を戻す() {
        assert_eq!(
            span_styles("echo\t--first\ntrue --second", "", ""),
            [
                ("echo".into(), "fg=green".into()),
                ("--first".into(), "fg=cyan".into()),
                ("true".into(), "fg=green".into()),
                ("--second".into(), "fg=cyan".into())
            ]
        );
    }

    #[test]
    fn 語の途中のクォートやエスケープでもエイリアスと予約語を抑止する() {
        for word in ["onl'y'_alias", "only\\_alias", "only\"_\"alias"] {
            let buffer = format!("{word} argument");
            let alias = ShellNames {
                aliases: "only_alias",
                ..ShellNames::default()
            };
            let spans = highlight_buffer(&buffer, alias);
            assert_eq!(spans[0].style, "fg=red,bold", "{word}");
            assert_eq!(spans[0].end, word.chars().count());
            assert!(spans.iter().all(|s| s.end <= word.chars().count()));
            let alias_and_function = ShellNames {
                functions: "only_alias",
                ..alias
            };
            assert_eq!(
                highlight_buffer(&buffer, alias_and_function)[0].style,
                "fg=green"
            );
        }
        for word in ["i'f'", "i\\f", "t\"hen\""] {
            let spans = highlight_buffer(&format!("{word} echo"), ShellNames::default());
            assert_eq!(spans[0].style, "fg=red,bold", "{word}");
            assert!(spans.iter().all(|s| s.end <= word.chars().count()));
        }
        assert_eq!(span_styles("ec'ho' --flag", "", "")[0].1, "fg=green");
        assert_eq!(
            span_styles("e'nv' echo", "", "").last().unwrap().1,
            "fg=green"
        );
    }

    #[test]
    fn 代入名にクォートやエスケープがあればコマンドとして扱う() {
        for word in ["F'O'O=bar", "F\\OO=bar", "FOO\\=bar"] {
            let spans = highlight_buffer(&format!("{word} echo"), ShellNames::default());
            assert_eq!(spans[0].style, "fg=red,bold", "{word}");
            assert!(spans.iter().all(|s| s.end <= word.chars().count()));
        }
        assert_eq!(
            span_styles("FOO='a=b' echo", "", "").last().unwrap().1,
            "fg=green"
        );
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
        let spans = highlight_buffer(buf, ShellNames::default());
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
        let spans = highlight_buffer(buf, ShellNames::default());
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

    /// テスト補助: 名前の一覧と PATH を与えて 1 語を分類する
    fn classify(word: &str, names: ShellNames, path_dirs: &[PathBuf]) -> String {
        Classifier::new(names, path_dirs.to_vec()).classify(word, false)
    }

    #[test]
    fn classify_sudo_はプレフィックスコマンド() {
        assert_eq!(
            classify("sudo", ShellNames::default(), &[]),
            "fg=green,bold"
        );
    }

    #[test]
    fn classify_env_はプレフィックスコマンド() {
        assert_eq!(classify("env", ShellNames::default(), &[]), "fg=green,bold");
    }

    #[test]
    fn classify_組み込みコマンドはgreen() {
        assert_eq!(classify("echo", ShellNames::default(), &[]), "fg=green");
    }

    #[test]
    fn classify_予約語はyellow_bold() {
        assert_eq!(
            classify("while", ShellNames::default(), &[]),
            "fg=yellow,bold"
        );
    }

    #[test]
    fn classify_エイリアスはgreen() {
        let names = ShellNames {
            aliases: "ll",
            ..ShellNames::default()
        };
        assert_eq!(classify("ll", names, &[]), "fg=green");
    }

    #[test]
    fn classify_関数はgreen() {
        let names = ShellNames {
            functions: "myfunc",
            ..ShellNames::default()
        };
        assert_eq!(classify("myfunc", names, &[]), "fg=green");
    }

    #[cfg(unix)]
    #[test]
    fn classify_pathコマンドはgreen() {
        let dir = dir_with_executables(&["rustc"]);
        assert_eq!(
            classify("rustc", ShellNames::default(), &[dir.path().to_path_buf()]),
            "fg=green"
        );
    }

    #[test]
    fn classify_不明コマンドはred_bold() {
        assert_eq!(
            classify("no_such_cmd_xyz", ShellNames::default(), &[]),
            "fg=red,bold"
        );
    }

    #[test]
    fn classify_zshから渡された組み込みの一覧を既定の一覧より優先する() {
        // zsh/datetime などのモジュールの組み込みは、zsh から受け取った一覧で分かる
        let names = ShellNames {
            builtins: "strftime\necho",
            ..ShellNames::default()
        };
        assert_eq!(classify("strftime", names, &[]), "fg=green");
        assert_eq!(classify("\\strftime", names, &[]), "fg=green");
        // 一覧に無い名前 (disable で無効にした組み込みなど) は組み込みとして扱わない
        assert_eq!(classify("disown", names, &[]), "fg=red,bold");
        // 一覧を受け取らなければ既定の一覧を使う
        assert_eq!(classify("disown", ShellNames::default(), &[]), "fg=green");
        assert_eq!(
            classify("strftime", ShellNames::default(), &[]),
            "fg=red,bold"
        );
    }

    #[cfg(unix)]
    #[test]
    fn classify_pathの探索結果を名前ごとに覚える() {
        let dir = dir_with_executables(&["zt-cached"]);
        let mut classifier = Classifier::new(ShellNames::default(), vec![dir.path().to_path_buf()]);
        assert!(classifier.in_path("zt-cached"));
        assert!(!classifier.in_path("zt-missing"));
        // 覚えた結果を使い、ファイルの増減ではなく最初の判定を返す
        std::fs::remove_file(dir.path().join("zt-cached")).unwrap();
        std::fs::write(dir.path().join("zt-missing"), "").unwrap();
        assert!(classifier.in_path("zt-cached"));
        assert!(!classifier.in_path("zt-missing"));
        assert_eq!(classifier.path_found.len(), 2);
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
        let spans = highlight_buffer("   ", ShellNames::default());
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
        let spans = highlight_buffer(buf, ShellNames::default());
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
        let spans = highlight_buffer(buf, ShellNames::default());
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
        let spans = highlight_buffer(buf, ShellNames::default());
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
        // `CommandPosition::expects_command` で `out` が
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

    #[test]
    fn highlight_区切り代入プレフィックスをまたぐ状態遷移() {
        let cases: &[(&str, &[(&str, &str)])] = &[
            // 代入語はプレフィックス直後の状態を保ち、続くオプションがコマンド位置を消費する
            (
                "env FOO=bar -u value echo",
                &[
                    ("env", "fg=green,bold"),
                    ("FOO=bar", "fg=cyan"),
                    ("-u", "fg=cyan"),
                ],
            ),
            // 区切り後の代入語の次もコマンド位置
            (
                "true; FOO='a b' echo",
                &[
                    ("true", "fg=green"),
                    (";", "fg=magenta"),
                    ("FOO='a b'", "fg=cyan"),
                    ("'a b'", "fg=yellow"),
                    ("echo", "fg=green"),
                ],
            ),
            // 区切り後の変数展開・クォートだけの語はコマンド名として消費される
            (
                "true; $cmd -x",
                &[
                    ("true", "fg=green"),
                    (";", "fg=magenta"),
                    ("$cmd", "fg=cyan"),
                    ("-x", "fg=cyan"),
                ],
            ),
            (
                "true | 'echo' -x",
                &[
                    ("true", "fg=green"),
                    ("|", "fg=magenta"),
                    ("'echo'", "fg=yellow"),
                    ("-x", "fg=cyan"),
                ],
            ),
            // 区切り直後のリダイレクトの先とその後の語はコマンドではない
            (
                "true; >| file rest",
                &[
                    ("true", "fg=green"),
                    (";", "fg=magenta"),
                    (">|", "fg=magenta"),
                ],
            ),
            (
                "true | 2>&1 rest",
                &[
                    ("true", "fg=green"),
                    ("|", "fg=magenta"),
                    ("2>&", "fg=magenta"),
                ],
            ),
            // 区切りの後はプレフィックス直後の扱いが消え、`-` 始まりの語は未知コマンド
            (
                "sudo ; -foo",
                &[
                    ("sudo", "fg=green,bold"),
                    (";", "fg=magenta"),
                    ("-foo", "fg=red,bold"),
                ],
            ),
        ];
        for (buffer, expected) in cases {
            let expected: Vec<(String, String)> = expected
                .iter()
                .map(|(text, style)| (text.to_string(), style.to_string()))
                .collect();
            assert_eq!(span_styles(buffer, "", ""), expected, "{buffer}");
        }
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
        let spans = highlight_buffer("FOO=\"bar baz\" echo hi", ShellNames::default());
        let assign = find_span(&spans, 0, 13).expect("代入語スパンが必要");
        assert_eq!(assign.style, "fg=cyan");
        let quote = find_span(&spans, 4, 13).expect("クォートセグメントスパンが必要");
        assert_eq!(quote.style, "fg=yellow");
        let cmd = find_span(&spans, 14, 18).expect("echo のスパンが必要");
        assert_eq!(cmd.style, "fg=green");
    }

    #[test]
    fn highlight_シングルクォート代入値の後もコマンド位置が続く() {
        let spans = highlight_buffer("FOO='bar baz' true", ShellNames::default());
        let cmd = find_span(&spans, 14, 18).expect("true のスパンが必要");
        assert_eq!(cmd.style, "fg=green");
    }

    #[test]
    fn highlight_クォート結合語はひとつのコマンドとして分類される() {
        // `ec'ho' hi` は zsh では echo として実行される
        let spans = highlight_buffer("ec'ho' hi", ShellNames::default());
        let cmd = find_span(&spans, 0, 6).expect("結合語全体のスパンが必要");
        assert_eq!(cmd.style, "fg=green");
        let quote = find_span(&spans, 2, 6).expect("クォートセグメントスパンが必要");
        assert_eq!(quote.style, "fg=yellow");
    }

    #[test]
    fn ダブルクォート内のエスケープを論理語へ復元する() {
        for (buffer, name) in [
            (r#"cmd"\$"tail"#, "cmd$tail"),
            (r#"cmd"\`"tail"#, "cmd`tail"),
            (r#"cmd"\""tail"#, "cmd\"tail"),
            (r#"cmd"\\"tail"#, "cmd\\tail"),
            (r#"cmd"\q"tail"#, "cmd\\qtail"),
            ("cmd\"\\界\"tail", "cmd\\界tail"),
            ("cmd\"\\\n\"tail", "cmdtail"),
        ] {
            let spans = highlight_buffer(
                buffer,
                ShellNames {
                    functions: name,
                    ..ShellNames::default()
                },
            );
            assert_eq!(spans[0].style, "fg=green", "{buffer:?}");
            assert_eq!((spans[0].start, spans[0].end), (0, buffer.chars().count()));
            assert_eq!(spans[1].style, "fg=yellow");
            assert_eq!(span_styles(buffer, name, "")[0].1, "fg=red,bold");
        }
    }

    #[test]
    fn highlight_クォート付きオプションはシアン() {
        // `--file='a b'` は 1 語のオプション
        let spans = highlight_buffer("true --file='a b'", ShellNames::default());
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
        let spans = highlight_buffer("sudo -u root ls", ShellNames::default());
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
        let spans = highlight_buffer("true; -foo", ShellNames::default());
        let word = find_span(&spans, 6, 10).expect("-foo のスパンが必要");
        assert_eq!(word.style, "fg=red,bold");
    }

    // ---------------------------------------------------------------
    // 予約語後のコマンド位置維持
    // ---------------------------------------------------------------

    #[test]
    fn highlight_if_then内のコマンドがハイライトされる() {
        let spans = highlight_buffer("if true; then echo hi; fi", ShellNames::default());
        assert_eq!(find_span(&spans, 0, 2).unwrap().style, "fg=yellow,bold"); // if
        assert_eq!(find_span(&spans, 3, 7).unwrap().style, "fg=green"); // true
        assert_eq!(find_span(&spans, 9, 13).unwrap().style, "fg=yellow,bold"); // then
        assert_eq!(find_span(&spans, 14, 18).unwrap().style, "fg=green"); // echo
        assert_eq!(find_span(&spans, 23, 25).unwrap().style, "fg=yellow,bold"); // fi
    }

    #[test]
    fn highlight_time_の後続はコマンド位置() {
        let spans = highlight_buffer("time true", ShellNames::default());
        assert_eq!(find_span(&spans, 0, 4).unwrap().style, "fg=yellow,bold");
        assert_eq!(find_span(&spans, 5, 9).unwrap().style, "fg=green");
    }

    #[test]
    fn highlight_while_の後続はコマンド位置() {
        let spans = highlight_buffer("while true; do echo x; done", ShellNames::default());
        assert_eq!(find_span(&spans, 6, 10).unwrap().style, "fg=green"); // true
        assert_eq!(find_span(&spans, 15, 19).unwrap().style, "fg=green"); // echo
    }

    // ---------------------------------------------------------------
    // 組み込み・予約語テーブルの補完
    // ---------------------------------------------------------------

    #[test]
    fn highlight_コロンとドットは組み込み() {
        let spans = highlight_buffer(": true", ShellNames::default());
        assert_eq!(find_span(&spans, 0, 1).unwrap().style, "fg=green");
        let spans = highlight_buffer(". ./script", ShellNames::default());
        assert_eq!(find_span(&spans, 0, 1).unwrap().style, "fg=green");
    }

    #[test]
    fn highlight_二重ブラケットと単一ブラケットは正当な構文() {
        let spans = highlight_buffer("[[ -f x ]]", ShellNames::default());
        assert_eq!(find_span(&spans, 0, 2).unwrap().style, "fg=yellow,bold");
        let spans = highlight_buffer("[ -f x ]", ShellNames::default());
        assert_eq!(find_span(&spans, 0, 1).unwrap().style, "fg=green");
    }

    #[test]
    fn highlight_ブレースと否定は予約語() {
        let spans = highlight_buffer("{ echo hi; }", ShellNames::default());
        assert_eq!(find_span(&spans, 0, 1).unwrap().style, "fg=yellow,bold"); // {
        assert_eq!(find_span(&spans, 2, 6).unwrap().style, "fg=green"); // echo (位置維持)
        assert_eq!(find_span(&spans, 11, 12).unwrap().style, "fg=yellow,bold"); // }
        let spans = highlight_buffer("! true", ShellNames::default());
        assert_eq!(find_span(&spans, 0, 1).unwrap().style, "fg=yellow,bold"); // !
        assert_eq!(find_span(&spans, 2, 6).unwrap().style, "fg=green"); // true (位置維持)
    }

    // ---------------------------------------------------------------
    // チルダ展開・CLOBBER・エイリアスバイパス
    // ---------------------------------------------------------------

    #[test]
    fn highlight_チルダ始まりの実在パスは実行可能パス扱い() {
        // `~/` はホームディレクトリとして必ず実在する
        let spans = highlight_buffer("~/ x", ShellNames::default());
        let cmd = find_span(&spans, 0, 2).expect("チルダパスのスパンが必要");
        assert_eq!(cmd.style, "fg=green,underline");
    }

    #[test]
    fn highlight_clobberリダイレクトの後続をコマンド扱いしない() {
        // `>|` の `|` はパイプではなく CLOBBER 演算子の一部
        let spans = highlight_buffer("echo hi >| out", ShellNames::default());
        let op = find_span(&spans, 8, 10).expect(">| は 1 トークン");
        assert_eq!(op.style, "fg=magenta");
        assert!(
            !spans.iter().any(|s| s.start == 11),
            "リダイレクト先 out にスパンが付いてはならない: {spans:?}"
        );
    }

    #[test]
    fn highlight_append_clobberも1トークン() {
        let spans = highlight_buffer("true >>| f", ShellNames::default());
        let op = find_span(&spans, 5, 8).expect(">>| は 1 トークン");
        assert_eq!(op.style, "fg=magenta");
    }

    #[test]
    fn highlight_fd付きclobberも1トークン() {
        let spans = highlight_buffer("true 2>| f", ShellNames::default());
        let op = find_span(&spans, 5, 8).expect("2>| は 1 トークン");
        assert_eq!(op.style, "fg=magenta");
    }

    #[test]
    fn リダイレクト直前の数字は1桁だけを記述子として読む() {
        let names = ShellNames {
            functions: "12",
            ..ShellNames::default()
        };
        for operator in [">", ">>", "<", ">&", "<&"] {
            let buffer = format!("12{operator}file");
            let spans = highlight_buffer(&buffer, names);
            assert_eq!(
                find_span(&spans, 0, 2).unwrap().style,
                "fg=green",
                "{buffer}"
            );
            assert_eq!(
                find_span(&spans, 2, 2 + operator.len()).unwrap().style,
                "fg=magenta",
                "{buffer}"
            );
            let buffer = format!("2{operator}file");
            let spans = highlight_buffer(&buffer, names);
            assert_eq!(
                find_span(&spans, 0, 1 + operator.len()).unwrap().style,
                "fg=magenta",
                "{buffer}"
            );
        }
    }

    #[test]
    fn 数値globは語の一部として読みリダイレクトに分けない() {
        for pattern in ["<1-10>", "<->", "<-9>", "<1->"] {
            for word in [
                pattern.to_owned(),
                format!("file{pattern}.txt"),
                format!("1{pattern}"),
            ] {
                let buffer = format!("echo {word} plain");
                let spans = highlight_buffer(&buffer, ShellNames::default());
                assert_eq!(
                    find_span(&spans, 5, 5 + word.len()).unwrap().style,
                    "fg=blue,bold",
                    "{buffer}"
                );
                assert!(spans.iter().all(|span| span.style != "fg=magenta"));
            }
        }
        for buffer in ["echo '<1-10>'", "echo \\<1-10\\>"] {
            let spans = highlight_buffer(buffer, ShellNames::default());
            assert!(spans.iter().all(|span| span.style != "fg=blue,bold"));
        }
    }

    #[test]
    fn highlight_バックスラッシュ付きコマンドはエイリアスバイパス() {
        // `\echo` は zsh でエイリアス展開を抑止して echo を実行する常用イディオム
        let spans = highlight_buffer("\\echo hi", ShellNames::default());
        let cmd = find_span(&spans, 0, 5).expect("\\echo のスパンが必要");
        assert_eq!(cmd.style, "fg=green");
    }

    #[test]
    fn highlight_バックスラッシュ付き未知コマンドは赤() {
        let spans = highlight_buffer("\\zt_no_such_cmd_x", ShellNames::default());
        assert_eq!(spans[0].style, "fg=red,bold");
    }

    #[test]
    fn highlight_バックスラッシュ付き予約語は予約語として扱わない() {
        // `\if` は予約語解釈が無効化されるため通常のコマンド検索になる
        let spans = highlight_buffer("\\if x", ShellNames::default());
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

        let dirs = path_dirs_from_in(dir.path().as_os_str(), None);

        assert!(
            in_path("zt-real", &dirs),
            "実体の実行ファイルが登録されるべき"
        );
        assert!(
            in_path("zt-link", &dirs),
            "symlink された実行ファイル (Homebrew 形式) が登録されるべき"
        );
        assert!(
            !in_path("zt-dangling", &dirs),
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
            let spans = highlight_buffer(buf, ShellNames::default());
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

/// 括弧・行継続・リダイレクト・代入など、zsh の構文に沿った分類のテスト
#[cfg(test)]
mod syntax_tests {
    use super::*;

    fn pairs(buffer: &str) -> Vec<(String, String)> {
        highlight_buffer(buffer, ShellNames::default())
            .iter()
            .map(|s| {
                let text: String = buffer.chars().skip(s.start).take(s.end - s.start).collect();
                (text, s.style.clone())
            })
            .collect()
    }

    fn assert_no_red(buffer: &str) {
        let p = pairs(buffer);
        assert!(
            !p.iter().any(|(_, s)| s == "fg=red,bold"),
            "{buffer:?} に赤スパンがある: {p:?}"
        );
    }

    fn style_of(buffer: &str, text: &str) -> Option<String> {
        pairs(buffer)
            .into_iter()
            .find(|(t, _)| t == text)
            .map(|(_, s)| s)
    }

    #[test]
    fn 行継続の後もコマンド位置を保つ() {
        for buffer in [
            "true && \\\n  echo hi",
            "FOO=1 \\\n  echo hi",
            "sudo \\\n  echo hi",
            "true | \\\n  echo hi",
        ] {
            assert_no_red(buffer);
            assert_eq!(
                style_of(buffer, "echo").as_deref(),
                Some("fg=green"),
                "{buffer:?}"
            );
        }
        // 語中の行継続は語を連結する (`ec\<改行>ho` は echo)
        assert_eq!(pairs("ec\\\nho hi")[0].1, "fg=green");
    }

    #[test]
    fn 配列代入の要素をコマンドとして分類しない() {
        assert_eq!(
            pairs("arr=(one two three)"),
            [("arr=(one two three)".to_string(), "fg=cyan".to_string())]
        );
        assert_no_red("files=( a.txt 'b c.txt' )");
        assert_eq!(
            style_of("arr=(a b) echo hi", "echo").as_deref(),
            Some("fg=green")
        );
    }

    #[test]
    fn 追記代入と添字付き代入は代入語() {
        for word in ["PATH+=:/opt/bin", "arr[2]=x", "arr+=(c)", "h[key]+=v"] {
            assert_eq!(style_of(word, word).as_deref(), Some("fg=cyan"), "{word}");
        }
        assert_eq!(
            style_of("N+=1 echo hi", "echo").as_deref(),
            Some("fg=green")
        );
    }

    #[test]
    fn 標準出力とエラーをまとめるリダイレクトは1トークン() {
        for op in [
            ">>&", "&>|", "&>!", "&>>|", "&>>!", ">>&|", ">>&!", ">&!", "2>>&",
        ] {
            let buffer = format!("true {op} out.log");
            assert_eq!(
                style_of(&buffer, op).as_deref(),
                Some("fg=magenta"),
                "{buffer}"
            );
            assert_no_red(&buffer);
        }
    }

    #[test]
    fn サブシェル_算術_関数定義を赤にしない() {
        let buffer = "(cd /tmp && echo ok)";
        assert_no_red(buffer);
        assert_eq!(style_of(buffer, "cd").as_deref(), Some("fg=green"));
        assert_eq!(style_of(buffer, "echo").as_deref(), Some("fg=green"));
        assert_no_red("(( n = 1 + 2 ))");
        assert_no_red("greet() { echo hi; }");
        assert_eq!(
            style_of("(( n > 1 )) && echo hi", "echo").as_deref(),
            Some("fg=green")
        );
        // 引数中のグロブの選択肢 `(a|b)` の `|` はパイプではない
        assert_eq!(
            pairs("echo (foo|bar).txt"),
            [("echo".to_string(), "fg=green".to_string())]
        );
        assert_no_red("[[ $x == (a|b) ]] && echo ok");
        assert_no_red("echo $(ls|wc -l)");
    }

    #[test]
    fn 二重ブラケット内の論理演算子はコマンド区切りではない() {
        for buffer in [
            "[[ -f a && -r a ]] && echo ok",
            "[[ -n $a || -z b ]]; echo ok",
        ] {
            assert_no_red(buffer);
            assert_eq!(
                style_of(buffer, "echo").as_deref(),
                Some("fg=green"),
                "{buffer}"
            );
        }
    }

    #[test]
    fn zshの組み込みコマンドを赤にしない() {
        for name in ["disown", "r", "bye", "disable", "sched", "pushln"] {
            assert_eq!(pairs(name)[0].1, "fg=green", "{name}");
        }
    }

    #[test]
    fn 履歴展開をコマンド名として赤にしない() {
        for buffer in ["sudo !!", "!!", "!vim file", "!-2"] {
            assert_no_red(buffer);
        }
        // `!` 単独は予約語のまま
        assert_eq!(pairs("! true")[0].1, "fg=yellow,bold");
    }

    // ---------------------------------------------------------------
    // 語の走査とトークン終端のテスト
    // ---------------------------------------------------------------

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    fn scan(text: &str, expects_command: bool, in_subshell: bool) -> Word {
        scan_word(
            &chars(text),
            0,
            WordScanContext {
                expects_command,
                in_subshell,
            },
        )
    }

    #[test]
    fn scan_word_はクォートを語に含め論理語とクォート外の文字を分ける() {
        let word = scan("ec'ho' hi", true, false);
        assert_eq!((word.start, word.end), (0, 6));
        assert_eq!(word.logical, "echo");
        assert_eq!(word.bare, "ec");
        assert_eq!(word.quote_segs, [(2, 6)]);
        assert!(word.is_quoted());
    }

    #[test]
    fn scan_word_は代入の名前がクォートされているかを区別する() {
        let text = chars("FOO='a b' cmd");
        let context = WordScanContext {
            expects_command: true,
            in_subshell: false,
        };
        let word = scan_word(&text, 0, context);
        assert_eq!(word.end, 9);
        assert_eq!(word.logical, "FOO=a b");
        assert!(!word.has_quoted_name(&text));

        let text = chars("F'O'O=bar");
        let word = scan_word(&text, 0, context);
        assert_eq!(word.logical, "FOO=bar");
        assert!(word.has_quoted_name(&text));
    }

    #[test]
    fn scan_word_はエスケープした文字をクォート外の判定から外す() {
        let word = scan("x\\*", false, false);
        assert_eq!(word.logical, "x*");
        assert_eq!(word.bare, "x\\");
        // 語頭の `\cmd` はエイリアスの抑止を分類まで伝えるため論理語に `\` を残す
        let word = scan("\\ls", true, false);
        assert_eq!(word.logical, "\\ls");
        assert_eq!(word.bare, "\\s");
    }

    #[test]
    fn scan_word_は配列代入の括弧の中の空白を語に含める() {
        let word = scan("a=(b c) rest", true, false);
        assert_eq!(word.end, 7);
        assert_eq!(word.logical, "a=(b c)");
        // コマンド位置でなければ括弧はグロブで、空白で語が終わる
        assert_eq!(scan("a=(b c) rest", false, false).end, 4);
    }

    #[test]
    fn scan_word_はグロブの括弧の中の縦棒で語を切らない() {
        assert_eq!(scan("x(a|b) | y", false, false).end, 6);
        assert_eq!(scan("x | y", false, false).end, 1);
    }

    #[test]
    fn scan_word_は関数定義とサブシェルの閉じ括弧で語を切る() {
        assert_eq!(scan("name() { :; }", true, false).end, 4);
        // 引数位置の `()` は語の一部
        assert_eq!(scan("name() x", false, false).end, 6);
        assert_eq!(scan("a)b", false, true).end, 1);
        assert_eq!(scan("a)b", false, false).end, 3);
    }

    #[test]
    fn scan_word_の位置は文字単位() {
        let word = scan("日本'語' x", true, false);
        assert_eq!((word.start, word.end), (0, 5));
        assert_eq!(word.logical, "日本語");
        assert_eq!(word.quote_segs, [(2, 5)]);
    }

    #[test]
    fn scan_word_はダブルクォートでエスケープできる文字だけバックスラッシュを外す() {
        let word = scan(r#""\$x\a\!""#, false, false);
        assert_eq!(word.logical, r"$x\a!");
        assert_eq!(word.bare, "");
        assert_eq!(word.quote_segs, [(0, 9)]);
    }

    #[test]
    fn scan_word_は閉じていないクォートを末尾まで読む() {
        let word = scan("'abc", false, false);
        assert_eq!((word.end, word.logical.as_str()), (4, "abc"));
        assert_eq!(word.quote_segs, [(0, 4)]);
        let word = scan("\"a\\", false, false);
        assert_eq!((word.end, word.logical.as_str()), (3, "a\\"));
        assert_eq!(word.quote_segs, [(0, 3)]);
    }

    #[test]
    fn variable_end_は変数展開の終端を返す() {
        let end = |text: &str| variable_end(&chars(text), 0);
        assert_eq!(end("$foo_1 x"), Some(6));
        assert_eq!(end("${a b}c"), Some(6));
        // 閉じていない `${` は末尾まで
        assert_eq!(end("${abc"), Some(5));
        assert_eq!(end("$?x"), Some(2));
        assert_eq!(end("$ x"), Some(1));
        // 末尾の `$` と `$` 以外は変数ではない
        assert_eq!(end("$"), None);
        assert_eq!(end("a$"), None);
    }

    #[test]
    fn redirect_end_はリダイレクト演算子の終端を返す() {
        let end = |text: &str| redirect_end(&chars(text), 0);
        assert_eq!(end("2>&1"), Some(3));
        assert_eq!(end(">>|f"), Some(3));
        assert_eq!(end("&>>!f"), Some(4));
        assert_eq!(end("&>f"), Some(2));
        assert_eq!(end("<&3"), Some(2));
        assert_eq!(end("<>f"), Some(2));
        // 数値 glob・2 桁の数字・`&&` はリダイレクトではない
        assert_eq!(end("<1-10>"), None);
        assert_eq!(end("2<1-10>"), None);
        assert_eq!(end("12>f"), None);
        assert_eq!(end("&&"), None);
    }

    #[test]
    fn arithmetic_close_は中の括弧の対応を数えて閉じ括弧を探す() {
        assert_eq!(arithmetic_close(&chars("((1+(2*3)))"), 2), Some(9));
        assert_eq!(arithmetic_close(&chars("(( a ) ))"), 2), Some(7));
        assert_eq!(arithmetic_close(&chars("((x"), 2), None);
    }
}
