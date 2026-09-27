use crate::config::{self, Config, CustomSegment};
use crate::icons::{FontLevel, Icons};
use crate::style::{self, Color as AppColor};

use crate::prompt::{RAINBOW_PALETTES, rainbow_colors, rainbow_palette};
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs};
use std::time::Duration;

mod block_colors;
mod colors;
mod completions;
mod lang;
use colors::{ColorPicker, color_field_item};
use lang::Lang;

pub fn history_menu_help() -> &'static str {
    Lang::from_env().text(
        "Up/Down Tab: select  Enter: insert  Esc: cancel",
        "↑/↓ Tab:選択  Enter:入力欄へ  Esc:取消",
    )
}

pub fn history_menu_empty() -> &'static str {
    Lang::from_env().text("No matching history", "一致する履歴がありません")
}

#[cfg(test)]
#[path = "tui/settings_tests.rs"]
mod settings_tests;

#[cfg(test)]
#[path = "tui/language_tests.rs"]
mod language_tests;

// ---------------------------------------------------------------------------
// 定数
// ---------------------------------------------------------------------------

const TAB_TITLES: &[&str] = &[
    "Prompt",
    "Segments",
    "Git",
    "Suggest",
    "Style",
    "Custom",
    "Shell",
    "Completions",
];

const PROMPT_STYLES: &[&str] = &["lean", "classic", "rainbow", "pure"];
const FONT_LEVELS: &[&str] = &["nerd", "powerline", "unicode", "ascii"];
const SUGGEST_STRATEGIES: &[&str] = &["prefix", "substring", "fuzzy"];
const SHELL_INTEGRATIONS: &[&str] = &["auto", "1", "0"];

// セグメント名一覧は prompt::ALL_SEGMENT_NAMES を単一ソースとする
const ALL_SEGMENTS: &[&str] = crate::prompt::ALL_SEGMENT_NAMES;

// ---------------------------------------------------------------------------
// 編集対象の追跡
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum EditTarget {
    HomeSymbol,
    TruncationSymbol,
    TruncationLength,
    BlankLines,
    MaxSuggestions,
    IpInterface,
    CompletionDirs,
    Completion(usize, completions::Field),
    Custom(usize, CustomField),
    HighlightColor,
    PrimaryColor,
    SuccessColor,
    ErrorColor,
    MutedColor,
    RainbowFg(String),
    RainbowBg(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CustomField {
    Name,
    Command,
    Icon,
    Fg,
    Bg,
    When,
}

impl EditTarget {
    fn is_color(&self) -> bool {
        matches!(
            self,
            Self::HighlightColor
                | Self::PrimaryColor
                | Self::SuccessColor
                | Self::ErrorColor
                | Self::MutedColor
                | Self::RainbowFg(_)
                | Self::RainbowBg(_)
                | Self::Custom(_, CustomField::Fg | CustomField::Bg)
        )
    }

    fn label(&self, lang: Lang) -> &'static str {
        match self {
            Self::BlankLines => lang.text(
                "Blank lines between prompts (0–10)",
                "プロンプト間の空行（0–10）",
            ),
            Self::HomeSymbol => lang.text("Home Symbol", "ホームの記号"),
            Self::TruncationSymbol => lang.text("Truncation Symbol", "省略記号"),
            Self::TruncationLength => lang.text("Truncation Length", "表示する階層数"),
            Self::MaxSuggestions => lang.text("Max Suggestions", "候補の最大数"),
            Self::IpInterface => lang.text(
                "IP Interface (empty: automatic)",
                "IP インターフェース（空欄で自動）",
            ),
            Self::Completion(_, field) => field.label(lang),
            Self::CompletionDirs => lang.text(
                "Completion Directories (colon-separated absolute paths)",
                "補完ディレクトリ（絶対パスを : で区切る）",
            ),
            Self::Custom(_, field) => field.label(lang),
            Self::HighlightColor => {
                lang.text("Highlight Color (e.g. fg=8)", "候補の色（例: fg=8）")
            }
            Self::PrimaryColor => lang.text("Primary Color", "基本色"),
            Self::SuccessColor => lang.text("Success Color", "成功の色"),
            Self::ErrorColor => lang.text("Error Color", "エラーの色"),
            Self::RainbowFg(_) => lang.text(
                "Block foreground (empty: palette)",
                "ブロックの文字色（空欄でセットの色）",
            ),
            Self::RainbowBg(_) => lang.text(
                "Block background (empty: palette)",
                "ブロックの背景色（空欄でセットの色）",
            ),
            Self::MutedColor => lang.text("Muted Color", "補助色"),
        }
    }
}

impl CustomField {
    const ALL: [Self; 6] = [
        Self::Name,
        Self::Command,
        Self::Icon,
        Self::Fg,
        Self::Bg,
        Self::When,
    ];

    fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Name => lang.text("Name", "名前"),
            Self::Command => lang.text("Command", "コマンド"),
            Self::Icon => lang.text("Icon", "アイコン"),
            Self::Fg => lang.text("Foreground", "文字色"),
            Self::Bg => lang.text("Background", "背景色"),
            Self::When => lang.text("When", "表示条件"),
        }
    }

    fn value(self, custom: &mut CustomSegment) -> &mut String {
        match self {
            Self::Name => &mut custom.name,
            Self::Command => &mut custom.command,
            Self::Icon => &mut custom.icon,
            Self::Fg => &mut custom.fg,
            Self::Bg => &mut custom.bg,
            Self::When => &mut custom.when,
        }
    }
}

// ---------------------------------------------------------------------------
// セグメントタブの列
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegColumn {
    Left,
    Right,
    Available,
}

// ---------------------------------------------------------------------------
// 終了確認
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuitState {
    Normal,
    Confirming,
}

// ---------------------------------------------------------------------------
// アプリケーション状態
// ---------------------------------------------------------------------------

struct App {
    lang: Lang,
    config: Config,
    tab: usize,
    dirty: bool,

    // タブごとのフォーカス位置
    prompt_focus: usize,
    git_focus: usize,
    suggest_focus: usize,
    style_focus: usize,
    rainbow_index: usize,
    custom_focus: usize,
    shell_focus: usize,
    completion_focus: usize,
    completion_index: usize,
    completion_job: Option<std::sync::mpsc::Receiver<String>>,
    custom_index: usize,
    custom_delete_pending: bool,

    // セグメントタブ
    seg_column: SegColumn,
    seg_last_side: SegColumn, // 利用可能リストから追加する際の直近の左右列
    seg_left_state: ListState,
    seg_right_state: ListState,
    seg_avail_state: ListState,

    // テキスト編集
    editing: Option<EditTarget>,
    edit_buffer: String,
    edit_cursor: usize,
    color_picker: Option<ColorPicker>,

    // 終了
    quit_state: QuitState,

    // ステータスメッセージ
    status_msg: Option<String>,
}

impl App {
    fn new(config: Config, lang: Lang) -> Self {
        let mut seg_left_state = ListState::default();
        if !config.prompt.left_segments.is_empty() {
            seg_left_state.select(Some(0));
        }
        let mut seg_right_state = ListState::default();
        if !config.prompt.right_segments.is_empty() {
            seg_right_state.select(Some(0));
        }
        let avail = available_segments(&config);
        let mut seg_avail_state = ListState::default();
        if !avail.is_empty() {
            seg_avail_state.select(Some(0));
        }
        Self {
            lang,
            config,
            tab: 0,
            dirty: false,
            prompt_focus: 0,
            git_focus: 0,
            suggest_focus: 0,
            style_focus: 0,
            rainbow_index: 0,
            custom_focus: 0,
            shell_focus: 0,
            completion_focus: 0,
            completion_index: 0,
            completion_job: None,
            custom_index: 0,
            custom_delete_pending: false,
            seg_column: SegColumn::Left,
            seg_last_side: SegColumn::Left,
            seg_left_state,
            seg_right_state,
            seg_avail_state,
            editing: None,
            edit_buffer: String::new(),
            edit_cursor: 0,
            color_picker: None,
            quit_state: QuitState::Normal,
            status_msg: None,
        }
    }

    fn max_focus(&self) -> usize {
        match self.tab {
            0 => 8,
            2 => 2,
            3 => 2,
            4 => {
                if self.rainbow_selected().is_some() {
                    8
                } else {
                    5
                }
            }
            5 if !self.config.prompt.custom.is_empty() => 6,
            6 => 1,
            7 if !self.config.completions.is_empty() => 7,
            _ => 0,
        }
    }

    fn current_focus(&self) -> usize {
        match self.tab {
            0 => self.prompt_focus,
            2 => self.git_focus,
            3 => self.suggest_focus,
            4 => self.style_focus,
            5 => self.custom_focus,
            6 => self.shell_focus,
            7 => self.completion_focus,
            _ => 0,
        }
    }

    fn set_focus(&mut self, v: usize) {
        match self.tab {
            0 => self.prompt_focus = v,
            2 => self.git_focus = v,
            3 => self.suggest_focus = v,
            4 => self.style_focus = v,
            5 => self.custom_focus = v,
            6 => self.shell_focus = v,
            7 => self.completion_focus = v,
            _ => {}
        }
    }

    fn focus_up(&mut self) {
        if self.tab == 1 {
            self.seg_focus_up();
            return;
        }
        let cur = self.current_focus();
        if cur > 0 {
            self.set_focus(cur - 1);
        }
    }

    fn focus_down(&mut self) {
        if self.tab == 1 {
            self.seg_focus_down();
            return;
        }
        let cur = self.current_focus();
        if cur < self.max_focus() {
            self.set_focus(cur + 1);
        }
    }

    // セグメントタブのナビゲーション補助
    fn seg_focus_up(&mut self) {
        match self.seg_column {
            SegColumn::Left => list_prev(&mut self.seg_left_state),
            SegColumn::Right => list_prev(&mut self.seg_right_state),
            SegColumn::Available => list_prev(&mut self.seg_avail_state),
        }
    }

    fn seg_focus_down(&mut self) {
        let len = match self.seg_column {
            SegColumn::Left => self.config.prompt.left_segments.len(),
            SegColumn::Right => self.config.prompt.right_segments.len(),
            SegColumn::Available => available_segments(&self.config).len(),
        };
        match self.seg_column {
            SegColumn::Left => list_next(&mut self.seg_left_state, len),
            SegColumn::Right => list_next(&mut self.seg_right_state, len),
            SegColumn::Available => list_next(&mut self.seg_avail_state, len),
        }
    }

    fn seg_column_left(&mut self) {
        self.seg_column = match self.seg_column {
            SegColumn::Left => SegColumn::Left,
            SegColumn::Right => SegColumn::Left,
            SegColumn::Available => SegColumn::Right,
        };
        if matches!(self.seg_column, SegColumn::Left | SegColumn::Right) {
            self.seg_last_side = self.seg_column;
        }
    }

    fn seg_column_right(&mut self) {
        self.seg_column = match self.seg_column {
            SegColumn::Left => SegColumn::Right,
            SegColumn::Right => SegColumn::Available,
            SegColumn::Available => SegColumn::Available,
        };
        if matches!(self.seg_column, SegColumn::Left | SegColumn::Right) {
            self.seg_last_side = self.seg_column;
        }
    }

    fn seg_add(&mut self) {
        if self.seg_column != SegColumn::Available {
            return;
        }
        let avail = available_segments(&self.config);
        if let Some(idx) = self.seg_avail_state.selected()
            && idx < avail.len()
        {
            let seg = avail[idx].clone();
            match self.seg_last_side {
                SegColumn::Left | SegColumn::Available => {
                    self.config.prompt.left_segments.push(seg);
                    let len = self.config.prompt.left_segments.len();
                    self.seg_left_state.select(Some(len - 1));
                }
                SegColumn::Right => {
                    self.config.prompt.right_segments.push(seg);
                    let len = self.config.prompt.right_segments.len();
                    self.seg_right_state.select(Some(len - 1));
                }
            }
            self.dirty = true;
            // 利用可能リストのインデックスを修正
            let new_avail = available_segments(&self.config);
            if new_avail.is_empty() {
                self.seg_avail_state.select(None);
            } else if idx >= new_avail.len() {
                self.seg_avail_state.select(Some(new_avail.len() - 1));
            }
        }
    }

    fn seg_add_to(&mut self, side: SegColumn) {
        self.seg_last_side = side;
        if self.seg_column == SegColumn::Available {
            self.seg_add();
            return;
        }
        let selected = match self.seg_column {
            SegColumn::Left => self
                .seg_left_state
                .selected()
                .and_then(|i| self.config.prompt.left_segments.get(i)),
            SegColumn::Right => self
                .seg_right_state
                .selected()
                .and_then(|i| self.config.prompt.right_segments.get(i)),
            SegColumn::Available => None,
        }
        .cloned();
        let Some(name) = selected else {
            return;
        };
        let (segments, state) = match side {
            SegColumn::Left => (
                &mut self.config.prompt.left_segments,
                &mut self.seg_left_state,
            ),
            SegColumn::Right => (
                &mut self.config.prompt.right_segments,
                &mut self.seg_right_state,
            ),
            SegColumn::Available => return,
        };
        if !segments.contains(&name) {
            segments.push(name);
            state.select(Some(segments.len() - 1));
            self.dirty = true;
        }
    }

    fn seg_remove(&mut self) {
        match self.seg_column {
            SegColumn::Left => {
                if let Some(idx) = self.seg_left_state.selected()
                    && idx < self.config.prompt.left_segments.len()
                {
                    self.config.prompt.left_segments.remove(idx);
                    self.dirty = true;
                    fix_list_state(
                        &mut self.seg_left_state,
                        self.config.prompt.left_segments.len(),
                    );
                    let avail = available_segments(&self.config);
                    if self.seg_avail_state.selected().is_none() && !avail.is_empty() {
                        self.seg_avail_state.select(Some(0));
                    }
                }
            }
            SegColumn::Right => {
                if let Some(idx) = self.seg_right_state.selected()
                    && idx < self.config.prompt.right_segments.len()
                {
                    self.config.prompt.right_segments.remove(idx);
                    self.dirty = true;
                    fix_list_state(
                        &mut self.seg_right_state,
                        self.config.prompt.right_segments.len(),
                    );
                    let avail = available_segments(&self.config);
                    if self.seg_avail_state.selected().is_none() && !avail.is_empty() {
                        self.seg_avail_state.select(Some(0));
                    }
                }
            }
            SegColumn::Available => {}
        }
    }

    fn seg_reorder(&mut self, up: bool) {
        match self.seg_column {
            SegColumn::Left => {
                if reorder_vec(
                    &mut self.config.prompt.left_segments,
                    &mut self.seg_left_state,
                    up,
                ) {
                    self.dirty = true;
                }
            }
            SegColumn::Right => {
                if reorder_vec(
                    &mut self.config.prompt.right_segments,
                    &mut self.seg_right_state,
                    up,
                ) {
                    self.dirty = true;
                }
            }
            SegColumn::Available => {}
        }
    }

    fn save(&mut self) {
        if let Err(error) = crate::completion::validate(&self.config.completions) {
            self.status_msg = Some(format!(
                "{}: {error}",
                self.lang.text("Completion settings", "補完設定")
            ));
            return;
        }
        match config::save_config(&self.config) {
            Ok(()) => {
                self.dirty = false;
                crate::completion::refresh_saved_async();
                self.status_msg = Some(
                    self.lang
                        .text("Config saved!", "設定を保存しました。")
                        .into(),
                );
            }
            Err(e) => {
                self.status_msg = Some(format!(
                    "{}: {e}",
                    self.lang.text("Save error", "保存エラー")
                ));
            }
        }
    }

    fn start_edit(&mut self, target: EditTarget) {
        let current = match &target {
            EditTarget::BlankLines => self
                .config
                .prompt
                .blank_lines
                .min(config::MAX_PROMPT_BLANK_LINES)
                .to_string(),
            EditTarget::HomeSymbol => self.config.prompt.dir.home_symbol.clone(),
            EditTarget::TruncationSymbol => self.config.prompt.dir.truncation_symbol.clone(),
            EditTarget::TruncationLength => self.config.prompt.dir.truncation_length.to_string(),
            EditTarget::MaxSuggestions => self.config.suggest.max_suggestions.to_string(),
            EditTarget::IpInterface => self.config.prompt.ip_interface.clone(),
            EditTarget::CompletionDirs => self.config.shell.completion_dirs.clone(),
            EditTarget::Completion(i, field) => field.value(&self.config.completions[*i]),
            EditTarget::Custom(index, field) => {
                field.value(&mut self.config.prompt.custom[*index]).clone()
            }
            EditTarget::HighlightColor => self.config.suggest.highlight_color.clone(),
            EditTarget::PrimaryColor => self.config.style.primary_color.clone(),
            EditTarget::SuccessColor => self.config.style.success_color.clone(),
            EditTarget::ErrorColor => self.config.style.error_color.clone(),
            EditTarget::MutedColor => self.config.style.muted_color.clone(),
            EditTarget::RainbowFg(name) => self.rainbow_color_value(name, true),
            EditTarget::RainbowBg(name) => self.rainbow_color_value(name, false),
        };
        self.edit_cursor = current.len();
        self.edit_buffer = current;
        self.editing = Some(target);
    }

    fn activate_edit(&mut self, target: EditTarget) {
        self.start_edit(target.clone());
        if target.is_color() {
            self.color_picker = Some(ColorPicker::new(&self.edit_buffer, &target));
        }
    }

    fn confirm_edit(&mut self) {
        if let Some(target) = self.editing.clone() {
            let val = self.edit_buffer.clone();
            if let Err(message) = self.validate_edit(&target, &val) {
                self.status_msg = Some(message.into());
                return;
            }
            match target {
                EditTarget::BlankLines => self.config.prompt.blank_lines = val.parse().unwrap(),
                EditTarget::HomeSymbol => self.config.prompt.dir.home_symbol = val,
                EditTarget::TruncationSymbol => self.config.prompt.dir.truncation_symbol = val,
                EditTarget::TruncationLength => {
                    self.config.prompt.dir.truncation_length = val.parse().unwrap()
                }
                EditTarget::MaxSuggestions => {
                    self.config.suggest.max_suggestions = val.parse().unwrap()
                }
                EditTarget::IpInterface => self.config.prompt.ip_interface = val,
                EditTarget::CompletionDirs => self.config.shell.completion_dirs = val,
                EditTarget::Completion(i, field) => {
                    field.set(&mut self.config.completions[i], &val)
                }
                EditTarget::Custom(index, field) => {
                    if field == CustomField::Name {
                        let old_name = self.config.prompt.custom[index].name.clone();
                        // 組込み名や重複名を含む既存設定を修正するときは別の定義の配置を奪わない。
                        if !ALL_SEGMENTS.contains(&old_name.as_str())
                            && !self
                                .config
                                .prompt
                                .custom
                                .iter()
                                .enumerate()
                                .any(|(i, c)| i != index && c.name == old_name)
                        {
                            if let Some(colors) =
                                self.config.style.rainbow_overrides.remove(&old_name)
                            {
                                self.config
                                    .style
                                    .rainbow_overrides
                                    .insert(val.clone(), colors);
                            }
                            for name in self
                                .config
                                .prompt
                                .left_segments
                                .iter_mut()
                                .chain(&mut self.config.prompt.right_segments)
                            {
                                if *name == old_name {
                                    *name = val.clone();
                                }
                            }
                        }
                    }
                    *field.value(&mut self.config.prompt.custom[index]) = val;
                    self.refresh_segment_selection();
                }
                EditTarget::HighlightColor => self.config.suggest.highlight_color = val,
                EditTarget::PrimaryColor => self.config.style.primary_color = val,
                EditTarget::SuccessColor => self.config.style.success_color = val,
                EditTarget::ErrorColor => self.config.style.error_color = val,
                EditTarget::MutedColor => self.config.style.muted_color = val,
                EditTarget::RainbowFg(name) => self.set_rainbow_color(name, true, val),
                EditTarget::RainbowBg(name) => self.set_rainbow_color(name, false, val),
            }
            self.editing = None;
            self.dirty = true;
        }
    }

    fn validate_edit(&self, target: &EditTarget, value: &str) -> Result<(), &'static str> {
        match target {
            EditTarget::Completion(i, field) => {
                return completions::validate(self, *i, *field, value);
            }
            EditTarget::BlankLines => {
                if !value
                    .parse::<usize>()
                    .is_ok_and(|n| n <= config::MAX_PROMPT_BLANK_LINES)
                {
                    return Err(self.lang.text(
                        "Enter a whole number from 0 to 10.",
                        "0〜10 の整数を入力してください。",
                    ));
                }
            }
            EditTarget::TruncationLength | EditTarget::MaxSuggestions => {
                value.parse::<usize>().map_err(|_| {
                    self.lang.text(
                        "Enter a non-negative whole number within the supported range.",
                        "指定可能な範囲の 0 以上の整数を入力してください。",
                    )
                })?;
            }
            EditTarget::Custom(index, CustomField::Name) => {
                if value.trim().is_empty() || value.chars().any(char::is_control) {
                    return Err(self.lang.text(
                        "Name must not be empty or contain control characters.",
                        "名前を空欄にしたり、制御文字を含めたりすることはできません。",
                    ));
                }
                if ALL_SEGMENTS.contains(&value)
                    || self
                        .config
                        .prompt
                        .custom
                        .iter()
                        .enumerate()
                        .any(|(i, c)| i != *index && c.name == value)
                {
                    return Err(self.lang.text(
                        "Name is already used by another segment.",
                        "この名前は別のセグメントで使われています。",
                    ));
                }
            }
            EditTarget::Custom(_, CustomField::When)
                if !value.is_empty()
                    && value != "always"
                    && !value.strip_prefix("env:").is_some_and(|s| !s.is_empty())
                    && !value.strip_prefix("file:").is_some_and(|s| !s.is_empty()) =>
            {
                return Err(self.lang.text(
                    "Use always, env:VARIABLE, or file:path.",
                    "always、env:変数名、file:パス のいずれかを指定してください。",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn custom_add(&mut self) {
        let name = (1..)
            .map(|i| format!("custom_{i}"))
            .find(|name| {
                !ALL_SEGMENTS.contains(&name.as_str())
                    && !self.config.prompt.custom.iter().any(|c| c.name == *name)
                    && !self.config.prompt.left_segments.contains(name)
                    && !self.config.prompt.right_segments.contains(name)
            })
            .unwrap();
        self.config.prompt.custom.push(CustomSegment {
            name,
            command: ":".into(),
            icon: String::new(),
            fg: "white".into(),
            bg: "238".into(),
            when: "always".into(),
        });
        self.custom_index = self.config.prompt.custom.len() - 1;
        self.custom_focus = 1;
        self.refresh_segment_selection();
        self.dirty = true;
        self.status_msg = Some(
            self.lang
                .text(
                    "Created. Edit fields, then add it in Segments.",
                    "作成しました。項目を編集し、「配置」で追加してください。",
                )
                .into(),
        );
    }

    fn custom_remove(&mut self) {
        if self.custom_index >= self.config.prompt.custom.len() {
            return;
        }
        let removed = self.config.prompt.custom.remove(self.custom_index);
        if !ALL_SEGMENTS.contains(&removed.name.as_str())
            && !self
                .config
                .prompt
                .custom
                .iter()
                .any(|c| c.name == removed.name)
        {
            self.config.style.rainbow_overrides.remove(&removed.name);
            self.config
                .prompt
                .left_segments
                .retain(|name| *name != removed.name);
            self.config
                .prompt
                .right_segments
                .retain(|name| *name != removed.name);
        }
        self.custom_index = self
            .custom_index
            .min(self.config.prompt.custom.len().saturating_sub(1));
        if self.config.prompt.custom.is_empty() {
            self.custom_focus = 0;
        }
        self.refresh_segment_selection();
        self.dirty = true;
    }

    fn refresh_segment_selection(&mut self) {
        fix_list_state(
            &mut self.seg_left_state,
            self.config.prompt.left_segments.len(),
        );
        fix_list_state(
            &mut self.seg_right_state,
            self.config.prompt.right_segments.len(),
        );
        let len = available_segments(&self.config).len();
        fix_list_state(&mut self.seg_avail_state, len);
        if len > 0 && self.seg_avail_state.selected().is_none() {
            self.seg_avail_state.select(Some(0));
        }
    }

    fn custom_select(&mut self, next: bool) {
        let len = self.config.prompt.custom.len();
        if len > 0 {
            self.custom_index = if next {
                (self.custom_index + 1) % len
            } else {
                (self.custom_index + len - 1) % len
            };
        }
    }

    fn cancel_edit(&mut self) {
        self.editing = None;
        self.edit_buffer.clear();
    }
}

// ---------------------------------------------------------------------------
// ヘルパー関数
// ---------------------------------------------------------------------------

fn available_segments(config: &Config) -> Vec<String> {
    let mut names: Vec<String> = ALL_SEGMENTS
        .iter()
        .copied()
        .chain(config.prompt.custom.iter().map(|c| c.name.as_str()))
        .filter(|s| {
            !config.prompt.left_segments.contains(&s.to_string())
                && !config.prompt.right_segments.contains(&s.to_string())
        })
        .map(|s| s.to_string())
        .collect();
    let mut seen = std::collections::HashSet::new();
    names.retain(|name| seen.insert(name.clone()));
    names
}

fn list_prev(state: &mut ListState) {
    if let Some(i) = state.selected()
        && i > 0
    {
        state.select(Some(i - 1));
    }
}

fn list_next(state: &mut ListState, len: usize) {
    if len == 0 {
        return;
    }
    if let Some(i) = state.selected()
        && i + 1 < len
    {
        state.select(Some(i + 1));
    } else if state.selected().is_none() {
        state.select(Some(0));
    }
}

fn fix_list_state(state: &mut ListState, len: usize) {
    if len == 0 {
        state.select(None);
    } else if let Some(i) = state.selected()
        && i >= len
    {
        state.select(Some(len - 1));
    }
}

fn reorder_vec(v: &mut [String], state: &mut ListState, up: bool) -> bool {
    if let Some(idx) = state.selected() {
        if up && idx > 0 {
            v.swap(idx, idx - 1);
            state.select(Some(idx - 1));
            return true;
        } else if !up && idx + 1 < v.len() {
            v.swap(idx, idx + 1);
            state.select(Some(idx + 1));
            return true;
        }
    }
    false
}

fn cycle_next(current: &str, options: &[&str]) -> String {
    let idx = options.iter().position(|&o| o == current).unwrap_or(0);
    options[(idx + 1) % options.len()].to_string()
}

fn cycle_prev(current: &str, options: &[&str]) -> String {
    let idx = options.iter().position(|&o| o == current).unwrap_or(0);
    options[(idx + options.len() - 1) % options.len()].to_string()
}

fn to_ratatui_color(c: AppColor) -> Color {
    match c {
        AppColor::Black => Color::Black,
        AppColor::Red => Color::Red,
        AppColor::Green => Color::Green,
        AppColor::Yellow => Color::Yellow,
        AppColor::Blue => Color::Blue,
        AppColor::Magenta => Color::Magenta,
        AppColor::Cyan => Color::Cyan,
        AppColor::White => Color::White,
        AppColor::BrightBlack => Color::DarkGray,
        AppColor::BrightRed => Color::LightRed,
        AppColor::BrightGreen => Color::LightGreen,
        AppColor::BrightYellow => Color::LightYellow,
        AppColor::BrightBlue => Color::LightBlue,
        AppColor::BrightMagenta => Color::LightMagenta,
        AppColor::BrightCyan => Color::LightCyan,
        AppColor::BrightWhite => Color::White,
        AppColor::Ansi256(n) => Color::Indexed(n),
        AppColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

// ---------------------------------------------------------------------------
// プレビュー描画
// ---------------------------------------------------------------------------

fn render_preview(config: &Config, width: u16, edit: Option<(&EditTarget, &str)>) -> Text<'static> {
    let color = |target: EditTarget, value: &str| {
        let value = edit
            .filter(|(t, _)| **t == target)
            .map_or(value, |(_, v)| v);
        to_ratatui_color(style::parse_color(value))
    };
    let primary = color(EditTarget::PrimaryColor, &config.style.primary_color);
    let success = color(EditTarget::SuccessColor, &config.style.success_color);
    let muted = color(EditTarget::MutedColor, &config.style.muted_color);
    let icons = Icons::for_level(FontLevel::from_str(&config.prompt.font_level));

    // 実コマンドを実行せず、各セグメントの位置をサンプルで保持する。
    let segment_text = |name: &str| -> Option<String> {
        match name {
            "os_icon" => Some(icons.os_icon.into()),
            "dir" => Some(format!("{}/project", config.prompt.dir.home_symbol)),
            "git" => Some(format!("{} main +1!2", icons.git_branch)),
            "status" => Some(icons.prompt_ok.into()),
            "duration" => Some(format!("{} 2.5s", icons.duration)),
            "time" => Some("12:34:56".into()),
            _ if ALL_SEGMENTS.contains(&name) => Some(format!("[{name}]")),
            _ => config
                .prompt
                .custom
                .iter()
                .find(|c| c.name == name)
                .map(|c| format!("{} [{}]", c.icon, c.name)),
        }
    };
    let segment_colors = |name: &str| -> (Color, Color) {
        if let Some((index, custom)) = config
            .prompt
            .custom
            .iter()
            .enumerate()
            .find(|(_, c)| c.name == name)
        {
            return (
                color(EditTarget::Custom(index, CustomField::Fg), &custom.fg),
                color(EditTarget::Custom(index, CustomField::Bg), &custom.bg),
            );
        }
        match name {
            "dir" => (primary, primary),
            "duration" | "time" => (muted, Color::Indexed(236)),
            "status" => (success, Color::Indexed(236)),
            _ => (Color::Green, Color::Indexed(240)),
        }
    };
    let left_parts: Vec<_> = config
        .prompt
        .left_segments
        .iter()
        .filter_map(|name| segment_text(name).map(|text| (name.as_str(), text)))
        .collect();
    let segment_style = |index: usize, name: &str| {
        if config.prompt.prompt_style == "rainbow" {
            let (mut fg, mut bg) = rainbow_colors(&config.style, name, index);
            if let Some((target, value)) = edit {
                let base = rainbow_palette(&config.style.rainbow_palette);
                let inherited = base[index % base.len()];
                match target {
                    EditTarget::RainbowFg(block) if block == name => {
                        fg = if value.trim().is_empty() {
                            inherited.0
                        } else {
                            style::parse_color(value)
                        };
                    }
                    EditTarget::RainbowBg(block) if block == name => {
                        bg = if value.trim().is_empty() {
                            inherited.1
                        } else {
                            style::parse_color(value)
                        };
                    }
                    _ => {}
                }
            }
            (to_ratatui_color(fg), to_ratatui_color(bg))
        } else {
            let (fg, bg) = segment_colors(name);
            (if name == "dir" { Color::White } else { fg }, bg)
        }
    };
    let mut left = Line::default();
    for (i, (name, text)) in left_parts.iter().enumerate() {
        if matches!(config.prompt.prompt_style.as_str(), "classic" | "rainbow") {
            let (fg, bg) = segment_style(i, name);
            left.push_span(Span::styled(
                format!(" {text} "),
                Style::default().fg(fg).bg(bg),
            ));
            let next_bg = left_parts
                .get(i + 1)
                .map(|(name, _)| segment_style(i + 1, name).1);
            let mut style = Style::default().fg(bg);
            if let Some(bg) = next_bg {
                style = style.bg(bg);
            }
            left.push_span(Span::styled(icons.separator_left, style));
        } else {
            if i > 0 {
                left.push_span(" ");
            }
            left.push_span(Span::styled(
                text.clone(),
                Style::default().fg(segment_colors(name).0),
            ));
        }
    }
    let mut lines = Vec::new();
    if config.prompt.newline {
        lines.push(left);
        left = Line::default();
    } else if !left.spans.is_empty() {
        left.push_span(" ");
    }
    let prompt_char = if config.prompt.prompt_style == "pure" {
        "❯"
    } else {
        icons.prompt_ok
    };
    left.push_span(Span::styled(
        format!("{prompt_char} "),
        Style::default().fg(success).bold(),
    ));
    // 右プロンプトは zsh と同じく入力行に置き、収まらない場合は隠す。
    let mut right = Line::default();
    for name in &config.prompt.right_segments {
        if let Some(text) = segment_text(name) {
            if !right.spans.is_empty() {
                right.push_span(" ");
            }
            right.push_span(Span::styled(
                text,
                Style::default().fg(segment_colors(name).0),
            ));
        }
    }
    if right.width() > 0 && left.width() + right.width() < usize::from(width) {
        left.push_span(" ".repeat(usize::from(width) - left.width() - right.width()));
        left.spans.extend(right.spans);
    }
    lines.push(left);
    Text::from(lines)
}

// ---------------------------------------------------------------------------
// UI 描画
// ---------------------------------------------------------------------------

fn render_spacing_preview(
    config: &Config,
    width: u16,
    edit: Option<(&EditTarget, &str)>,
) -> Text<'static> {
    let count = edit
        .filter(|(target, _)| **target == EditTarget::BlankLines)
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(config.prompt.blank_lines)
        .min(config::MAX_PROMPT_BLANK_LINES);
    let prompt = render_preview(config, width, None);
    let mut lines = prompt.lines.clone();
    lines.extend((0..count).map(|_| Line::default()));
    lines.extend(prompt.lines);
    Text::from(lines)
}

fn wrap_preview(text: Text<'static>, width: u16) -> Text<'static> {
    if width == 0 {
        return Text::default();
    }
    let mut lines = Vec::new();
    for source in text.lines {
        let mut line = Line::default();
        let mut used = 0;
        for source_span in &source.spans {
            if used > 0 && used + source_span.width() > usize::from(width) {
                lines.push(line);
                line = Line::default();
                used = 0;
            }
            // 収まるブロックは分割せず、端末幅より長いものだけ文字境界で折り返す。
            if source_span.width() <= usize::from(width) {
                used += source_span.width();
                line.push_span(source_span.clone());
            } else {
                for grapheme in source_span.styled_graphemes(source.style) {
                    let span = Span::styled(grapheme.symbol.to_owned(), grapheme.style);
                    let size = span.width();
                    if used > 0 && used + size > usize::from(width) {
                        lines.push(line);
                        line = Line::default();
                        used = 0;
                    }
                    used += size;
                    line.push_span(span);
                }
            }
        }
        lines.push(line);
    }
    Text::from(lines)
}

fn ui(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let width = area.width.saturating_sub(2);
    let edit = app
        .editing
        .as_ref()
        .map(|target| (target, app.edit_buffer.as_str()));
    let spacing_preview = app.tab == 0 && app.prompt_focus == 8;
    let preview_text = if spacing_preview {
        render_spacing_preview(&app.config, width, edit)
    } else {
        render_preview(&app.config, width, edit)
    };
    let preview_text = wrap_preview(preview_text, width);
    let preview_height = (preview_text
        .lines
        .len()
        .saturating_add(2)
        .min(u16::MAX as usize) as u16)
        .min(area.height.saturating_sub(if app.color_picker.is_some() {
            6
        } else if spacing_preview {
            8
        } else {
            10
        }));
    let preview = Paragraph::new(preview_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(app.lang.text(" Preview ", " プレビュー ")),
    );
    if app.color_picker.is_some() {
        let parts =
            Layout::vertical([Constraint::Length(preview_height), Constraint::Min(0)]).split(area);
        frame.render_widget(preview, parts[0]);
        colors::render_picker(frame, app, parts[1]);
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(preview_height), // プレビュー
        Constraint::Length(3),              // タブ
        Constraint::Fill(1),                // 内容
        Constraint::Length(2),              // 状態とエラー
        Constraint::Length(if area.width < 80 { 3 } else { 1 }), // ヘルプバー
    ])
    .split(area);

    // -- プレビュー --
    frame.render_widget(preview, chunks[0]);

    // -- タブ --
    let titles = app.lang.tabs();
    let tabs = Tabs::new(titles.iter().copied())
        .select(app.tab)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(tabs, chunks[1]);

    // -- コンテンツ --
    let content_area = chunks[2];
    match app.tab {
        0 => render_prompt_tab(frame, app, content_area),
        1 => render_segments_tab(frame, app, content_area),
        2 => render_git_tab(frame, app, content_area),
        3 => render_suggest_tab(frame, app, content_area),
        4 => render_style_tab(frame, app, content_area),
        5 => render_custom_tab(frame, app, content_area),
        6 => render_shell_tab(frame, app, content_area),
        7 => completions::render(frame, app, content_area),
        _ => {}
    }

    // -- ヘルプバー --
    let dirty_indicator = if app.dirty {
        app.lang.text(" [UNSAVED]", " [未保存]")
    } else {
        ""
    };
    let status = app.status_msg.as_deref().unwrap_or("");

    let help_text = if app.quit_state == QuitState::Confirming {
        app.lang.text(
            "Unsaved changes! Press Esc again to quit, or any other key to cancel.",
            "未保存の変更があります。Esc:破棄して終了  その他のキー:戻る",
        )
    } else if app.custom_delete_pending {
        app.lang.text(
            "Delete again: remove definition and placements  Any other key: cancel",
            "再度 Del:定義と配置を削除  その他のキー:戻る",
        )
    } else if app.editing.is_some() {
        app.lang.text(
            "Enter:confirm  Esc:cancel  Arrows/Home/End:cursor  Alt+Enter:newline",
            "Enter:確定  Esc:取消  矢印/Home/End:移動  Alt+Enter:改行",
        )
    } else if app.tab == 1 {
        app.lang.text(
            "Tab:switch  hjkl:nav  L/R:add to side  Del:remove  u/d:order  S:save  Esc:quit",
            "Tab:切替  hjkl:移動  L/R:追加  Del:削除  u/d:順序  S:保存  Esc:終了",
        )
    } else if app.tab == 5 {
        app.lang.text(
            "Tab:switch  jk:field  hl:select  n:new  Del:delete  Enter:edit  S:save  Esc:quit",
            "Tab:切替  jk:項目  hl:選択  n:作成  Del:削除  Enter:編集  S:保存 Esc:終了",
        )
    } else {
        app.lang.text(
            "Tab:switch  jk:nav  hl:change  Enter:edit/toggle  S:save  Esc:quit",
            "Tab:切替  jk:移動  hl:変更  Enter:編集/切替  S:保存  Esc:終了",
        )
    };

    let status_line = Line::from(vec![
        Span::styled(dirty_indicator, Style::default().fg(Color::Yellow).bold()),
        Span::raw("  "),
        Span::styled(status, Style::default().fg(Color::Yellow)),
    ]);
    frame.render_widget(
        Paragraph::new(status_line).wrap(ratatui::widgets::Wrap { trim: false }),
        chunks[3],
    );
    frame.render_widget(
        Paragraph::new(help_text)
            .wrap(ratatui::widgets::Wrap { trim: true })
            .style(Style::default().fg(Color::DarkGray)),
        chunks[4],
    );
    if let Some(target) = &app.editing {
        render_editor(frame, app, target, content_area);
    }
}

fn render_editor(frame: &mut Frame, app: &App, target: &EditTarget, area: ratatui::layout::Rect) {
    let cursor = app.edit_cursor.min(app.edit_buffer.len());
    let before = &app.edit_buffer[..cursor];
    let row = before.chars().filter(|c| *c == '\n').count();
    let column = Line::from(before.rsplit('\n').next().unwrap_or("")).width();
    let text = format!("{}▏{}", before, &app.edit_buffer[cursor..]);
    let scroll_y = row
        .saturating_sub(usize::from(area.height.saturating_sub(3)))
        .min(u16::MAX as usize) as u16;
    let scroll_x = column
        .saturating_sub(usize::from(area.width.saturating_sub(4)))
        .min(u16::MAX as usize) as u16;
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text).scroll((scroll_y, scroll_x)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    " {}: {} ",
                    app.lang.text("Edit", "編集"),
                    target.label(app.lang)
                ))
                .border_style(Style::default().fg(Color::Yellow)),
        ),
        area,
    );
}

fn render_fields(
    frame: &mut Frame,
    items: Vec<ListItem<'static>>,
    title: &str,
    focus: usize,
    area: ratatui::layout::Rect,
) {
    let mut state = ListState::default().with_selected(Some(focus));
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_stateful_widget(list, area, &mut state);
}

fn field_style(focused: bool, editing: bool) -> Style {
    if editing {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if focused {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else {
        Style::default()
    }
}

fn render_prompt_tab(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let items: Vec<ListItem> = vec![
        field_item(
            app.lang.text("Prompt Style", "スタイル"),
            &app.config.prompt.prompt_style,
            app.prompt_focus == 0,
            false,
        ),
        field_item(
            app.lang.text("Font Level", "フォント種別"),
            &app.config.prompt.font_level,
            app.prompt_focus == 1,
            false,
        ),
        field_item(
            app.lang
                .text("Transient Prompt", "過去のプロンプトを簡略化"),
            bool_str(app.config.prompt.transient, app.lang),
            app.prompt_focus == 2,
            false,
        ),
        {
            let is_editing = app.editing.as_ref() == Some(&EditTarget::HomeSymbol);
            let val = if is_editing {
                format!("{}|", app.edit_buffer)
            } else {
                app.config.prompt.dir.home_symbol.clone()
            };
            field_item(
                app.lang.text("Home Symbol", "ホームの記号"),
                &val,
                app.prompt_focus == 3,
                is_editing,
            )
        },
        field_item(
            app.lang.text("Truncation Length", "表示する階層数"),
            &app.config.prompt.dir.truncation_length.to_string(),
            app.prompt_focus == 4,
            false,
        ),
        field_item(
            app.lang.text("Truncation Symbol", "省略記号"),
            &app.config.prompt.dir.truncation_symbol,
            app.prompt_focus == 5,
            false,
        ),
        field_item(
            app.lang.text(
                "IP Interface (auto if empty)",
                "IP インターフェース（空欄で自動）",
            ),
            &app.config.prompt.ip_interface,
            app.prompt_focus == 6,
            false,
        ),
        field_item(
            app.lang.text("Command input", "コマンド入力位置"),
            if app.config.prompt.newline {
                app.lang.text("Next line", "次の行")
            } else {
                app.lang.text("Same line", "同じ行")
            },
            app.prompt_focus == 7,
            false,
        ),
        field_item(
            app.lang.text(
                "Blank lines between prompts (0–10)",
                "プロンプト間の空行（0–10）",
            ),
            &app.config
                .prompt
                .blank_lines
                .min(config::MAX_PROMPT_BLANK_LINES)
                .to_string(),
            app.prompt_focus == 8,
            false,
        ),
    ];
    render_fields(
        frame,
        items,
        app.lang.text(" Prompt Settings ", " プロンプト設定 "),
        app.prompt_focus,
        area,
    );
}

fn render_git_tab(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let items: Vec<ListItem> = vec![
        field_item(
            app.lang.text("Show Status", "変更状態を表示"),
            bool_str(app.config.prompt.git.show_status, app.lang),
            app.git_focus == 0,
            false,
        ),
        field_item(
            app.lang.text("Show Ahead/Behind", "先行・遅延を表示"),
            bool_str(app.config.prompt.git.show_ahead_behind, app.lang),
            app.git_focus == 1,
            false,
        ),
        field_item(
            app.lang.text("Show Stash", "スタッシュを表示"),
            bool_str(app.config.prompt.git.show_stash, app.lang),
            app.git_focus == 2,
            false,
        ),
    ];

    render_fields(
        frame,
        items,
        app.lang.text(" Git Settings ", " Git 設定 "),
        app.git_focus,
        area,
    );
}

fn render_suggest_tab(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let items: Vec<ListItem> = vec![
        field_item(
            app.lang.text("Strategy", "候補の検索方法"),
            &app.config.suggest.strategy,
            app.suggest_focus == 0,
            false,
        ),
        {
            let is_editing = app.editing.as_ref() == Some(&EditTarget::HighlightColor);
            let val = if is_editing {
                format!("{}|", app.edit_buffer)
            } else {
                app.config.suggest.highlight_color.clone()
            };
            color_field_item(
                app.lang,
                app.lang.text("Highlight Color", "候補の色"),
                &val,
                app.suggest_focus == 1,
                &EditTarget::HighlightColor,
            )
        },
        field_item(
            app.lang.text("Max Suggestions", "候補の最大数"),
            &app.config.suggest.max_suggestions.to_string(),
            app.suggest_focus == 2,
            false,
        ),
    ];

    render_fields(
        frame,
        items,
        app.lang.text(" Suggest Settings ", " 入力候補の設定 "),
        app.suggest_focus,
        area,
    );
}

fn palette_label(name: &str, lang: Lang) -> &'static str {
    match name {
        "blue" => lang.text("Blue", "青系"),
        "green" => lang.text("Green", "緑系"),
        "blue_green" => lang.text("Blue / Green", "青緑系"),
        "purple" => lang.text("Purple", "紫系"),
        "cyan" => lang.text("Cyan", "シアン系"),
        "pink" => lang.text("Pink", "ピンク系"),
        "red" => lang.text("Red", "赤系"),
        "orange" => lang.text("Orange", "オレンジ系"),
        "ocean" => lang.text("Ocean", "オーシャン"),
        "forest" => lang.text("Forest", "フォレスト"),
        "sunset" => lang.text("Sunset", "サンセット"),
        "pastel" => lang.text("Pastel", "パステル"),
        "grayscale" => lang.text("Grayscale", "モノクロ"),
        _ => lang.text("Default", "標準"),
    }
}

fn render_style_tab(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let targets = [
        (
            app.lang.text("Primary Color", "基本色"),
            &app.config.style.primary_color,
            EditTarget::PrimaryColor,
        ),
        (
            app.lang.text("Success Color", "成功の色"),
            &app.config.style.success_color,
            EditTarget::SuccessColor,
        ),
        (
            app.lang.text("Error Color", "エラーの色"),
            &app.config.style.error_color,
            EditTarget::ErrorColor,
        ),
        (
            app.lang.text("Muted Color", "補助色"),
            &app.config.style.muted_color,
            EditTarget::MutedColor,
        ),
    ];

    let mut items: Vec<ListItem> = targets
        .iter()
        .enumerate()
        .map(|(i, (label, val, target))| {
            let is_editing = app.editing.as_ref() == Some(target);
            let display = if is_editing {
                format!("{}|", app.edit_buffer)
            } else {
                val.to_string()
            };
            color_field_item(app.lang, label, &display, app.style_focus == i, target)
        })
        .collect();

    items.push(field_item(
        app.lang.text("Rainbow palette", "Rainbow の配色"),
        palette_label(&app.config.style.rainbow_palette, app.lang),
        app.style_focus == 4,
        false,
    ));

    items.extend(block_colors::fields(app));

    render_fields(
        frame,
        items,
        app.lang.text(" Style Settings ", " 配色設定 "),
        app.style_focus,
        area,
    );
}

fn render_custom_tab(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let Some(custom) = app.config.prompt.custom.get(app.custom_index) else {
        frame.render_widget(Paragraph::new(app.lang.text("No custom segments. Press n to create one.\nThen add it to Left/Right in the Segments tab.", "カスタムセグメントはありません。n キーで作成できます。\n作成後、「配置」タブで左側・右側に追加してください。")).block(Block::default().borders(Borders::ALL).title(app.lang.text(" Custom Segments ", " カスタムセグメント "))), area);
        return;
    };
    let mut items = vec![field_item(
        app.lang.text("Selected (Left/Right)", "選択（←/→）"),
        &format!(
            "{}/{}: {}",
            app.custom_index + 1,
            app.config.prompt.custom.len(),
            custom.name
        ),
        app.custom_focus == 0,
        false,
    )];
    let mut custom = custom.clone();
    for (i, field) in CustomField::ALL.iter().enumerate() {
        let target = EditTarget::Custom(app.custom_index, *field);
        let value = field.value(&mut custom).replace('\n', " ↵ ");
        items.push(if target.is_color() {
            color_field_item(
                app.lang,
                field.label(app.lang),
                &value,
                app.custom_focus == i + 1,
                &target,
            )
        } else {
            field_item(
                field.label(app.lang),
                &value,
                app.custom_focus == i + 1,
                false,
            )
        });
    }
    render_fields(
        frame,
        items,
        app.lang.text(
            " Custom: edit here, place in Segments ",
            " カスタム: 編集後に「配置」で追加 ",
        ),
        app.custom_focus,
        area,
    );
}

fn render_shell_tab(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let items = vec![
        field_item(
            app.lang.text(
                "Completion Dirs (: separated)",
                "補完ディレクトリ（: 区切り）",
            ),
            &app.config.shell.completion_dirs,
            app.shell_focus == 0,
            false,
        ),
        field_item(
            app.lang.text("Terminal Integration", "端末との連携"),
            &app.config.shell.term_shell_integration,
            app.shell_focus == 1,
            false,
        ),
    ];
    render_fields(
        frame,
        items,
        app.lang.text(
            " Shell (auto / 1:on / 0:off) ",
            " シェル（auto:自動 / 1:有効 / 0:無効） ",
        ),
        app.shell_focus,
        area,
    );
}

fn render_segments_tab(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let cols = Layout::horizontal([
        Constraint::Percentage(33),
        Constraint::Percentage(33),
        Constraint::Percentage(34),
    ])
    .split(area);

    // 左セグメント
    let left_items: Vec<ListItem> = app
        .config
        .prompt
        .left_segments
        .iter()
        .map(|s| ListItem::new(s.as_str().to_owned()))
        .collect();
    let left_block = Block::default()
        .borders(Borders::ALL)
        .title(app.lang.text(" Left Segments ", " 左側 "))
        .border_style(if app.seg_column == SegColumn::Left {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    let left_list = List::new(left_items)
        .block(left_block)
        .highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black));
    frame.render_stateful_widget(left_list, cols[0], &mut app.seg_left_state);

    // 右セグメント
    let right_items: Vec<ListItem> = app
        .config
        .prompt
        .right_segments
        .iter()
        .map(|s| ListItem::new(s.as_str().to_owned()))
        .collect();
    let right_block = Block::default()
        .borders(Borders::ALL)
        .title(app.lang.text(" Right Segments ", " 右側 "))
        .border_style(if app.seg_column == SegColumn::Right {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    let right_list = List::new(right_items)
        .block(right_block)
        .highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black));
    frame.render_stateful_widget(right_list, cols[1], &mut app.seg_right_state);

    // 利用可能なセグメント
    let avail = available_segments(&app.config);
    let avail_items: Vec<ListItem> = avail
        .iter()
        .map(|s| ListItem::new(s.as_str().to_owned()))
        .collect();
    let avail_block = Block::default()
        .borders(Borders::ALL)
        .title(app.lang.text(" Available ", " 追加候補 "))
        .border_style(if app.seg_column == SegColumn::Available {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    let avail_list = List::new(avail_items)
        .block(avail_block)
        .highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black));
    frame.render_stateful_widget(avail_list, cols[2], &mut app.seg_avail_state);
}

fn field_item<'a>(label: &str, value: &str, focused: bool, editing: bool) -> ListItem<'a> {
    let style = field_style(focused, editing);
    let line = Line::from(vec![
        Span::styled(
            format!("  {label}: "),
            if focused && !editing {
                style
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            },
        ),
        Span::styled(format!("{value}  "), style),
    ]);
    ListItem::new(line)
}

fn bool_str(v: bool, lang: Lang) -> &'static str {
    if v {
        lang.text("ON", "有効")
    } else {
        lang.text("OFF", "無効")
    }
}

// ---------------------------------------------------------------------------
// イベント処理
// ---------------------------------------------------------------------------

fn handle_event(app: &mut App, key: KeyEvent) -> bool {
    // キー押下時にステータスメッセージをクリア
    app.status_msg = None;

    if app.custom_delete_pending {
        app.custom_delete_pending = false;
        if key.code == KeyCode::Delete {
            app.custom_remove();
        }
        return false;
    }

    // 終了確認
    if app.quit_state == QuitState::Confirming {
        if key.code == KeyCode::Esc {
            return true; // 確定終了
        }
        app.quit_state = QuitState::Normal;
        return false;
    }

    // テキスト編集モード
    if app.color_picker.is_some() {
        colors::handle_picker(app, key);
        return false;
    }
    if app.editing.is_some() {
        match key.code {
            KeyCode::Enter
                if key.modifiers.contains(KeyModifiers::ALT)
                    && matches!(
                        app.editing,
                        Some(EditTarget::Custom(_, CustomField::Command))
                    ) =>
            {
                app.edit_buffer.insert(app.edit_cursor, '\n');
                app.edit_cursor += 1;
            }
            KeyCode::Enter => app.confirm_edit(),
            KeyCode::Esc => app.cancel_edit(),
            KeyCode::Backspace => {
                if app.edit_cursor > 0 {
                    let prev = app.edit_buffer[..app.edit_cursor]
                        .char_indices()
                        .next_back()
                        .unwrap()
                        .0;
                    app.edit_buffer.drain(prev..app.edit_cursor);
                    app.edit_cursor = prev;
                }
            }
            KeyCode::Left => {
                app.edit_cursor = app.edit_buffer[..app.edit_cursor]
                    .char_indices()
                    .next_back()
                    .map_or(0, |(i, _)| i)
            }
            KeyCode::Right => {
                if let Some(c) = app.edit_buffer[app.edit_cursor..].chars().next() {
                    app.edit_cursor += c.len_utf8();
                }
            }
            KeyCode::Home => app.edit_cursor = 0,
            KeyCode::End => app.edit_cursor = app.edit_buffer.len(),
            KeyCode::Delete if app.edit_cursor < app.edit_buffer.len() => {
                app.edit_buffer.remove(app.edit_cursor);
            }
            // Ctrl/Alt 付きキー (Ctrl+C 等) を生文字として挿入しない
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                app.edit_buffer.insert(app.edit_cursor, c);
                app.edit_cursor += c.len_utf8();
            }
            _ => {}
        }
        return false;
    }

    match key.code {
        // 保存
        KeyCode::Char('s') | KeyCode::Char('S') => {
            app.save();
        }
        // タブ切り替え
        KeyCode::Tab | KeyCode::Char(']') => {
            app.tab = (app.tab + 1) % TAB_TITLES.len();
        }
        KeyCode::BackTab | KeyCode::Char('[') => {
            app.tab = (app.tab + TAB_TITLES.len() - 1) % TAB_TITLES.len();
        }
        // ナビゲーション
        KeyCode::Up | KeyCode::Char('k') => app.focus_up(),
        KeyCode::Down | KeyCode::Char('j') => app.focus_down(),
        KeyCode::Left | KeyCode::Char('h') => handle_left(app),
        KeyCode::Right | KeyCode::Char('l') => handle_right(app),
        // トグル / 決定
        KeyCode::Char(' ') | KeyCode::Enter => handle_activate(app),
        // セグメント固有の操作 (Segments タブ専用)
        KeyCode::Char('L' | 'R') if app.tab == 1 => {
            let side = if key.code == KeyCode::Char('L') {
                SegColumn::Left
            } else {
                SegColumn::Right
            };
            app.seg_add_to(side);
        }
        KeyCode::Delete | KeyCode::Backspace if app.tab == 1 => {
            app.seg_remove();
        }
        KeyCode::Char('u') if app.tab == 1 => {
            app.seg_reorder(true);
        }
        KeyCode::Char('d') if app.tab == 1 => {
            app.seg_reorder(false);
        }
        KeyCode::Char('n') if app.tab == 7 => completions::add(app),
        KeyCode::Delete if app.tab == 7 => completions::remove(app),
        KeyCode::Char('r') if app.tab == 7 => completions::refresh(app, false),
        KeyCode::Char('R') if app.tab == 7 => completions::refresh(app, true),
        KeyCode::Char('n') if app.tab == 5 => app.custom_add(),
        KeyCode::Delete if app.tab == 5 && !app.config.prompt.custom.is_empty() => {
            app.custom_delete_pending = true;
            app.status_msg = Some(format!(
                "{}: {}",
                app.config.prompt.custom[app.custom_index].name,
                app.lang.text(
                    "Delete definition and placements?",
                    "定義と配置を削除しますか？"
                )
            ));
        }
        KeyCode::Esc => {
            // 非編集モードでは Esc で終了
            if app.dirty {
                app.quit_state = QuitState::Confirming;
            } else {
                return true;
            }
        }
        _ => {}
    }
    false
}

fn handle_left(app: &mut App) {
    if app.tab == 7 {
        completions::select(app, false);
        return;
    }
    match app.tab {
        0 => match app.prompt_focus {
            0 => {
                app.config.prompt.prompt_style =
                    cycle_prev(&app.config.prompt.prompt_style, PROMPT_STYLES);
                app.dirty = true;
            }
            1 => {
                app.config.prompt.font_level =
                    cycle_prev(&app.config.prompt.font_level, FONT_LEVELS);
                app.dirty = true;
            }
            4 if app.config.prompt.dir.truncation_length > 0 => {
                app.config.prompt.dir.truncation_length -= 1;
                app.dirty = true;
            }
            7 => {
                app.config.prompt.newline = !app.config.prompt.newline;
                app.dirty = true;
            }
            8 => {
                app.config.prompt.blank_lines = app
                    .config
                    .prompt
                    .blank_lines
                    .min(config::MAX_PROMPT_BLANK_LINES)
                    .saturating_sub(1);
                app.dirty = true;
            }
            _ => {}
        },
        1 => app.seg_column_left(),
        3 => {
            if app.suggest_focus == 0 {
                app.config.suggest.strategy =
                    cycle_prev(&app.config.suggest.strategy, SUGGEST_STRATEGIES);
                app.dirty = true;
            } else if app.suggest_focus == 2 && app.config.suggest.max_suggestions > 0 {
                app.config.suggest.max_suggestions -= 1;
                app.dirty = true;
            }
        }
        4 if app.style_focus == 4 => {
            app.config.style.rainbow_palette =
                cycle_prev(&app.config.style.rainbow_palette, RAINBOW_PALETTES);
            app.dirty = true;
        }
        4 if app.style_focus == 5 => app.rainbow_select(false),
        5 => app.custom_select(false),
        6 if app.shell_focus == 1 => {
            app.config.shell.term_shell_integration =
                cycle_prev(&app.config.shell.term_shell_integration, SHELL_INTEGRATIONS);
            app.dirty = true;
        }
        _ => {}
    }
}

fn handle_right(app: &mut App) {
    if app.tab == 7 {
        completions::select(app, true);
        return;
    }
    match app.tab {
        0 => match app.prompt_focus {
            0 => {
                app.config.prompt.prompt_style =
                    cycle_next(&app.config.prompt.prompt_style, PROMPT_STYLES);
                app.dirty = true;
            }
            1 => {
                app.config.prompt.font_level =
                    cycle_next(&app.config.prompt.font_level, FONT_LEVELS);
                app.dirty = true;
            }
            4 => {
                app.config.prompt.dir.truncation_length =
                    app.config.prompt.dir.truncation_length.saturating_add(1);
                app.dirty = true;
            }
            7 => {
                app.config.prompt.newline = !app.config.prompt.newline;
                app.dirty = true;
            }
            8 => {
                app.config.prompt.blank_lines = app
                    .config
                    .prompt
                    .blank_lines
                    .saturating_add(1)
                    .min(config::MAX_PROMPT_BLANK_LINES);
                app.dirty = true;
            }
            _ => {}
        },
        1 => app.seg_column_right(),
        3 => {
            if app.suggest_focus == 0 {
                app.config.suggest.strategy =
                    cycle_next(&app.config.suggest.strategy, SUGGEST_STRATEGIES);
                app.dirty = true;
            } else if app.suggest_focus == 2 {
                app.config.suggest.max_suggestions =
                    app.config.suggest.max_suggestions.saturating_add(1);
                app.dirty = true;
            }
        }
        4 if app.style_focus == 4 => {
            app.config.style.rainbow_palette =
                cycle_next(&app.config.style.rainbow_palette, RAINBOW_PALETTES);
            app.dirty = true;
        }
        4 if app.style_focus == 5 => app.rainbow_select(true),
        5 => app.custom_select(true),
        6 if app.shell_focus == 1 => {
            app.config.shell.term_shell_integration =
                cycle_next(&app.config.shell.term_shell_integration, SHELL_INTEGRATIONS);
            app.dirty = true;
        }
        _ => {}
    }
}

fn handle_activate(app: &mut App) {
    if app.tab == 7 {
        completions::activate(app);
        return;
    }
    match app.tab {
        0 => match app.prompt_focus {
            0 => {
                app.config.prompt.prompt_style =
                    cycle_next(&app.config.prompt.prompt_style, PROMPT_STYLES);
                app.dirty = true;
            }
            1 => {
                app.config.prompt.font_level =
                    cycle_next(&app.config.prompt.font_level, FONT_LEVELS);
                app.dirty = true;
            }
            2 => {
                app.config.prompt.transient = !app.config.prompt.transient;
                app.dirty = true;
            }
            3 => app.start_edit(EditTarget::HomeSymbol),
            4 => app.start_edit(EditTarget::TruncationLength),
            5 => app.start_edit(EditTarget::TruncationSymbol),
            6 => app.start_edit(EditTarget::IpInterface),
            7 => {
                app.config.prompt.newline = !app.config.prompt.newline;
                app.dirty = true;
            }
            8 => app.start_edit(EditTarget::BlankLines),
            _ => {}
        },
        1 => app.seg_add(),
        2 => match app.git_focus {
            0 => {
                app.config.prompt.git.show_status = !app.config.prompt.git.show_status;
                app.dirty = true;
            }
            1 => {
                app.config.prompt.git.show_ahead_behind = !app.config.prompt.git.show_ahead_behind;
                app.dirty = true;
            }
            2 => {
                app.config.prompt.git.show_stash = !app.config.prompt.git.show_stash;
                app.dirty = true;
            }
            _ => {}
        },
        3 => match app.suggest_focus {
            0 => {
                app.config.suggest.strategy =
                    cycle_next(&app.config.suggest.strategy, SUGGEST_STRATEGIES);
                app.dirty = true;
            }
            1 => app.activate_edit(EditTarget::HighlightColor),
            2 => app.start_edit(EditTarget::MaxSuggestions),
            _ => {}
        },
        4 if app.style_focus == 4 => handle_right(app),
        4 if app.style_focus == 5 => app.rainbow_select(true),
        4 if app.style_focus == 8 => app.rainbow_reset(),
        4 => {
            let target = match app.style_focus {
                0 => Some(EditTarget::PrimaryColor),
                1 => Some(EditTarget::SuccessColor),
                2 => Some(EditTarget::ErrorColor),
                3 => Some(EditTarget::MutedColor),
                6 => app
                    .rainbow_selected()
                    .map(|name| EditTarget::RainbowFg(name.into())),
                7 => app
                    .rainbow_selected()
                    .map(|name| EditTarget::RainbowBg(name.into())),
                _ => None,
            };
            if let Some(t) = target {
                app.activate_edit(t);
            }
        }
        5 if app.custom_focus > 0 && !app.config.prompt.custom.is_empty() => {
            app.activate_edit(EditTarget::Custom(
                app.custom_index,
                CustomField::ALL[app.custom_focus - 1],
            ));
        }
        6 => {
            if app.shell_focus == 0 {
                app.start_edit(EditTarget::CompletionDirs);
            } else {
                handle_right(app);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Suggest タブの数値増減（Left/Right キー経由）
// ---------------------------------------------------------------------------

// （handle_left / handle_right 内でインライン処理）

// ---------------------------------------------------------------------------
// メインループ
// ---------------------------------------------------------------------------

fn run_app(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    loop {
        completions::poll_refresh(app);
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if handle_event(app, key) {
                break;
            }
        }
    }
    Ok(())
}

pub fn run_tui() -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new(config::load_config(), Lang::from_env());
    let result = run_app(&mut terminal, &mut app);
    ratatui::restore();
    result
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::widgets::ListState;

    // --- cycle_next ---

    #[test]
    fn cycle_next_は次の要素を返す() {
        let opts = &["a", "b", "c"];
        assert_eq!(cycle_next("a", opts), "b");
        assert_eq!(cycle_next("b", opts), "c");
    }

    #[test]
    fn cycle_next_は末尾から先頭へ巡回する() {
        let opts = &["a", "b", "c"];
        assert_eq!(cycle_next("c", opts), "a");
    }

    #[test]
    fn cycle_next_は不明な値を先頭と見なす() {
        let opts = &["a", "b", "c"];
        assert_eq!(cycle_next("unknown", opts), "b");
    }

    // --- cycle_prev ---

    #[test]
    fn cycle_prev_は前の要素を返す() {
        let opts = &["a", "b", "c"];
        assert_eq!(cycle_prev("c", opts), "b");
        assert_eq!(cycle_prev("b", opts), "a");
    }

    #[test]
    fn cycle_prev_は先頭から末尾へ巡回する() {
        let opts = &["a", "b", "c"];
        assert_eq!(cycle_prev("a", opts), "c");
    }

    #[test]
    fn cycle_prev_は不明な値を先頭と見なす() {
        let opts = &["a", "b", "c"];
        assert_eq!(cycle_prev("unknown", opts), "c");
    }

    // --- reorder_vec ---

    #[test]
    fn reorder_vec_は要素を上に移動する() {
        let mut v = vec!["x".into(), "y".into(), "z".into()];
        let mut state = ListState::default();
        state.select(Some(1));
        assert!(reorder_vec(&mut v, &mut state, true));
        assert_eq!(v, vec!["y", "x", "z"]);
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn reorder_vec_は要素を下に移動する() {
        let mut v = vec!["x".into(), "y".into(), "z".into()];
        let mut state = ListState::default();
        state.select(Some(0));
        assert!(reorder_vec(&mut v, &mut state, false));
        assert_eq!(v, vec!["y", "x", "z"]);
        assert_eq!(state.selected(), Some(1));
    }

    #[test]
    fn reorder_vec_は先頭で上移動しても変化しない() {
        let mut v = vec!["x".into(), "y".into()];
        let mut state = ListState::default();
        state.select(Some(0));
        assert!(!reorder_vec(&mut v, &mut state, true));
        assert_eq!(v, vec!["x", "y"]);
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn reorder_vec_は末尾で下移動しても変化しない() {
        let mut v = vec!["x".into(), "y".into()];
        let mut state = ListState::default();
        state.select(Some(1));
        assert!(!reorder_vec(&mut v, &mut state, false));
        assert_eq!(v, vec!["x", "y"]);
        assert_eq!(state.selected(), Some(1));
    }

    #[test]
    fn reorder_vec_は未選択なら何もしない() {
        let mut v = vec!["x".into(), "y".into()];
        let mut state = ListState::default();
        assert!(!reorder_vec(&mut v, &mut state, true));
        assert_eq!(v, vec!["x", "y"]);
        assert_eq!(state.selected(), None);
    }

    // --- fix_list_state ---

    #[test]
    fn fix_list_state_は空リストで選択を解除する() {
        let mut state = ListState::default();
        state.select(Some(3));
        fix_list_state(&mut state, 0);
        assert_eq!(state.selected(), None);
    }

    #[test]
    fn fix_list_state_は範囲外を末尾に補正する() {
        let mut state = ListState::default();
        state.select(Some(5));
        fix_list_state(&mut state, 3);
        assert_eq!(state.selected(), Some(2));
    }

    #[test]
    fn fix_list_state_は範囲内ならそのまま() {
        let mut state = ListState::default();
        state.select(Some(1));
        fix_list_state(&mut state, 3);
        assert_eq!(state.selected(), Some(1));
    }

    #[test]
    fn fix_list_state_は未選択のままにする() {
        let mut state = ListState::default();
        fix_list_state(&mut state, 5);
        assert_eq!(state.selected(), None);
    }

    // --- available_segments ---

    #[test]
    fn available_segments_はデフォルトで未使用セグメントを返す() {
        let config = Config::default();
        let avail = available_segments(&config);
        // デフォルトの左右セグメントに含まれないものが返る
        for seg in &config.prompt.left_segments {
            assert!(!avail.contains(seg), "{seg} は除外されるべき");
        }
        for seg in &config.prompt.right_segments {
            assert!(!avail.contains(seg), "{seg} は除外されるべき");
        }
    }

    #[test]
    fn available_segments_は全セグメント使用時に空を返す() {
        let mut config = Config::default();
        config.prompt.left_segments = ALL_SEGMENTS.iter().map(|s| s.to_string()).collect();
        config.prompt.right_segments.clear();
        let avail = available_segments(&config);
        assert!(avail.is_empty());
    }

    #[test]
    fn available_segments_は空設定で全セグメントを返す() {
        let mut config = Config::default();
        config.prompt.left_segments.clear();
        config.prompt.right_segments.clear();
        let avail = available_segments(&config);
        assert_eq!(avail.len(), ALL_SEGMENTS.len());
    }

    // --- bool_str ---

    #[test]
    fn bool_str_はtrueでonを返す() {
        assert_eq!(bool_str(true, Lang::En), "ON");
    }

    #[test]
    fn bool_str_はfalseでoffを返す() {
        assert_eq!(bool_str(false, Lang::En), "OFF");
    }

    // --- to_ratatui_color ---

    #[test]
    fn to_ratatui_color_基本色の変換() {
        assert_eq!(to_ratatui_color(AppColor::Red), Color::Red);
        assert_eq!(to_ratatui_color(AppColor::Green), Color::Green);
        assert_eq!(to_ratatui_color(AppColor::Blue), Color::Blue);
        assert_eq!(to_ratatui_color(AppColor::Black), Color::Black);
        assert_eq!(to_ratatui_color(AppColor::White), Color::White);
    }

    #[test]
    fn to_ratatui_color_明るい色の変換() {
        assert_eq!(to_ratatui_color(AppColor::BrightBlack), Color::DarkGray);
        assert_eq!(to_ratatui_color(AppColor::BrightRed), Color::LightRed);
        assert_eq!(to_ratatui_color(AppColor::BrightGreen), Color::LightGreen);
        assert_eq!(to_ratatui_color(AppColor::BrightCyan), Color::LightCyan);
        assert_eq!(to_ratatui_color(AppColor::BrightWhite), Color::White);
    }

    #[test]
    fn to_ratatui_color_ansi256の変換() {
        assert_eq!(to_ratatui_color(AppColor::Ansi256(42)), Color::Indexed(42));
        assert_eq!(to_ratatui_color(AppColor::Ansi256(0)), Color::Indexed(0));
        assert_eq!(
            to_ratatui_color(AppColor::Ansi256(255)),
            Color::Indexed(255)
        );
    }

    #[test]
    fn to_ratatui_color_rgbの変換() {
        assert_eq!(
            to_ratatui_color(AppColor::Rgb(10, 20, 30)),
            Color::Rgb(10, 20, 30)
        );
    }

    // --- list_prev / list_next ---

    #[test]
    fn list_prev_は先頭で変化しない() {
        let mut state = ListState::default();
        state.select(Some(0));
        list_prev(&mut state);
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn list_prev_は1つ前に移動する() {
        let mut state = ListState::default();
        state.select(Some(2));
        list_prev(&mut state);
        assert_eq!(state.selected(), Some(1));
    }

    #[test]
    fn list_prev_は未選択なら何もしない() {
        let mut state = ListState::default();
        list_prev(&mut state);
        assert_eq!(state.selected(), None);
    }

    #[test]
    fn list_next_は1つ後に移動する() {
        let mut state = ListState::default();
        state.select(Some(0));
        list_next(&mut state, 3);
        assert_eq!(state.selected(), Some(1));
    }

    #[test]
    fn list_next_は末尾で変化しない() {
        let mut state = ListState::default();
        state.select(Some(2));
        list_next(&mut state, 3);
        assert_eq!(state.selected(), Some(2));
    }

    #[test]
    fn list_next_は未選択なら先頭を選択する() {
        let mut state = ListState::default();
        list_next(&mut state, 3);
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn list_next_は空リストで何もしない() {
        let mut state = ListState::default();
        list_next(&mut state, 0);
        assert_eq!(state.selected(), None);
    }

    // --- App の状態管理 ---

    #[test]
    fn app_new_はデフォルト設定で正しく初期化される() {
        let config = Config::default();
        let app = App::new(config, Lang::En);
        assert_eq!(app.tab, 0);
        assert!(!app.dirty);
        assert_eq!(app.prompt_focus, 0);
        assert_eq!(app.seg_column, SegColumn::Left);
        assert!(app.editing.is_none());
        assert_eq!(app.quit_state, QuitState::Normal);
    }

    #[test]
    fn app_max_focus_は各タブで正しい値を返す() {
        let app = App::new(Config::default(), Lang::En);
        assert_eq!(app.max_focus(), 8);
    }

    #[test]
    fn app_focus_up_down_は範囲内で移動する() {
        let mut app = App::new(Config::default(), Lang::En);
        assert_eq!(app.current_focus(), 0);
        app.focus_down();
        assert_eq!(app.current_focus(), 1);
        app.focus_down();
        assert_eq!(app.current_focus(), 2);
        app.focus_up();
        assert_eq!(app.current_focus(), 1);
    }

    #[test]
    fn app_focus_up_は0以下にならない() {
        let mut app = App::new(Config::default(), Lang::En);
        app.focus_up();
        assert_eq!(app.current_focus(), 0);
    }

    #[test]
    fn app_focus_down_はmax_focusを超えない() {
        let mut app = App::new(Config::default(), Lang::En);
        for _ in 0..20 {
            app.focus_down();
        }
        assert_eq!(app.current_focus(), app.max_focus());
    }

    #[test]
    fn app_start_edit_とconfirm_edit() {
        let mut app = App::new(Config::default(), Lang::En);
        app.start_edit(EditTarget::HomeSymbol);
        assert!(app.editing.is_some());
        assert_eq!(app.edit_buffer, "~");
        app.edit_buffer = "HOME".into();
        app.confirm_edit();
        assert!(app.editing.is_none());
        assert_eq!(app.config.prompt.dir.home_symbol, "HOME");
        assert!(app.dirty);
    }

    #[test]
    fn app_cancel_edit_は変更を破棄する() {
        let mut app = App::new(Config::default(), Lang::En);
        let original = app.config.prompt.dir.home_symbol.clone();
        app.start_edit(EditTarget::HomeSymbol);
        app.edit_buffer = "CHANGED".into();
        app.cancel_edit();
        assert!(app.editing.is_none());
        assert_eq!(app.config.prompt.dir.home_symbol, original);
    }

    // --- handle_event ---

    #[test]
    fn handle_event_escキーで未変更時に終了() {
        let mut app = App::new(Config::default(), Lang::En);
        let key = KeyEvent::from(KeyCode::Esc);
        assert!(handle_event(&mut app, key));
    }

    #[test]
    fn handle_event_escキーで変更ありの場合は確認状態に移行() {
        let mut app = App::new(Config::default(), Lang::En);
        app.dirty = true;
        let key = KeyEvent::from(KeyCode::Esc);
        assert!(!handle_event(&mut app, key));
        assert_eq!(app.quit_state, QuitState::Confirming);
    }

    #[test]
    fn handle_event_確認状態でescキーは終了() {
        let mut app = App::new(Config::default(), Lang::En);
        app.quit_state = QuitState::Confirming;
        let key = KeyEvent::from(KeyCode::Esc);
        assert!(handle_event(&mut app, key));
    }

    #[test]
    fn handle_event_確認状態で他のキーはキャンセル() {
        let mut app = App::new(Config::default(), Lang::En);
        app.quit_state = QuitState::Confirming;
        let key = KeyEvent::from(KeyCode::Char('a'));
        assert!(!handle_event(&mut app, key));
        assert_eq!(app.quit_state, QuitState::Normal);
    }

    #[test]
    fn handle_event_tabキーでタブ切り替え() {
        let mut app = App::new(Config::default(), Lang::En);
        assert_eq!(app.tab, 0);
        let key = KeyEvent::from(KeyCode::Tab);
        handle_event(&mut app, key);
        assert_eq!(app.tab, 1);
    }

    #[test]
    fn handle_event_tabキーは末尾から先頭へ巡回() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = TAB_TITLES.len() - 1;
        let key = KeyEvent::from(KeyCode::Tab);
        handle_event(&mut app, key);
        assert_eq!(app.tab, 0);
    }

    #[test]
    fn handle_event_jkキーでフォーカス移動() {
        let mut app = App::new(Config::default(), Lang::En);
        let down = KeyEvent::from(KeyCode::Char('j'));
        handle_event(&mut app, down);
        assert_eq!(app.current_focus(), 1);
        let up = KeyEvent::from(KeyCode::Char('k'));
        handle_event(&mut app, up);
        assert_eq!(app.current_focus(), 0);
    }

    #[test]
    fn handle_event_編集モードでバックスペースは文字削除() {
        let mut app = App::new(Config::default(), Lang::En);
        app.start_edit(EditTarget::HomeSymbol);
        app.edit_buffer = "abc".into();
        app.edit_cursor = app.edit_buffer.len();
        let key = KeyEvent::from(KeyCode::Backspace);
        handle_event(&mut app, key);
        assert_eq!(app.edit_buffer, "ab");
    }

    #[test]
    fn handle_event_編集モードで文字入力() {
        let mut app = App::new(Config::default(), Lang::En);
        app.start_edit(EditTarget::HomeSymbol);
        app.edit_buffer.clear();
        app.edit_cursor = 0;
        let key = KeyEvent::from(KeyCode::Char('x'));
        handle_event(&mut app, key);
        assert_eq!(app.edit_buffer, "x");
    }

    #[test]
    fn handle_event_編集モードでescはキャンセル() {
        let mut app = App::new(Config::default(), Lang::En);
        app.start_edit(EditTarget::HomeSymbol);
        let key = KeyEvent::from(KeyCode::Esc);
        handle_event(&mut app, key);
        assert!(app.editing.is_none());
    }

    #[test]
    fn handle_event_編集モードでenterは確定() {
        let mut app = App::new(Config::default(), Lang::En);
        app.start_edit(EditTarget::HomeSymbol);
        app.edit_buffer = "NEW".into();
        let key = KeyEvent::from(KeyCode::Enter);
        handle_event(&mut app, key);
        assert!(app.editing.is_none());
        assert_eq!(app.config.prompt.dir.home_symbol, "NEW");
    }

    // --- seg_column ナビゲーション ---

    #[test]
    fn seg_column_leftは左端で変化しない() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 1;
        app.seg_column = SegColumn::Left;
        app.seg_column_left();
        assert_eq!(app.seg_column, SegColumn::Left);
    }

    #[test]
    fn seg_column_rightは右端で変化しない() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 1;
        app.seg_column = SegColumn::Available;
        app.seg_column_right();
        assert_eq!(app.seg_column, SegColumn::Available);
    }

    #[test]
    fn seg_column_の左右移動() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 1;
        app.seg_column = SegColumn::Left;
        app.seg_column_right();
        assert_eq!(app.seg_column, SegColumn::Right);
        app.seg_column_right();
        assert_eq!(app.seg_column, SegColumn::Available);
        app.seg_column_left();
        assert_eq!(app.seg_column, SegColumn::Right);
        app.seg_column_left();
        assert_eq!(app.seg_column, SegColumn::Left);
    }

    // --- seg_add / seg_remove ---

    #[test]
    fn seg_add_はavailableカラムからのみ追加できる() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 1;
        app.seg_column = SegColumn::Left;
        let before = app.config.prompt.left_segments.len();
        app.seg_add();
        assert_eq!(app.config.prompt.left_segments.len(), before);
    }

    #[test]
    fn seg_add_はavailableから左に追加する() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 1;
        app.seg_column = SegColumn::Available;
        app.seg_last_side = SegColumn::Left;
        let before = app.config.prompt.left_segments.len();
        app.seg_add();
        assert_eq!(app.config.prompt.left_segments.len(), before + 1);
        assert!(app.dirty);
    }

    #[test]
    fn seg_remove_は左カラムから削除できる() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 1;
        app.seg_column = SegColumn::Left;
        let before = app.config.prompt.left_segments.len();
        assert!(before > 0);
        app.seg_remove();
        assert_eq!(app.config.prompt.left_segments.len(), before - 1);
        assert!(app.dirty);
    }

    #[test]
    fn seg_remove_はavailableカラムでは何もしない() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 1;
        app.seg_column = SegColumn::Available;
        let left_before = app.config.prompt.left_segments.len();
        let right_before = app.config.prompt.right_segments.len();
        app.seg_remove();
        assert_eq!(app.config.prompt.left_segments.len(), left_before);
        assert_eq!(app.config.prompt.right_segments.len(), right_before);
    }

    #[test]
    fn seg_reorder_は実際に移動したときだけdirtyにする() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 1;
        app.seg_column = SegColumn::Right;
        app.seg_right_state.select(Some(1));

        app.seg_reorder(true);

        assert!(app.dirty);
        assert_eq!(
            app.config.prompt.right_segments,
            vec!["status", "duration", "time"]
        );
    }

    #[test]
    fn seg_reorder_は境界で変化しない場合dirtyにしない() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 1;
        app.seg_column = SegColumn::Left;
        app.seg_left_state.select(Some(0));

        app.seg_reorder(true);

        assert!(!app.dirty);
        assert_eq!(app.config.prompt.left_segments, vec!["dir", "git"]);
    }

    // ── 編集モードの修飾キーガード ─────────────────────────────

    #[test]
    fn handle_event_編集モードでctrl付き文字は挿入しない() {
        let mut app = App::new(Config::default(), Lang::En);
        app.editing = Some(EditTarget::HomeSymbol);
        app.edit_buffer = "~".to_string();
        // Ctrl+C は生文字 `c` として挿入されないこと
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(!handle_event(&mut app, key));
        assert_eq!(app.edit_buffer, "~");
        // Alt 付きも同様
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
        assert!(!handle_event(&mut app, key));
        assert_eq!(app.edit_buffer, "~");
    }

    #[test]
    fn handle_event_編集モードで通常文字とshift付き文字は挿入する() {
        let mut app = App::new(Config::default(), Lang::En);
        app.editing = Some(EditTarget::HomeSymbol);
        app.edit_buffer = String::new();
        app.edit_cursor = 0;
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        handle_event(&mut app, key);
        // 大文字は SHIFT 修飾付きで届くため許可されること
        let key = KeyEvent::new(KeyCode::Char('B'), KeyModifiers::SHIFT);
        handle_event(&mut app, key);
        assert_eq!(app.edit_buffer, "aB");
    }

    // ── セグメントリストの空化境界 ─────────────────────────────

    #[test]
    fn seg_remove_はリストを空にし切っても安全() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 1;
        app.seg_column = SegColumn::Left;
        let n = app.config.prompt.left_segments.len();
        // 全要素 + 1 回 (空リストでの no-op) 削除してもパニックしない
        for _ in 0..=n {
            app.seg_remove();
        }
        assert!(app.config.prompt.left_segments.is_empty());
        assert_eq!(app.seg_left_state.selected(), None);
    }

    #[test]
    fn seg_add_はavailableが空なら何もしない() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 1;
        app.seg_column = SegColumn::Available;
        // Available を空にするまで追加し続ける
        while !available_segments(&app.config).is_empty() {
            app.seg_add();
        }
        let left_len = app.config.prompt.left_segments.len();
        let right_len = app.config.prompt.right_segments.len();
        app.seg_add(); // 空 Available での no-op
        assert_eq!(app.config.prompt.left_segments.len(), left_len);
        assert_eq!(app.config.prompt.right_segments.len(), right_len);
        assert_eq!(app.seg_avail_state.selected(), None);
    }

    #[test]
    fn handle_right_数値が最大値でもオーバーフローしない() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = 0;
        app.prompt_focus = 4;
        app.config.prompt.dir.truncation_length = usize::MAX;
        handle_right(&mut app);
        assert_eq!(app.config.prompt.dir.truncation_length, usize::MAX);

        app.tab = 3;
        app.suggest_focus = 2;
        app.config.suggest.max_suggestions = usize::MAX;
        handle_right(&mut app);
        assert_eq!(app.config.suggest.max_suggestions, usize::MAX);
    }
}
