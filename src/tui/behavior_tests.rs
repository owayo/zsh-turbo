//! 設定画面のキー操作の挙動を固定するテスト。
//! 内部のタブ・選択位置の表現に依存しないよう、操作は実際のキー入力で行い、
//! 表示中のタブと選択中の行は描画結果の強調表示から読み取る。

use super::*;
use crate::completion::{Registration, Source};
use crate::config::SegmentColors;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
use serde_json::Value;
use unicode_width::UnicodeWidthStr;

const PROMPT_ROWS: [&str; 9] = [
    "Prompt Style",
    "Font Level",
    "Transient Prompt",
    "Home Symbol",
    "Truncation Length",
    "Truncation Symbol",
    "IP Interface (auto if empty)",
    "Command input",
    "Blank lines between prompts (0–10)",
];
const GIT_ROWS: [&str; 3] = ["Show Status", "Show Ahead/Behind", "Show Stash"];
const SUGGEST_ROWS: [&str; 9] = [
    "Strategy",
    "Highlight Color",
    "Max Suggestions",
    "Tab",
    "Right Arrow",
    "Alt+F",
    "Ctrl+Right",
    "Record Task Usage",
    "Record Directory History",
];
const STYLE_ROWS: [&str; 9] = [
    "Primary Color",
    "Success Color",
    "Error Color",
    "Muted Color",
    "Rainbow palette",
    "Rainbow block (Left/Right)",
    "Block foreground",
    "Block background",
    "Reset block colors",
];
const CUSTOM_ROWS: [&str; 7] = [
    "Selected (Left/Right)",
    "Name",
    "Command",
    "Icon",
    "Foreground",
    "Background",
    "When",
];
const SHELL_ROWS: [&str; 4] = [
    "Completion Dirs (: separated)",
    "Terminal Integration",
    "Interface Language",
    "Terminal Font Setup",
];
const COMPLETION_ROWS: [&str; 8] = [
    "Back to registered CLIs",
    "CLI command name",
    "Enabled",
    "Source",
    "Generator argv (JSON array) / completion file",
    "Extra files to watch (JSON array)",
    "Rebuild changed caches (r)",
    "Force rebuild caches (Shift+R)",
];
const TAB_ORDER: [&str; 8] = [
    "Prompt",
    "Segments",
    "Git",
    "Suggest",
    "Style",
    "Custom",
    "Shell",
    "Completions",
];

fn press(app: &mut App, code: KeyCode) {
    assert!(
        !handle_event(app, KeyEvent::from(code)),
        "{code:?} で終了した"
    );
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    terminal.draw(|frame| ui(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}

/// 描画結果の各行の文字列と、その行に選択中の強調表示（黒字・シアン背景）があるか
fn screen_lines(buffer: &Buffer) -> Vec<(String, bool)> {
    buffer
        .content()
        .chunks(usize::from(buffer.area.width))
        .map(|cells| {
            let mut text = String::new();
            let mut hidden = 0;
            for cell in cells {
                // 全角文字の後ろのセルは表示されないので読み飛ばす
                if hidden == 0 {
                    text.push_str(cell.symbol());
                }
                hidden = hidden.max(cell.symbol().width()).saturating_sub(1);
            }
            let focused = cells
                .iter()
                .any(|cell| cell.fg == Color::Black && cell.bg == Color::Cyan);
            (text, focused)
        })
        .collect()
}

/// 上から n 番目（0 始まり）に現れる枠の上端の行
fn block_top(lines: &[(String, bool)], n: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .filter(|(_, (text, _))| text.starts_with('┌'))
        .nth(n)
        .unwrap()
        .0
}

/// 強調表示されているタブの見出し
fn tab_title(app: &mut App) -> String {
    let buffer = draw(app);
    let row = block_top(&screen_lines(&buffer), 1) + 1;
    buffer
        .content()
        .chunks(usize::from(buffer.area.width))
        .nth(row)
        .unwrap()
        .iter()
        .filter(|cell| cell.fg == Color::Cyan && cell.modifier.contains(Modifier::BOLD))
        .map(|cell| cell.symbol())
        .collect()
}

/// 設定項目の枠の中の行（罫線と字下げを除く）と、選択中の行の位置
fn field_rows(app: &mut App) -> (Vec<String>, Option<usize>) {
    let lines = screen_lines(&draw(app));
    let top = block_top(&lines, 2);
    let mut rows = Vec::new();
    let mut focused = None;
    for (text, is_focused) in &lines[top + 1..] {
        if text.starts_with('└') {
            break;
        }
        let inner = text
            .trim_start_matches('│')
            .trim_end_matches(['│', ' '])
            .trim_start();
        if inner.is_empty() {
            continue;
        }
        if *is_focused {
            focused = Some(rows.len());
        }
        rows.push(inner.to_owned());
    }
    (rows, focused)
}

fn assert_rows(app: &mut App, labels: &[&str], context: &str) {
    let (rows, _) = field_rows(app);
    assert_eq!(rows.len(), labels.len(), "{context}: {rows:?}");
    for (row, label) in rows.iter().zip(labels) {
        assert!(row.starts_with(&format!("{label}:")), "{context}: {rows:?}");
    }
}

fn focused_row(app: &mut App) -> Option<String> {
    let (rows, focused) = field_rows(app);
    focused.map(|i| rows[i].clone())
}

fn open_tab(app: &mut App, title: &str) {
    for _ in 0..TAB_ORDER.len() {
        if tab_title(app) == title {
            return;
        }
        press(app, KeyCode::Tab);
    }
    panic!("{title} タブを開けない");
}

/// 先頭まで ↑ で戻ってから、ラベルの行まで ↓ で進む
fn select_row(app: &mut App, label: &str) {
    let prefix = format!("{label}:");
    for _ in 0..20 {
        if field_rows(app).1 == Some(0) {
            break;
        }
        press(app, KeyCode::Up);
    }
    for _ in 0..20 {
        if focused_row(app).is_some_and(|row| row.starts_with(&prefix)) {
            return;
        }
        press(app, KeyCode::Down);
    }
    panic!("{label} を選択できない: {:?}", field_rows(app));
}

fn snapshot(app: &App) -> Value {
    serde_json::to_value(&app.config).unwrap()
}

/// 値が変わった設定の位置（JSON Pointer）
fn changed_paths(before: &Value, after: &Value) -> Vec<String> {
    fn walk(before: &Value, after: &Value, path: String, out: &mut Vec<String>) {
        match (before, after) {
            (Value::Object(a), Value::Object(b)) => {
                let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
                for key in keys {
                    walk(
                        a.get(key).unwrap_or(&Value::Null),
                        b.get(key).unwrap_or(&Value::Null),
                        format!("{path}/{key}"),
                        out,
                    );
                }
            }
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    walk(a, b, format!("{path}/{i}"), out);
                }
            }
            _ if before != after => out.push(path),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(before, after, String::new(), &mut out);
    out
}

/// 未保存の印を消してから項目を選んでキーを押し、変わった設定の位置を返す
fn act(app: &mut App, label: &str, key: KeyCode) -> Vec<String> {
    select_row(app, label);
    app.dirty = false;
    let before = snapshot(app);
    press(app, key);
    changed_paths(&before, &snapshot(app))
}

/// 左右キー（h/l を含む）では設定も未保存の印も変わらず、編集も始まらない
fn assert_adjust_ignored(app: &mut App, label: &str) {
    for key in [
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Char('h'),
        KeyCode::Char('l'),
    ] {
        assert!(act(app, label, key).is_empty(), "{label}: {key:?}");
        assert!(!app.dirty, "{label}: {key:?}");
        assert!(app.editing.is_none(), "{label}: {key:?}");
        assert!(app.color_picker.is_none(), "{label}: {key:?}");
    }
}

/// 決定で編集を始め、取消しでは設定を変えない
fn assert_starts_edit(
    app: &mut App,
    label: &str,
    target: EditTarget,
    buffer: Option<&str>,
    picker: bool,
) {
    assert!(act(app, label, KeyCode::Enter).is_empty(), "{label}");
    assert_eq!(app.editing, Some(target), "{label}");
    if let Some(buffer) = buffer {
        assert_eq!(app.edit_buffer, buffer, "{label}");
    }
    assert_eq!(app.color_picker.is_some(), picker, "{label}");
    assert!(!app.dirty, "{label}");
    let before = snapshot(app);
    press(app, KeyCode::Esc);
    assert!(app.editing.is_none(), "{label}");
    assert!(app.color_picker.is_none(), "{label}");
    assert_eq!(snapshot(app), before, "{label}");
}

fn replace_input(app: &mut App, text: &str) {
    press(app, KeyCode::End);
    for _ in 0..app.edit_buffer.chars().count() {
        press(app, KeyCode::Backspace);
    }
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn custom(name: &str) -> CustomSegment {
    CustomSegment {
        name: name.into(),
        command: ":".into(),
        icon: String::new(),
        fg: "white".into(),
        bg: "238".into(),
        when: "always".into(),
    }
}

fn registration(command: &str) -> Registration {
    Registration {
        command: command.into(),
        ..Default::default()
    }
}

fn help_line(app: &mut App) -> String {
    screen_lines(&draw(app)).pop().unwrap().0
}

fn truncation_length(config: &mut Config) -> &mut usize {
    &mut config.prompt.dir.truncation_length
}

fn max_suggestions(config: &mut Config) -> &mut usize {
    &mut config.suggest.max_suggestions
}

fn prompt_style(config: &mut Config) -> &mut String {
    &mut config.prompt.prompt_style
}

fn font_level(config: &mut Config) -> &mut String {
    &mut config.prompt.font_level
}

fn strategy(config: &mut Config) -> &mut String {
    &mut config.suggest.strategy
}

fn rainbow_palette_name(config: &mut Config) -> &mut String {
    &mut config.style.rainbow_palette
}

fn term_shell_integration(config: &mut Config) -> &mut String {
    &mut config.shell.term_shell_integration
}

fn tab_key(config: &Config) -> SuggestKeyAction {
    config.suggest.keys.tab
}

fn right_key(config: &Config) -> SuggestKeyAction {
    config.suggest.keys.right
}

fn alt_f_key(config: &Config) -> SuggestKeyAction {
    config.suggest.keys.alt_f
}

fn ctrl_right_key(config: &Config) -> SuggestKeyAction {
    config.suggest.keys.ctrl_right
}

#[test]
fn 各タブの設定項目が決まった順に並び先頭を選んだ状態で始まる() {
    let mut config = Config::default();
    config.prompt.custom.push(custom("custom_1"));
    let mut app = App::new(config, Lang::En);
    assert_eq!(tab_title(&mut app), "Prompt");
    for (tab, labels) in [
        ("Prompt", &PROMPT_ROWS[..]),
        ("Git", &GIT_ROWS[..]),
        ("Suggest", &SUGGEST_ROWS[..]),
        ("Style", &STYLE_ROWS[..]),
        ("Custom", &CUSTOM_ROWS[..]),
        ("Shell", &SHELL_ROWS[..]),
    ] {
        open_tab(&mut app, tab);
        assert_rows(&mut app, labels, tab);
        assert_eq!(field_rows(&mut app).1, Some(0), "{tab}");
    }
    // 補完は一覧から開いた詳細画面に項目が並び、CLI 名を選んだ状態で開く
    open_tab(&mut app, "Completions");
    press(&mut app, KeyCode::Enter);
    assert_rows(&mut app, &COMPLETION_ROWS, "Completions");
    assert_eq!(field_rows(&mut app).1, Some(1));
}

#[test]
fn タブは決まった順に前後へ巡回し別名のキーでも切り替わる() {
    let mut app = App::new(Config::default(), Lang::En);
    for title in TAB_ORDER.iter().skip(1).chain(&TAB_ORDER[..1]) {
        press(&mut app, KeyCode::Tab);
        assert_eq!(tab_title(&mut app), *title);
    }
    for title in TAB_ORDER.iter().rev() {
        press(&mut app, KeyCode::BackTab);
        assert_eq!(tab_title(&mut app), *title);
    }
    press(&mut app, KeyCode::Char(']'));
    assert_eq!(tab_title(&mut app), "Segments");
    press(&mut app, KeyCode::Char('['));
    press(&mut app, KeyCode::Char('['));
    assert_eq!(tab_title(&mut app), "Completions");
}

#[test]
fn 上下キーは先頭と末尾で止まりタブを移っても選択位置を保つ() {
    let mut config = Config::default();
    config.prompt.custom.push(custom("custom_1"));
    let mut app = App::new(config, Lang::En);
    let tabs = [
        ("Prompt", PROMPT_ROWS.len()),
        ("Git", GIT_ROWS.len()),
        ("Suggest", SUGGEST_ROWS.len()),
        ("Style", STYLE_ROWS.len()),
        ("Custom", CUSTOM_ROWS.len()),
        ("Shell", SHELL_ROWS.len()),
    ];
    for (tab, len) in tabs {
        open_tab(&mut app, tab);
        press(&mut app, KeyCode::Up);
        assert_eq!(field_rows(&mut app).1, Some(0), "{tab}");
        for _ in 0..len + 2 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(field_rows(&mut app).1, Some(len - 1), "{tab}");
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(field_rows(&mut app).1, Some(len - 2), "{tab}");
    }
    for (tab, len) in tabs {
        open_tab(&mut app, tab);
        assert_eq!(field_rows(&mut app).1, Some(len - 2), "{tab}");
    }
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(field_rows(&mut app).1, Some(SHELL_ROWS.len() - 1));
    // 補完の詳細画面も同じ範囲で止まる
    open_tab(&mut app, "Completions");
    press(&mut app, KeyCode::Enter);
    for _ in 0..COMPLETION_ROWS.len() + 2 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(field_rows(&mut app).1, Some(COMPLETION_ROWS.len() - 1));
    for _ in 0..COMPLETION_ROWS.len() + 2 {
        press(&mut app, KeyCode::Up);
    }
    assert_eq!(field_rows(&mut app).1, Some(0));
}

#[test]
fn 配色タブの項目数は左側のブロックの有無に追従する() {
    for (left, len) in [
        (vec!["dir", "git"], STYLE_ROWS.len()),
        (vec![], 6),
        (vec!["missing"], 6),
    ] {
        let mut config = Config::default();
        config.prompt.left_segments = left.iter().map(|s| s.to_string()).collect();
        let mut app = App::new(config, Lang::En);
        open_tab(&mut app, "Style");
        assert_rows(&mut app, &STYLE_ROWS[..len], &format!("{left:?}"));
        for _ in 0..STYLE_ROWS.len() + 2 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(field_rows(&mut app).1, Some(len - 1), "{left:?}");
    }
}

#[test]
fn 配色タブで選択中のブロック項目が消えても戻せば同じ行を選ぶ() {
    let mut app = App::new(Config::default(), Lang::En);
    open_tab(&mut app, "Style");
    select_row(&mut app, "Reset block colors");
    let left = std::mem::take(&mut app.config.prompt.left_segments);
    assert_eq!(field_rows(&mut app).0.len(), 6);
    // 項目が消えている間の左右・決定は設定を変えない
    for key in [KeyCode::Left, KeyCode::Right, KeyCode::Enter] {
        app.dirty = false;
        let before = snapshot(&app);
        press(&mut app, key);
        assert_eq!(snapshot(&app), before, "{key:?}");
        assert!(!app.dirty);
        assert!(app.editing.is_none());
    }
    app.config.prompt.left_segments = left;
    assert!(
        focused_row(&mut app)
            .unwrap()
            .starts_with("Reset block colors:")
    );
}

#[test]
fn 配色タブで選択中のブロック項目が消えると表示中の末尾の項目を選んだものとして扱う() {
    // 強調・移動・操作のどれも末尾の行（ブロックの選択）を起点にする。
    // ↓ では元の項目を選んだままにするので、ブロックが戻れば元の行に戻る。
    let mut app = App::new(Config::default(), Lang::En);
    open_tab(&mut app, "Style");
    select_row(&mut app, "Reset block colors");
    let left = std::mem::take(&mut app.config.prompt.left_segments);
    assert_eq!(field_rows(&mut app).1, Some(5));
    press(&mut app, KeyCode::Down);
    assert_eq!(field_rows(&mut app).1, Some(5));
    app.config.prompt.left_segments = left.clone();
    assert!(
        focused_row(&mut app)
            .unwrap()
            .starts_with("Reset block colors:")
    );
    app.config.prompt.left_segments.clear();
    press(&mut app, KeyCode::Up);
    assert_eq!(field_rows(&mut app).1, Some(4));
    app.config.prompt.left_segments = left;
    assert!(
        focused_row(&mut app)
            .unwrap()
            .starts_with("Rainbow palette:")
    );
}

#[test]
fn プロンプトタブの各項目は左右と決定で決まった操作をする() {
    use KeyCode::{Enter, Left, Right};
    let mut app = App::new(Config::default(), Lang::En);
    // スタイルとフォント種別は左右で前後に巡回し、決定で次へ進む
    for (key, value) in [
        (Right, "classic"),
        (Left, "lean"),
        (Left, "pure"),
        (Enter, "lean"),
    ] {
        assert_eq!(act(&mut app, "Prompt Style", key), ["/prompt/prompt_style"]);
        assert_eq!(app.config.prompt.prompt_style, value);
        assert!(app.dirty);
    }
    for (key, value) in [
        (Right, "ascii"),
        (Left, "unicode"),
        (Enter, "ascii"),
        (Right, "nerd"),
    ] {
        assert_eq!(act(&mut app, "Font Level", key), ["/prompt/font_level"]);
        assert_eq!(app.config.prompt.font_level, value);
        assert!(app.dirty);
    }
    // 過去のプロンプトの簡略化は決定でだけ切り替わる
    assert_adjust_ignored(&mut app, "Transient Prompt");
    assert_eq!(
        act(&mut app, "Transient Prompt", Enter),
        ["/prompt/transient"]
    );
    assert!(app.config.prompt.transient);
    assert!(app.dirty);
    // 文字列の項目は決定で編集を始め、左右では何もしない
    for (label, target, value) in [
        ("Home Symbol", EditTarget::HomeSymbol, "~"),
        ("Truncation Symbol", EditTarget::TruncationSymbol, "…"),
        ("IP Interface (auto if empty)", EditTarget::IpInterface, ""),
    ] {
        assert_adjust_ignored(&mut app, label);
        assert_starts_edit(&mut app, label, target, Some(value), false);
    }
    // 入力行の位置は左右と決定のどれでも切り替わる
    for key in [Left, Right, Enter, KeyCode::Char(' ')] {
        let old = app.config.prompt.newline;
        assert_eq!(act(&mut app, "Command input", key), ["/prompt/newline"]);
        assert_ne!(app.config.prompt.newline, old);
        assert!(app.dirty);
    }
}

#[test]
fn 数値の項目は左右で増減し境界でも未保存の扱いを保つ() {
    use KeyCode::{Left, Right};
    let mut app = App::new(Config::default(), Lang::En);
    // 0 では減らさず未保存にもしない。最大値では増えないが未保存にする
    for (tab, label, path, field, target) in [
        (
            "Prompt",
            "Truncation Length",
            "/prompt/dir/truncation_length",
            truncation_length as fn(&mut Config) -> &mut usize,
            EditTarget::TruncationLength,
        ),
        (
            "Suggest",
            "Max Suggestions",
            "/suggest/max_suggestions",
            max_suggestions,
            EditTarget::MaxSuggestions,
        ),
    ] {
        open_tab(&mut app, tab);
        *field(&mut app.config) = 3;
        assert_eq!(act(&mut app, label, Left), [path]);
        assert_eq!(*field(&mut app.config), 2);
        assert!(app.dirty);
        assert_eq!(act(&mut app, label, Right), [path]);
        assert_eq!(*field(&mut app.config), 3);
        assert!(app.dirty);
        *field(&mut app.config) = 0;
        assert!(act(&mut app, label, Left).is_empty());
        assert!(!app.dirty);
        *field(&mut app.config) = usize::MAX;
        assert!(act(&mut app, label, Right).is_empty());
        assert!(app.dirty);
        assert_starts_edit(
            &mut app,
            label,
            target,
            Some(&usize::MAX.to_string()),
            false,
        );
    }
    // 空行数は 0〜10 に収め、境界で値が変わらなくても未保存にする
    open_tab(&mut app, "Prompt");
    let label = "Blank lines between prompts (0–10)";
    for (start, key, expected) in [
        (0, Left, 0),
        (0, Right, 1),
        (10, Right, 10),
        (10, Left, 9),
        (usize::MAX, Left, 9),
        (usize::MAX, Right, 10),
    ] {
        app.config.prompt.blank_lines = start;
        act(&mut app, label, key);
        assert_eq!(app.config.prompt.blank_lines, expected, "{start} {key:?}");
        assert!(app.dirty, "{start} {key:?}");
    }
    // 範囲外の値は編集欄で上限に丸めて見せるだけで、設定は変えない
    app.config.prompt.blank_lines = 15;
    assert_starts_edit(&mut app, label, EditTarget::BlankLines, Some("10"), false);
    assert_eq!(app.config.prompt.blank_lines, 15);
}

#[test]
fn gitタブのフラグは決定でだけ切り替わる() {
    let mut app = App::new(Config::default(), Lang::En);
    open_tab(&mut app, "Git");
    for (label, path) in [
        ("Show Status", "/prompt/git/show_status"),
        ("Show Ahead/Behind", "/prompt/git/show_ahead_behind"),
        ("Show Stash", "/prompt/git/show_stash"),
    ] {
        assert_adjust_ignored(&mut app, label);
        for _ in 0..2 {
            assert_eq!(act(&mut app, label, KeyCode::Enter), [path]);
            assert!(app.dirty);
        }
    }
}

#[test]
fn 入力候補タブの各項目は左右と決定で決まった操作をする() {
    use KeyCode::{Enter, Left, Right};
    use SuggestKeyAction::{Default, Full, Step, Word};
    let mut app = App::new(Config::default(), Lang::En);
    open_tab(&mut app, "Suggest");
    for (key, value) in [
        (Right, "substring"),
        (Left, "prefix"),
        (Left, "fuzzy"),
        (Enter, "prefix"),
    ] {
        assert_eq!(act(&mut app, "Strategy", key), ["/suggest/strategy"]);
        assert_eq!(app.config.suggest.strategy, value);
        assert!(app.dirty);
    }
    assert_adjust_ignored(&mut app, "Highlight Color");
    assert_starts_edit(
        &mut app,
        "Highlight Color",
        EditTarget::HighlightColor,
        Some("fg=8"),
        true,
    );
    // キー操作は左右で前後に巡回し、決定で次へ進む。他のキーの設定は変えない
    for (label, path, get, values) in [
        (
            "Tab",
            "/suggest/keys/tab",
            tab_key as fn(&Config) -> SuggestKeyAction,
            [Step, Full, Default, Full],
        ),
        (
            "Right Arrow",
            "/suggest/keys/right",
            right_key,
            [Word, Step, Full, Step],
        ),
        (
            "Alt+F",
            "/suggest/keys/alt_f",
            alt_f_key,
            [Default, Word, Step, Word],
        ),
        (
            "Ctrl+Right",
            "/suggest/keys/ctrl_right",
            ctrl_right_key,
            [Default, Word, Step, Word],
        ),
    ] {
        for (key, value) in [Right, Left, Left, Enter].into_iter().zip(values) {
            assert_eq!(act(&mut app, label, key), [path], "{label} {key:?}");
            assert_eq!(get(&app.config), value, "{label} {key:?}");
            assert!(app.dirty);
        }
    }
    // 記録の設定は左右と決定のどれでも切り替わる
    for (label, path) in [
        ("Record Task Usage", "/suggest/record_task_usage"),
        (
            "Record Directory History",
            "/suggest/record_directory_history",
        ),
    ] {
        for key in [Left, Right, Enter] {
            assert_eq!(act(&mut app, label, key), [path], "{label} {key:?}");
            assert!(app.dirty);
        }
    }
}

#[test]
fn 配色タブの各項目は左右と決定で決まった操作をする() {
    use KeyCode::{Enter, Left, Right};
    let mut app = App::new(Config::default(), Lang::En);
    open_tab(&mut app, "Style");
    for (label, target, value) in [
        ("Primary Color", EditTarget::PrimaryColor, "blue"),
        ("Success Color", EditTarget::SuccessColor, "green"),
        ("Error Color", EditTarget::ErrorColor, "red"),
        ("Muted Color", EditTarget::MutedColor, "bright_black"),
    ] {
        assert_adjust_ignored(&mut app, label);
        assert_starts_edit(&mut app, label, target, Some(value), true);
    }
    for (key, value) in [
        (Right, "blue"),
        (Left, "default"),
        (Left, "grayscale"),
        (Enter, "default"),
    ] {
        assert_eq!(
            act(&mut app, "Rainbow palette", key),
            ["/style/rainbow_palette"]
        );
        assert_eq!(app.config.style.rainbow_palette, value);
        assert!(app.dirty);
    }
    // ブロックの選択は設定を変えずに左右で巡回し、決定で次へ進む
    let block = "Rainbow block (Left/Right)";
    for (key, name) in [
        (Right, "git"),
        (Right, "dir"),
        (Left, "git"),
        (Enter, "dir"),
    ] {
        assert!(act(&mut app, block, key).is_empty());
        assert!(!app.dirty);
        assert_eq!(focused_row(&mut app).unwrap(), format!("{block}: {name}"));
    }
    // ブロックの文字色・背景色は選択中のブロックの色選択を開く
    for (label, target) in [
        ("Block foreground", EditTarget::RainbowFg("dir".into())),
        ("Block background", EditTarget::RainbowBg("dir".into())),
    ] {
        assert_adjust_ignored(&mut app, label);
        assert_starts_edit(&mut app, label, target, None, true);
    }
    // 個別色の解除は個別色があるときだけ設定を変える
    let reset = "Reset block colors";
    assert_adjust_ignored(&mut app, reset);
    assert!(act(&mut app, reset, Enter).is_empty());
    assert!(!app.dirty);
    assert!(app.status_msg.is_none());
    app.config.style.rainbow_overrides.insert(
        "dir".into(),
        SegmentColors {
            fg: Some("1".into()),
            bg: None,
        },
    );
    assert_eq!(act(&mut app, reset, Enter), ["/style/rainbow_overrides"]);
    assert!(app.dirty);
    assert_eq!(
        app.status_msg.as_deref(),
        Some("Restored palette colors for this block.")
    );
}

#[test]
fn シェルタブの各項目は左右と決定で決まった操作をする() {
    use KeyCode::{Enter, Left, Right};
    let mut app = App::new(Config::default(), Lang::En);
    open_tab(&mut app, "Shell");
    let dirs = "Completion Dirs (: separated)";
    assert_adjust_ignored(&mut app, dirs);
    assert_starts_edit(&mut app, dirs, EditTarget::CompletionDirs, Some(""), false);
    for (key, value) in [(Right, "1"), (Left, "auto"), (Left, "0"), (Enter, "auto")] {
        assert_eq!(
            act(&mut app, "Terminal Integration", key),
            ["/shell/term_shell_integration"]
        );
        assert_eq!(app.config.shell.term_shell_integration, value);
        assert!(app.dirty);
    }
    // 表示言語は設定と画面の言語を同時に切り替える（日本語では見出しも変わる）
    select_row(&mut app, "Interface Language");
    for (key, language, lang) in [
        (Right, UiLanguage::En, Lang::En),
        (Right, UiLanguage::Ja, Lang::Ja),
        (Left, UiLanguage::En, Lang::En),
        (Enter, UiLanguage::Ja, Lang::Ja),
        (Left, UiLanguage::En, Lang::En),
    ] {
        app.dirty = false;
        let before = snapshot(&app);
        press(&mut app, key);
        assert_eq!(changed_paths(&before, &snapshot(&app)), ["/ui/language"]);
        assert_eq!(app.config.ui.language, language);
        assert_eq!(app.lang, lang);
        assert!(app.dirty);
    }
    app.config.ui.language = UiLanguage::Auto;
    press(&mut app, Left);
    assert_eq!(app.config.ui.language, UiLanguage::Ja);
    assert_eq!(app.lang, Lang::Ja);
    app.config.ui.language = UiLanguage::Ja;
    press(&mut app, Right);
    assert_eq!(app.config.ui.language, UiLanguage::Auto);
    app.config.ui.language = UiLanguage::Ja;
    press(&mut app, Left);
    assert_eq!(app.lang, Lang::En);
    // 端末フォント設定は未保存の変更があると保存を促し、無ければ設定画面を閉じて起動する
    let font = "Terminal Font Setup";
    assert_adjust_ignored(&mut app, font);
    select_row(&mut app, font);
    app.dirty = true;
    press(&mut app, Enter);
    assert!(!app.launch_font_wizard);
    assert_eq!(app.status_msg.as_deref(), Some("Save settings first (s)."));
    app.dirty = false;
    press(&mut app, Enter);
    assert!(app.launch_font_wizard);
}

#[test]
fn 循環する項目は不明な値を先頭とみなして前後に巡回する() {
    for (tab, label, field, first_next, last) in [
        (
            "Prompt",
            "Prompt Style",
            prompt_style as fn(&mut Config) -> &mut String,
            "classic",
            "pure",
        ),
        ("Prompt", "Font Level", font_level, "powerline", "ascii"),
        ("Suggest", "Strategy", strategy, "substring", "fuzzy"),
        (
            "Style",
            "Rainbow palette",
            rainbow_palette_name,
            "blue",
            "grayscale",
        ),
        (
            "Shell",
            "Terminal Integration",
            term_shell_integration,
            "1",
            "0",
        ),
    ] {
        let mut app = App::new(Config::default(), Lang::En);
        open_tab(&mut app, tab);
        for (key, expected) in [
            (KeyCode::Right, first_next),
            (KeyCode::Left, last),
            (KeyCode::Enter, first_next),
        ] {
            *field(&mut app.config) = "unknown".into();
            act(&mut app, label, key);
            assert_eq!(field(&mut app.config), expected, "{label} {key:?}");
            assert!(app.dirty, "{label} {key:?}");
        }
    }
}

#[test]
fn カスタムタブの左右は選択中の項目に関わらず定義を切り替え決定で編集を始める() {
    use KeyCode::{Enter, Left, Right};
    let mut config = Config::default();
    config.prompt.custom = vec![custom("first"), custom("second")];
    let mut app = App::new(config, Lang::En);
    open_tab(&mut app, "Custom");
    let selector = "Selected (Left/Right)";
    assert!(act(&mut app, selector, Enter).is_empty());
    assert!(app.editing.is_none());
    assert!(!app.dirty);
    for label in [selector, "Command", "When"] {
        for (key, shown) in [
            (Right, "2/2: second"),
            (Right, "1/2: first"),
            (Left, "2/2: second"),
            (Left, "1/2: first"),
        ] {
            assert!(act(&mut app, label, key).is_empty());
            assert!(!app.dirty);
            assert_eq!(field_rows(&mut app).0[0], format!("{selector}: {shown}"));
            assert!(focused_row(&mut app).unwrap().starts_with(label));
        }
    }
    for (label, field, value, picker) in [
        ("Name", CustomField::Name, "first", false),
        ("Command", CustomField::Command, ":", false),
        ("Icon", CustomField::Icon, "", false),
        ("Foreground", CustomField::Fg, "white", true),
        ("Background", CustomField::Bg, "238", true),
        ("When", CustomField::When, "always", false),
    ] {
        assert_starts_edit(
            &mut app,
            label,
            EditTarget::Custom(0, field),
            Some(value),
            picker,
        );
    }
    act(&mut app, selector, Right);
    assert_starts_edit(
        &mut app,
        "Name",
        EditTarget::Custom(1, CustomField::Name),
        Some("second"),
        false,
    );
    // 作成すると新しい定義の名前の行を選ぶ
    press(&mut app, KeyCode::Char('n'));
    assert!(app.dirty);
    assert_eq!(
        field_rows(&mut app).0[0],
        format!("{selector}: 3/3: custom_1")
    );
    assert_eq!(focused_row(&mut app).unwrap(), "Name: custom_1");
}

#[test]
fn カスタムの削除確認は他のキーで取り消し定義が無ければ始めない() {
    let mut config = Config::default();
    config.prompt.custom = vec![custom("only")];
    let mut app = App::new(config, Lang::En);
    open_tab(&mut app, "Custom");
    press(&mut app, KeyCode::Backspace);
    assert_eq!(
        app.status_msg.as_deref(),
        Some("only: Delete definition and placements?")
    );
    // 確認中の他のキーは取消しにだけ使い、タブも切り替えない
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.config.prompt.custom.len(), 1);
    assert_eq!(tab_title(&mut app), "Custom");
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Delete);
    assert!(app.config.prompt.custom.is_empty());
    let (rows, focused) = field_rows(&mut app);
    assert!(rows[0].starts_with("No custom segments."));
    assert_eq!(focused, None);
    app.dirty = false;
    press(&mut app, KeyCode::Backspace);
    assert!(app.status_msg.is_none());
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(app.config.prompt.custom.len(), 1);
}

#[test]
fn 補完の詳細画面の各項目は左右と決定で決まった操作をする() {
    use KeyCode::{Enter, Left, Right};
    use completions::Field;
    let mut app = App::new(
        Config {
            completions: vec![registration("alpha"), registration("beta")],
            ..Default::default()
        },
        Lang::En,
    );
    open_tab(&mut app, "Completions");
    // 一覧では左右キーで何も変わらない
    for key in [Left, Right] {
        press(&mut app, key);
        assert!(!app.completion_detail);
        assert!(!app.dirty);
    }
    press(&mut app, Enter);
    // 先頭の行は左右で登録を巡回する
    let back = "Back to registered CLIs";
    for (key, shown) in [
        (Right, "2 / 2  beta"),
        (Right, "1 / 2  alpha"),
        (Left, "2 / 2  beta"),
        (Left, "1 / 2  alpha"),
    ] {
        assert!(act(&mut app, back, key).is_empty());
        assert!(!app.dirty);
        assert!(focused_row(&mut app).unwrap().contains(shown), "{key:?}");
    }
    assert_adjust_ignored(&mut app, "CLI command name");
    assert_starts_edit(
        &mut app,
        "CLI command name",
        EditTarget::Completion(0, Field::Command),
        Some("alpha"),
        false,
    );
    // 有効と補完元は左右でも決定と同じく切り替わり、補完元は左でも次へ進む
    for key in [Left, Right, Enter] {
        assert_eq!(act(&mut app, "Enabled", key), ["/completions/0/enabled"]);
        assert!(app.dirty);
    }
    for (key, source) in [
        (Left, Source::Generator),
        (Right, Source::File),
        (Enter, Source::Native),
        (Enter, Source::Help),
    ] {
        assert_eq!(act(&mut app, "Source", key), ["/completions/0/source"]);
        assert_eq!(app.config.completions[0].source, source);
        assert!(app.dirty);
    }
    // 補完元がヘルプのときは入力欄を編集しない
    let input = "Generator argv (JSON array) / completion file";
    assert_adjust_ignored(&mut app, input);
    assert!(act(&mut app, input, Enter).is_empty());
    assert!(app.editing.is_none());
    app.config.completions[0].source = Source::Generator;
    assert_starts_edit(
        &mut app,
        input,
        EditTarget::Completion(0, Field::Input),
        Some("[]"),
        false,
    );
    let watch = "Extra files to watch (JSON array)";
    assert_adjust_ignored(&mut app, watch);
    assert_starts_edit(
        &mut app,
        watch,
        EditTarget::Completion(0, Field::Watch),
        Some("[]"),
        false,
    );
    // 再生成の行は未保存の変更があると保存を促す
    for label in [
        "Rebuild changed caches (r)",
        "Force rebuild caches (Shift+R)",
    ] {
        assert_adjust_ignored(&mut app, label);
        select_row(&mut app, label);
        app.dirty = true;
        press(&mut app, Enter);
        assert_eq!(app.status_msg.as_deref(), Some("Save settings first (s)."));
        assert!(app.completion_job.is_none());
    }
    // 先頭の行の決定と Esc は一覧へ戻り、設定画面は閉じない
    select_row(&mut app, back);
    press(&mut app, Enter);
    assert!(!app.completion_detail);
    press(&mut app, Enter);
    assert!(app.completion_detail);
    app.dirty = false;
    press(&mut app, KeyCode::Esc);
    assert!(!app.completion_detail);
    assert!(handle_event(&mut app, KeyCode::Esc.into()));
}

#[test]
fn 補完の一覧は上下で登録を選び詳細を開くとcli名を選ぶ() {
    let mut app = App::new(
        Config {
            completions: vec![registration("alpha"), registration("beta")],
            ..Default::default()
        },
        Lang::En,
    );
    open_tab(&mut app, "Completions");
    for (key, index) in [
        (KeyCode::Down, 1),
        (KeyCode::Down, 1),
        (KeyCode::Up, 0),
        (KeyCode::Up, 0),
        (KeyCode::Char('j'), 1),
    ] {
        press(&mut app, key);
        assert_eq!(app.completion_index, index);
    }
    press(&mut app, KeyCode::Enter);
    assert!(
        focused_row(&mut app)
            .unwrap()
            .starts_with("CLI command name: beta")
    );
}

#[test]
fn 補完の新規登録の取消しは直前の未保存状態に戻し確定後は取り消さない() {
    use completions::Field;
    let mut app = App::new(
        Config {
            completions: vec![],
            ..Default::default()
        },
        Lang::En,
    );
    open_tab(&mut app, "Completions");
    app.dirty = true;
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(app.editing, Some(EditTarget::Completion(0, Field::Command)));
    assert_eq!(app.edit_buffer, "cli1");
    press(&mut app, KeyCode::Esc);
    assert!(app.config.completions.is_empty());
    assert!(app.dirty);
    assert!(!app.completion_detail);
    app.dirty = false;
    press(&mut app, KeyCode::Char('n'));
    replace_input(&mut app, "tool");
    press(&mut app, KeyCode::Enter);
    assert!(app.editing.is_none());
    assert!(app.completion_detail);
    assert_eq!(focused_row(&mut app).unwrap(), "CLI command name: tool");
    // 確定済みの登録は、後で CLI 名の編集を取り消しても消えない
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.config.completions.len(), 1);
    assert!(app.completion_detail);
}

#[test]
fn タブを移ると補完の詳細表示を閉じる() {
    let mut app = App::new(Config::default(), Lang::En);
    open_tab(&mut app, "Completions");
    press(&mut app, KeyCode::Enter);
    assert!(app.completion_detail);
    press(&mut app, KeyCode::Tab);
    assert_eq!(tab_title(&mut app), "Prompt");
    press(&mut app, KeyCode::BackTab);
    assert!(!app.completion_detail);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::BackTab);
    press(&mut app, KeyCode::Tab);
    assert!(!app.completion_detail);
}

#[test]
fn 配置タブの決定は追加候補の列でだけ追加する() {
    let mut app = App::new(Config::default(), Lang::En);
    open_tab(&mut app, "Segments");
    let before = snapshot(&app);
    for _ in 0..2 {
        press(&mut app, KeyCode::Enter);
        assert_eq!(snapshot(&app), before);
        assert!(!app.dirty);
        press(&mut app, KeyCode::Right);
    }
    // 右側の列を経由して追加候補へ移ると右側へ追加する
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        changed_paths(&before, &snapshot(&app)),
        ["/prompt/right_segments"]
    );
    assert_eq!(app.config.prompt.right_segments.len(), 4);
    assert!(app.dirty);
}

#[test]
fn ヘルプ行はタブと状態に応じて切り替わる() {
    let mut config = Config::default();
    config.prompt.custom.push(custom("custom_1"));
    let mut app = App::new(config, Lang::En);
    let common = "Tab:switch  jk:nav  hl:change  Enter:edit/toggle  S:save  Esc:quit";
    for (tab, help) in [
        ("Prompt", common),
        ("Segments", "Tab:switch  hjkl:nav  L/R:add to side"),
        ("Git", common),
        ("Suggest", common),
        ("Style", common),
        ("Custom", "Tab:switch  jk:field  hl:select  n:new"),
        ("Shell", common),
        ("Completions", "↑/↓:select  Enter:settings"),
    ] {
        open_tab(&mut app, tab);
        assert!(
            help_line(&mut app).contains(help),
            "{tab}: {}",
            help_line(&mut app)
        );
    }
    press(&mut app, KeyCode::Enter);
    assert!(help_line(&mut app).contains("↑/↓:field  Enter:edit"));
    press(&mut app, KeyCode::Char('d'));
    assert!(help_line(&mut app).contains("⌫/d again: remove selected CLI"));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Enter);
    assert!(help_line(&mut app).contains("Enter:confirm  Esc:cancel"));
    press(&mut app, KeyCode::Esc);
    open_tab(&mut app, "Custom");
    press(&mut app, KeyCode::Delete);
    assert!(help_line(&mut app).contains("⌫ again: remove definition"));
    press(&mut app, KeyCode::Esc);
    app.dirty = true;
    press(&mut app, KeyCode::Esc);
    assert!(help_line(&mut app).contains("Unsaved changes! Press Esc again"));
}

#[test]
fn 空行数を選んだときだけ間隔のプレビューを表示する() {
    let mut config = Config::default();
    config.prompt.left_segments = vec!["dir".into()];
    config.prompt.right_segments.clear();
    let mut app = App::new(config, Lang::En);
    let prompts = |app: &mut App| {
        screen_lines(&draw(app))
            .iter()
            .filter(|(text, _)| text.contains("~/project"))
            .count()
    };
    assert_eq!(prompts(&mut app), 1);
    select_row(&mut app, "Blank lines between prompts (0–10)");
    assert_eq!(prompts(&mut app), 2);
    select_row(&mut app, "Command input");
    assert_eq!(prompts(&mut app), 1);
    select_row(&mut app, "Blank lines between prompts (0–10)");
    open_tab(&mut app, "Git");
    assert_eq!(prompts(&mut app), 1);
}

#[test]
fn カスタム名の変更は組込み名や重複名の配置と配色を奪わない() {
    for (customs, renamed, other) in [
        (vec![custom("dir")], 0, None),
        (vec![custom("dup"), custom("dup")], 0, Some(1)),
    ] {
        let old = customs[0].name.clone();
        let mut config = Config::default();
        config.prompt.custom = customs;
        config.prompt.left_segments = vec![old.clone(), "git".into()];
        config.style.rainbow_overrides.insert(
            old.clone(),
            SegmentColors {
                fg: None,
                bg: Some("88".into()),
            },
        );
        let mut app = App::new(config, Lang::En);
        open_tab(&mut app, "Custom");
        select_row(&mut app, "Name");
        press(&mut app, KeyCode::Enter);
        replace_input(&mut app, "mine");
        press(&mut app, KeyCode::Enter);
        assert!(app.editing.is_none(), "{:?}", app.status_msg);
        assert_eq!(app.config.prompt.custom[renamed].name, "mine");
        assert_eq!(app.config.prompt.left_segments, [old.as_str(), "git"]);
        assert!(app.config.style.rainbow_overrides.contains_key(&old));
        assert!(!app.config.style.rainbow_overrides.contains_key("mine"));
        if let Some(other) = other {
            assert_eq!(app.config.prompt.custom[other].name, old);
        }
    }
}

#[test]
fn 編集の検証に失敗すると値と未保存の印を変えずに編集を続ける() {
    let mut app = App::new(Config::default(), Lang::En);
    select_row(&mut app, "Truncation Length");
    press(&mut app, KeyCode::Enter);
    replace_input(&mut app, "x");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.editing, Some(EditTarget::TruncationLength));
    assert_eq!(app.edit_buffer, "x");
    assert_eq!(app.config.prompt.dir.truncation_length, 3);
    assert!(!app.dirty);
    assert!(app.status_msg.is_some());
    replace_input(&mut app, "4");
    press(&mut app, KeyCode::Enter);
    assert!(app.editing.is_none());
    assert_eq!(app.config.prompt.dir.truncation_length, 4);
    assert!(app.dirty);
}
