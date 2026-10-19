use crate::config::{self, Config, CustomSegment, SuggestKeyAction, UiLanguage};
use crate::icons::{FontLevel, Icons};
use crate::style::{self, Color as AppColor};

use crate::prompt::{RAINBOW_PALETTES, rainbow_colors, rainbow_palette};
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs};
use std::time::Duration;

mod block_colors;
mod colors;
mod completions;
mod custom_tab;
mod git_tab;
mod lang;
mod prompt_tab;
mod segments_tab;
mod shell_tab;
mod style_tab;
mod suggest_tab;
use colors::{ColorPicker, color_field_item, color_row};
use custom_tab::CustomItem;
use git_tab::GitItem;
use lang::Lang;
use prompt_tab::PromptItem;
use segments_tab::{SegColumn, available_segments};
use shell_tab::ShellItem;
use style_tab::StyleItem;
use suggest_tab::SuggestItem;

pub(crate) fn interface_is_japanese() -> bool {
    Lang::from_preference(config::load_config().ui.language) == Lang::Ja
}

pub fn history_menu_help() -> &'static str {
    Lang::from_preference(config::load_config().ui.language).text(
        "Up/Down Tab: select  Enter: insert  Esc: cancel",
        "↑/↓ Tab:選択  Enter:入力欄へ  Esc:取消",
    )
}

pub fn history_menu_empty() -> &'static str {
    Lang::from_preference(config::load_config().ui.language)
        .text("No matching history", "一致する履歴がありません")
}

/// 入力欄の下に出すタスク一覧の見出し
pub fn task_list_title(language: UiLanguage) -> &'static str {
    Lang::from_preference(language).text("Tasks", "タスク")
}

/// 入力欄の下に出すファイル一覧の見出し
pub fn file_list_title(language: UiLanguage) -> &'static str {
    Lang::from_preference(language).text("Files", "ファイル")
}

/// 入力欄の下に出すサブコマンド一覧の見出し
pub fn command_list_title(language: UiLanguage) -> &'static str {
    Lang::from_preference(language).text("Commands", "コマンド")
}

/// 入力欄の下に出すオプション一覧の見出し
pub fn option_list_title(language: UiLanguage) -> &'static str {
    Lang::from_preference(language).text("Options", "オプション")
}

#[cfg(test)]
#[path = "tui/settings_tests.rs"]
mod settings_tests;

#[cfg(test)]
#[path = "tui/language_tests.rs"]
mod language_tests;

#[cfg(test)]
#[path = "tui/behavior_tests.rs"]
mod behavior_tests;

// ---------------------------------------------------------------------------
// 定数
// ---------------------------------------------------------------------------

// セグメント名一覧は prompt::ALL_SEGMENT_NAMES を単一ソースとする
const ALL_SEGMENTS: &[&str] = crate::prompt::ALL_SEGMENT_NAMES;

// ---------------------------------------------------------------------------
// タブと設定項目への操作
// ---------------------------------------------------------------------------

/// 設定画面のタブ。表示順は `ALL` の並びで決まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Prompt,
    Segments,
    Git,
    Suggest,
    Style,
    Custom,
    Shell,
    Completions,
}

impl Tab {
    const ALL: [Self; 8] = [
        Self::Prompt,
        Self::Segments,
        Self::Git,
        Self::Suggest,
        Self::Style,
        Self::Custom,
        Self::Shell,
        Self::Completions,
    ];

    /// 表示順での位置
    fn position(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    fn next(self) -> Self {
        cycle_next(self, &Self::ALL)
    }

    fn prev(self) -> Self {
        cycle_prev(self, &Self::ALL)
    }

    fn title(self, lang: Lang) -> &'static str {
        match self {
            Self::Prompt => lang.text("Prompt", "プロンプト"),
            Self::Segments => lang.text("Segments", "配置"),
            Self::Git => "Git",
            Self::Suggest => lang.text("Suggest", "入力候補"),
            Self::Style => lang.text("Style", "配色"),
            Self::Custom => lang.text("Custom", "カスタム"),
            Self::Shell => lang.text("Shell", "シェル"),
            Self::Completions => lang.text("Completions", "補完"),
        }
    }
}

/// 選択や値を動かす向き。↑と←は前、↓と→は次。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Previous,
    Next,
}

/// 設定項目への操作。←→ は `Adjust`、Enter と空白は `Activate`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemAction {
    Adjust(Direction),
    Activate,
}

impl ItemAction {
    /// 選択肢を巡回する項目での向き。決定は「次へ」として扱う。
    fn cycle_direction(self) -> Direction {
        match self {
            Self::Adjust(direction) => direction,
            Self::Activate => Direction::Next,
        }
    }
}

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
    tab: Tab,
    dirty: bool,

    // タブごとの選択中の項目
    prompt_focus: PromptItem,
    git_focus: GitItem,
    suggest_focus: SuggestItem,
    style_focus: StyleItem,
    rainbow_index: usize,
    custom_focus: CustomItem,
    shell_focus: ShellItem,
    completion_focus: completions::DetailItem,
    completion_index: usize,
    completion_detail: bool,
    completion_delete_pending: bool,
    completion_draft: bool,
    completion_draft_prior_dirty: bool,
    launch_font_wizard: bool,
    initial_config_bytes: Option<Option<Vec<u8>>>,
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
            tab: Tab::Prompt,
            dirty: false,
            prompt_focus: PromptItem::Style,
            git_focus: GitItem::Status,
            suggest_focus: SuggestItem::Strategy,
            style_focus: StyleItem::PrimaryColor,
            rainbow_index: 0,
            custom_focus: CustomItem::Selector,
            shell_focus: ShellItem::CompletionDirs,
            completion_focus: completions::DetailItem::Back,
            completion_index: 0,
            completion_detail: false,
            completion_delete_pending: false,
            completion_draft: false,
            completion_draft_prior_dirty: false,
            launch_font_wizard: false,
            initial_config_bytes: None,
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

    /// タブを切り替える。補完タブは一覧から始める。
    fn switch_tab(&mut self, tab: Tab) {
        self.tab = tab;
        if tab == Tab::Completions {
            self.completion_detail = false;
        }
    }

    /// ↑↓ で選択を動かす。配置タブと補完タブは専用の動きをする。
    fn move_focus(&mut self, direction: Direction) {
        match self.tab {
            Tab::Prompt => move_item(&PromptItem::ALL, &mut self.prompt_focus, direction),
            Tab::Segments => self.seg_focus_move(direction),
            Tab::Git => move_item(&GitItem::ALL, &mut self.git_focus, direction),
            Tab::Suggest => move_item(&SuggestItem::ALL, &mut self.suggest_focus, direction),
            Tab::Style => move_item(
                &style_tab::visible_items(self),
                &mut self.style_focus,
                direction,
            ),
            Tab::Custom => move_item(
                &custom_tab::visible_items(self),
                &mut self.custom_focus,
                direction,
            ),
            Tab::Shell => move_item(&ShellItem::ALL, &mut self.shell_focus, direction),
            Tab::Completions => completions::move_focus(self, direction),
        }
    }

    fn save(&mut self) {
        if let Some(initial) = &self.initial_config_bytes {
            let current = match std::fs::read(config::config_path()) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    self.status_msg = Some(format!(
                        "{}: {e}",
                        self.lang.text("Read error", "読み込みエラー")
                    ));
                    return;
                }
            };
            if &current != initial {
                self.status_msg = Some(
                    self.lang
                        .text(
                            "Config changed outside this screen. Reopen it before saving.",
                            "別の操作で設定ファイルが変わりました。画面を開き直してください。",
                        )
                        .into(),
                );
                return;
            }
        }
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
                if self.initial_config_bytes.is_some() {
                    self.initial_config_bytes = Some(std::fs::read(config::config_path()).ok());
                }
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

    /// 決定で編集を始める項目。左右キーでは何もしない。
    fn edit_on_activate(&mut self, action: ItemAction, target: EditTarget) {
        match action {
            ItemAction::Activate => self.activate_edit(target),
            ItemAction::Adjust(_) => {}
        }
    }

    fn is_editing(&self, target: &EditTarget) -> bool {
        self.editing.as_ref() == Some(target)
    }

    /// 項目に表示する値。編集中の項目は入力中の文字列に `|` を付けて見せる。
    fn shown_value(&self, target: &EditTarget, value: &str) -> String {
        if self.is_editing(target) {
            format!("{}|", self.edit_buffer)
        } else {
            value.to_owned()
        }
    }

    /// 真偽値の設定を反転し、未保存にする
    fn toggle_setting(&mut self, flag: impl FnOnce(&mut Config) -> &mut bool) {
        let flag = flag(&mut self.config);
        *flag = !*flag;
        self.dirty = true;
    }

    /// 文字列の設定を選択肢の中で前後に巡回させ、未保存にする。不明な値は先頭とみなす。
    fn cycle_setting(
        &mut self,
        direction: Direction,
        options: &[&str],
        value: impl FnOnce(&mut Config) -> &mut String,
    ) {
        let value = value(&mut self.config);
        *value = cycle_option(value.as_str(), options, direction).to_owned();
        self.dirty = true;
    }

    /// 数の設定を 1 つ増減する。0 では減らさず未保存にもしない。上限では値を保ったまま未保存にする。
    fn step_count(&mut self, direction: Direction, value: impl FnOnce(&mut Config) -> &mut usize) {
        let value = value(&mut self.config);
        match direction {
            Direction::Previous if *value > 0 => *value -= 1,
            Direction::Previous => return,
            Direction::Next => *value = value.saturating_add(1),
        }
        self.dirty = true;
    }

    fn confirm_edit(&mut self) {
        let Some(target) = self.editing.clone() else {
            return;
        };
        let value = self.edit_buffer.clone();
        if let Err(message) = self.validate_edit(&target, &value) {
            // 検証に失敗したら設定へ反映せず、編集を続ける
            self.status_msg = Some(message.into());
            return;
        }
        self.apply_edit(target, value);
        self.finish_edit();
    }

    /// 検証済みの編集値を設定へ反映する
    fn apply_edit(&mut self, target: EditTarget, value: String) {
        match target {
            EditTarget::BlankLines => self.config.prompt.blank_lines = value.parse().unwrap(),
            EditTarget::HomeSymbol => self.config.prompt.dir.home_symbol = value,
            EditTarget::TruncationSymbol => self.config.prompt.dir.truncation_symbol = value,
            EditTarget::TruncationLength => {
                self.config.prompt.dir.truncation_length = value.parse().unwrap()
            }
            EditTarget::MaxSuggestions => {
                self.config.suggest.max_suggestions = value.parse().unwrap()
            }
            EditTarget::IpInterface => self.config.prompt.ip_interface = value,
            EditTarget::CompletionDirs => self.config.shell.completion_dirs = value,
            EditTarget::Completion(index, field) => {
                field.set(&mut self.config.completions[index], &value);
                // CLI 名を確定した新規登録は、以後の取消しで消さない
                if field == completions::Field::Command {
                    self.completion_draft = false;
                }
            }
            EditTarget::Custom(index, field) => {
                if field == CustomField::Name {
                    self.rename_custom(index, &value);
                }
                *field.value(&mut self.config.prompt.custom[index]) = value;
                self.refresh_segment_selection();
            }
            EditTarget::HighlightColor => self.config.suggest.highlight_color = value,
            EditTarget::PrimaryColor => self.config.style.primary_color = value,
            EditTarget::SuccessColor => self.config.style.success_color = value,
            EditTarget::ErrorColor => self.config.style.error_color = value,
            EditTarget::MutedColor => self.config.style.muted_color = value,
            EditTarget::RainbowFg(name) => self.set_rainbow_color(name, true, value),
            EditTarget::RainbowBg(name) => self.set_rainbow_color(name, false, value),
        }
    }

    /// 編集を閉じて未保存にする
    fn finish_edit(&mut self) {
        self.editing = None;
        self.dirty = true;
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

    fn cancel_edit(&mut self) {
        if self.completion_draft
            && matches!(
                self.editing,
                Some(EditTarget::Completion(_, completions::Field::Command))
            )
        {
            completions::remove(self);
            self.completion_draft = false;
            self.dirty = self.completion_draft_prior_dirty;
        }
        self.editing = None;
        self.edit_buffer.clear();
    }
}

// ---------------------------------------------------------------------------
// ヘルパー関数
// ---------------------------------------------------------------------------

/// 項目の並びの中で選択中の項目の位置。消えた項目を選んでいたときは末尾の項目とみなす。
fn focus_position<T: PartialEq>(items: &[T], focus: &T) -> usize {
    items
        .iter()
        .position(|item| item == focus)
        .unwrap_or(items.len().saturating_sub(1))
}

/// 操作の対象にする項目。消えた項目を選んでいたときは `focus_position` と同じく末尾の項目にする。
fn focused_item<T: Copy + PartialEq>(items: &[T], focus: T) -> T {
    items
        .get(focus_position(items, &focus))
        .copied()
        .unwrap_or(focus)
}

/// 選択を前後へ 1 つ動かす。端では動かず、消えた項目からは末尾の項目を起点にする。
fn move_item<T: Copy + PartialEq>(items: &[T], focus: &mut T, direction: Direction) {
    let index = focus_position(items, focus);
    let next = match direction {
        Direction::Previous => index.checked_sub(1),
        Direction::Next => index.checked_add(1),
    };
    if let Some(item) = next.and_then(|index| items.get(index)) {
        *focus = *item;
    }
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

/// 選択肢を向きに沿って 1 つ巡回させる。不明な値は先頭とみなす。
fn cycle_option<T: Copy + PartialEq>(current: T, options: &[T], direction: Direction) -> T {
    let index = options
        .iter()
        .position(|option| *option == current)
        .unwrap_or(0);
    options[cycle_index(index, options.len(), direction)]
}

fn cycle_next<T: Copy + PartialEq>(current: T, options: &[T]) -> T {
    cycle_option(current, options, Direction::Next)
}

fn cycle_prev<T: Copy + PartialEq>(current: T, options: &[T]) -> T {
    cycle_option(current, options, Direction::Previous)
}

/// 0 から len 未満の位置を向きに沿って 1 つ動かし、端では反対側へ回る
fn cycle_index(index: usize, len: usize, direction: Direction) -> usize {
    match direction {
        Direction::Previous => (index + len - 1) % len,
        Direction::Next => (index + 1) % len,
    }
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

/// プレビューの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewKind {
    /// 設定を反映したプロンプト
    Prompt,
    /// 空行数を選んでいる間は、プロンプトを 2 つ並べて間隔を見せる
    Spacing,
}

fn preview_kind(app: &App) -> PreviewKind {
    if app.tab == Tab::Prompt && app.prompt_focus == PromptItem::BlankLines {
        PreviewKind::Spacing
    } else {
        PreviewKind::Prompt
    }
}

fn ui(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let width = area.width.saturating_sub(2);
    let edit = app
        .editing
        .as_ref()
        .map(|target| (target, app.edit_buffer.as_str()));
    let preview_kind = preview_kind(app);
    let preview_text = match preview_kind {
        PreviewKind::Prompt => render_preview(&app.config, width, edit),
        PreviewKind::Spacing => render_spacing_preview(&app.config, width, edit),
    };
    let preview_text = wrap_preview(preview_text, width);
    // プレビューの下に残す高さ
    let reserved = match (app.color_picker.is_some(), preview_kind) {
        (true, _) => 6,
        (false, PreviewKind::Spacing) => 8,
        (false, PreviewKind::Prompt) => 10,
    };
    let preview_height = (preview_text
        .lines
        .len()
        .saturating_add(2)
        .min(u16::MAX as usize) as u16)
        .min(area.height.saturating_sub(reserved));
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
    let tabs = Tabs::new(Tab::ALL.map(|tab| tab.title(app.lang)))
        .select(app.tab.position())
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(tabs, chunks[1]);

    // -- コンテンツ --
    let content_area = chunks[2];
    render_tab_content(frame, app, content_area);

    // -- ヘルプバー --
    let dirty_indicator = if app.dirty {
        app.lang.text(" [UNSAVED]", " [未保存]")
    } else {
        ""
    };
    let status = app.status_msg.as_deref().unwrap_or("");
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
        Paragraph::new(help_text(app))
            .wrap(ratatui::widgets::Wrap { trim: true })
            .style(Style::default().fg(Color::DarkGray)),
        chunks[4],
    );
    if let Some(target) = &app.editing {
        render_editor(frame, app, target, content_area);
    }
}

/// 選択中のタブの内容を描画する
fn render_tab_content(frame: &mut Frame, app: &mut App, area: Rect) {
    match app.tab {
        Tab::Prompt => prompt_tab::render(frame, app, area),
        Tab::Segments => segments_tab::render(frame, app, area),
        Tab::Git => git_tab::render(frame, app, area),
        Tab::Suggest => suggest_tab::render(frame, app, area),
        Tab::Style => style_tab::render(frame, app, area),
        Tab::Custom => custom_tab::render(frame, app, area),
        Tab::Shell => shell_tab::render(frame, app, area),
        Tab::Completions => completions::render(frame, app, area),
    }
}

/// ヘルプバーの文言。確認や編集の案内を優先し、それ以外はタブごとに出し分ける。
fn help_text(app: &App) -> &'static str {
    let lang = app.lang;
    if app.quit_state == QuitState::Confirming {
        lang.text(
            "Unsaved changes! Press Esc again to quit, or any other key to cancel.",
            "未保存の変更があります。Esc:破棄して終了  その他のキー:戻る",
        )
    } else if app.custom_delete_pending {
        lang.text(
            "⌫ again: remove definition and placements  Any other key: cancel",
            "再度 ⌫:定義と配置を削除  その他のキー:戻る",
        )
    } else if app.completion_delete_pending {
        lang.text(
            "⌫/d again: remove selected CLI  Any other key: cancel",
            "再度 ⌫/d:選択中の CLI を削除  その他のキー:取消",
        )
    } else if app.editing.is_some() {
        lang.text(
            "Enter:confirm  Esc:cancel  Arrows/Home/End:cursor  Alt+Enter:newline",
            "Enter:確定  Esc:取消  矢印/Home/End:移動  Alt+Enter:改行",
        )
    } else {
        tab_help_text(app)
    }
}

fn tab_help_text(app: &App) -> &'static str {
    let lang = app.lang;
    match app.tab {
        Tab::Completions if app.completion_detail => lang.text(
            "↑/↓:field  Enter:edit  n:add  ⌫/d:delete  s:save  Esc:list",
            "↑/↓:項目  Enter:編集  n:追加  ⌫/d:削除  s:保存  Esc:一覧",
        ),
        Tab::Completions => lang.text(
            "↑/↓:select  Enter:settings  n:add  ⌫/d:delete  s:save  Esc:quit",
            "↑/↓:選択  Enter:設定  n:追加  ⌫/d:削除  s:保存  Esc:終了",
        ),
        Tab::Segments => lang.text(
            "Tab:switch  hjkl:nav  L/R:add to side  ⌫:remove  u/d:order  S:save  Esc:quit",
            "Tab:切替  hjkl:移動  L/R:追加  ⌫:削除  u/d:順序  S:保存  Esc:終了",
        ),
        Tab::Custom => lang.text(
            "Tab:switch  jk:field  hl:select  n:new  ⌫:delete  Enter:edit  S:save  Esc:quit",
            "Tab:切替  jk:項目  hl:選択  n:作成  ⌫:削除  Enter:編集  S:保存 Esc:終了",
        ),
        Tab::Prompt | Tab::Git | Tab::Suggest | Tab::Style | Tab::Shell => lang.text(
            "Tab:switch  jk:nav  hl:change  Enter:edit/toggle  S:save  Esc:quit",
            "Tab:切替  jk:移動  hl:変更  Enter:編集/切替  S:保存  Esc:終了",
        ),
    }
}

fn render_editor(frame: &mut Frame, app: &App, target: &EditTarget, area: Rect) {
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

/// 項目を並びの順に 1 行ずつ描画し、選択中の項目を強調する
fn render_items<T: Copy + PartialEq>(
    frame: &mut Frame,
    items: &[T],
    focus: T,
    title: &str,
    area: Rect,
    mut row: impl FnMut(T, bool) -> ListItem<'static>,
) {
    let selected = focus_position(items, &focus);
    let rows: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(index, item)| row(*item, index == selected))
        .collect();
    // 選択中の行が見える位置まで一覧をスクロールする
    let mut state = ListState::default().with_selected(Some(selected));
    let list = List::new(rows).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_stateful_widget(list, area, &mut state);
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

    // 削除確認・終了確認・色の選択・テキスト編集の順に、表示中の確認や入力欄へキーを渡す
    if app.custom_delete_pending {
        app.custom_delete_pending = false;
        if matches!(key.code, KeyCode::Delete | KeyCode::Backspace) {
            app.custom_remove();
        }
        return false;
    }

    if app.completion_delete_pending {
        app.completion_delete_pending = false;
        if matches!(
            key.code,
            KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('d')
        ) {
            completions::remove(app);
        }
        return false;
    }

    if app.quit_state == QuitState::Confirming {
        if key.code == KeyCode::Esc {
            return true; // 確定終了
        }
        app.quit_state = QuitState::Normal;
        return false;
    }

    if app.color_picker.is_some() {
        colors::handle_picker(app, key);
        return false;
    }
    if app.editing.is_some() {
        handle_edit_key(app, key);
        return false;
    }
    handle_normal_key(app, key)
}

/// 確認や入力欄が無いときのキー。共通のキーを先に処理し、残りはタブ専用のキーとして扱う。
fn handle_normal_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('s') | KeyCode::Char('S') => app.save(),
        KeyCode::Tab | KeyCode::Char(']') => app.switch_tab(app.tab.next()),
        KeyCode::BackTab | KeyCode::Char('[') => app.switch_tab(app.tab.prev()),
        KeyCode::Up | KeyCode::Char('k') => app.move_focus(Direction::Previous),
        KeyCode::Down | KeyCode::Char('j') => app.move_focus(Direction::Next),
        KeyCode::Left | KeyCode::Char('h') => {
            handle_item_action(app, ItemAction::Adjust(Direction::Previous))
        }
        KeyCode::Right | KeyCode::Char('l') => {
            handle_item_action(app, ItemAction::Adjust(Direction::Next))
        }
        KeyCode::Char(' ') | KeyCode::Enter => handle_item_action(app, ItemAction::Activate),
        KeyCode::Esc => return handle_escape(app),
        code => handle_tab_key(app, code),
    }
    false
}

/// Esc。補完の詳細画面では一覧へ戻り、それ以外では終了する（未保存なら確認する）。
fn handle_escape(app: &mut App) -> bool {
    if app.tab == Tab::Completions && app.completion_detail {
        completions::close_detail(app);
        return false;
    }
    if app.dirty {
        app.quit_state = QuitState::Confirming;
        false
    } else {
        true
    }
}

/// タブ専用のキー
fn handle_tab_key(app: &mut App, code: KeyCode) {
    match app.tab {
        Tab::Segments => segments_tab::handle_key(app, code),
        Tab::Custom => custom_tab::handle_key(app, code),
        Tab::Completions => completions::handle_key(app, code),
        Tab::Prompt | Tab::Git | Tab::Suggest | Tab::Style | Tab::Shell => {}
    }
}

/// ←→ と決定を、選択中の項目へ渡す
fn handle_item_action(app: &mut App, action: ItemAction) {
    match app.tab {
        Tab::Prompt => app.prompt_focus.handle(app, action),
        Tab::Segments => segments_tab::handle(app, action),
        Tab::Git => app.git_focus.handle(app, action),
        Tab::Suggest => app.suggest_focus.handle(app, action),
        Tab::Style => {
            focused_item(&style_tab::visible_items(app), app.style_focus).handle(app, action)
        }
        Tab::Custom => {
            focused_item(&custom_tab::visible_items(app), app.custom_focus).handle(app, action)
        }
        Tab::Shell => app.shell_focus.handle(app, action),
        Tab::Completions => completions::handle(app, action),
    }
}

/// テキスト編集中のキー入力。カーソルは UTF-8 のバイト位置で、常に文字境界に置く。
fn handle_edit_key(app: &mut App, key: KeyEvent) {
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
}

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
            if app.launch_font_wizard {
                break;
            }
        }
    }
    Ok(())
}

pub fn run_tui() -> std::io::Result<()> {
    let config = config::load_config_strict()?;
    let lang = Lang::from_preference(config.ui.language);
    let initial = match std::fs::read(config::config_path()) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let mut terminal = ratatui::init();
    let mut app = App::new(config, lang);
    app.initial_config_bytes = Some(initial);
    let result = run_app(&mut terminal, &mut app);
    ratatui::restore();
    result?;
    if app.launch_font_wizard {
        crate::font_wizard::run()?;
    }
    Ok(())
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
        assert_eq!(app.tab, Tab::Prompt);
        assert!(!app.dirty);
        assert_eq!(app.prompt_focus, PromptItem::Style);
        assert_eq!(app.seg_column, SegColumn::Left);
        assert!(app.editing.is_none());
        assert_eq!(app.quit_state, QuitState::Normal);
    }

    // --- 項目の並びと選択 ---

    /// 並びの中で同じ項目が 2 回現れない
    fn assert_distinct<T: PartialEq + std::fmt::Debug>(items: &[T]) {
        for (index, item) in items.iter().enumerate() {
            assert_eq!(
                items.iter().position(|other| other == item),
                Some(index),
                "{item:?}"
            );
        }
    }

    #[test]
    fn タブと項目の並びに同じものが2回現れない() {
        assert_distinct(&Tab::ALL);
        assert_distinct(&PromptItem::ALL);
        assert_distinct(&GitItem::ALL);
        assert_distinct(&SuggestItem::ALL);
        assert_distinct(&StyleItem::ALL);
        assert_distinct(&ShellItem::ALL);
        assert_distinct(&completions::DetailItem::ALL);
        for (index, tab) in Tab::ALL.iter().enumerate() {
            assert_eq!(tab.position(), index);
        }
        assert_eq!(Tab::Completions.next(), Tab::Prompt);
        assert_eq!(Tab::Prompt.prev(), Tab::Completions);
    }

    #[test]
    fn focus_position_は消えた項目を末尾の項目とみなす() {
        let items = [1, 2, 3];
        assert_eq!(focus_position(&items, &2), 1);
        assert_eq!(focus_position(&items, &9), 2);
        assert_eq!(focused_item(&items, 9), 3);
        // 項目が無くても panic しない
        assert_eq!(focus_position::<i32>(&[], &1), 0);
        assert_eq!(focused_item(&[], 1), 1);
    }

    #[test]
    fn move_item_は端で止まり消えた項目からは末尾を起点に動く() {
        let items = [1, 2, 3];
        let mut focus = 1;
        move_item(&items, &mut focus, Direction::Previous);
        assert_eq!(focus, 1);
        move_item(&items, &mut focus, Direction::Next);
        assert_eq!(focus, 2);
        focus = 3;
        move_item(&items, &mut focus, Direction::Next);
        assert_eq!(focus, 3);
        // 消えた項目からは下へ動かず選択を保ち、上へは末尾の 1 つ前へ動く
        focus = 9;
        move_item(&items, &mut focus, Direction::Next);
        assert_eq!(focus, 9);
        move_item(&items, &mut focus, Direction::Previous);
        assert_eq!(focus, 2);
    }

    #[test]
    fn app_move_focus_は範囲内で移動する() {
        let mut app = App::new(Config::default(), Lang::En);
        app.move_focus(Direction::Next);
        assert_eq!(app.prompt_focus, PromptItem::FontLevel);
        app.move_focus(Direction::Next);
        assert_eq!(app.prompt_focus, PromptItem::Transient);
        app.move_focus(Direction::Previous);
        assert_eq!(app.prompt_focus, PromptItem::FontLevel);
    }

    #[test]
    fn app_move_focus_は先頭より上へ動かない() {
        let mut app = App::new(Config::default(), Lang::En);
        app.move_focus(Direction::Previous);
        assert_eq!(app.prompt_focus, PromptItem::Style);
    }

    #[test]
    fn app_move_focus_は末尾より下へ動かない() {
        let mut app = App::new(Config::default(), Lang::En);
        for _ in 0..20 {
            app.move_focus(Direction::Next);
        }
        assert_eq!(app.prompt_focus, PromptItem::BlankLines);
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
        assert_eq!(app.tab, Tab::Prompt);
        let key = KeyEvent::from(KeyCode::Tab);
        handle_event(&mut app, key);
        assert_eq!(app.tab, Tab::Segments);
    }

    #[test]
    fn handle_event_tabキーは末尾から先頭へ巡回() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = Tab::Completions;
        let key = KeyEvent::from(KeyCode::Tab);
        handle_event(&mut app, key);
        assert_eq!(app.tab, Tab::Prompt);
    }

    #[test]
    fn handle_event_jkキーでフォーカス移動() {
        let mut app = App::new(Config::default(), Lang::En);
        let down = KeyEvent::from(KeyCode::Char('j'));
        handle_event(&mut app, down);
        assert_eq!(app.prompt_focus, PromptItem::FontLevel);
        let up = KeyEvent::from(KeyCode::Char('k'));
        handle_event(&mut app, up);
        assert_eq!(app.prompt_focus, PromptItem::Style);
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
        app.tab = Tab::Segments;
        app.seg_column = SegColumn::Left;
        app.seg_column_left();
        assert_eq!(app.seg_column, SegColumn::Left);
    }

    #[test]
    fn seg_column_rightは右端で変化しない() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = Tab::Segments;
        app.seg_column = SegColumn::Available;
        app.seg_column_right();
        assert_eq!(app.seg_column, SegColumn::Available);
    }

    #[test]
    fn seg_column_の左右移動() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = Tab::Segments;
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
        app.tab = Tab::Segments;
        app.seg_column = SegColumn::Left;
        let before = app.config.prompt.left_segments.len();
        app.seg_add();
        assert_eq!(app.config.prompt.left_segments.len(), before);
    }

    #[test]
    fn seg_add_はavailableから左に追加する() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = Tab::Segments;
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
        app.tab = Tab::Segments;
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
        app.tab = Tab::Segments;
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
        app.tab = Tab::Segments;
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
        app.tab = Tab::Segments;
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

    #[test]
    fn handle_event_編集モードのsは保存せず文字として入る() {
        let mut app = App::new(Config::default(), Lang::En);
        // 誤って保存処理に入っても、外部変更の検知で止まり実環境の設定を書き換えない
        app.initial_config_bytes = Some(Some(b"\0not-a-config".to_vec()));
        app.start_edit(EditTarget::HomeSymbol);
        app.edit_buffer.clear();
        app.edit_cursor = 0;
        for c in ['s', 'S'] {
            assert!(!handle_event(&mut app, KeyEvent::from(KeyCode::Char(c))));
        }
        assert_eq!(app.edit_buffer, "sS");
        assert!(app.editing.is_some());
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn handle_event_alt_enterはコマンド欄以外では通常の確定になる() {
        let mut app = App::new(Config::default(), Lang::En);
        app.start_edit(EditTarget::HomeSymbol);
        app.edit_buffer = "NEW".into();
        app.edit_cursor = app.edit_buffer.len();
        handle_event(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
        assert!(app.editing.is_none());
        assert_eq!(app.config.prompt.dir.home_symbol, "NEW");
    }

    // ── セグメントリストの空化境界 ─────────────────────────────

    #[test]
    fn seg_remove_はリストを空にし切っても安全() {
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = Tab::Segments;
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
        app.tab = Tab::Segments;
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
    fn 右キーで数値が最大値でもオーバーフローしない() {
        let right = ItemAction::Adjust(Direction::Next);
        let mut app = App::new(Config::default(), Lang::En);
        app.tab = Tab::Prompt;
        app.prompt_focus = PromptItem::TruncationLength;
        app.config.prompt.dir.truncation_length = usize::MAX;
        handle_item_action(&mut app, right);
        assert_eq!(app.config.prompt.dir.truncation_length, usize::MAX);

        app.tab = Tab::Suggest;
        app.suggest_focus = SuggestItem::MaxSuggestions;
        app.config.suggest.max_suggestions = usize::MAX;
        handle_item_action(&mut app, right);
        assert_eq!(app.config.suggest.max_suggestions, usize::MAX);
    }
}
