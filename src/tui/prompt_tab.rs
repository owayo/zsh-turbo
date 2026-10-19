use super::*;

const PROMPT_STYLES: &[&str] = &["lean", "classic", "rainbow", "pure"];
const FONT_LEVELS: &[&str] = &["nerd", "powerline", "unicode", "ascii"];

/// プロンプトタブの設定項目。表示順は `ALL` の並びで決まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PromptItem {
    Style,
    FontLevel,
    Transient,
    HomeSymbol,
    TruncationLength,
    TruncationSymbol,
    IpInterface,
    Newline,
    BlankLines,
}

impl PromptItem {
    pub(super) const ALL: [Self; 9] = [
        Self::Style,
        Self::FontLevel,
        Self::Transient,
        Self::HomeSymbol,
        Self::TruncationLength,
        Self::TruncationSymbol,
        Self::IpInterface,
        Self::Newline,
        Self::BlankLines,
    ];

    pub(super) fn handle(self, app: &mut App, action: ItemAction) {
        match self {
            Self::Style => app.cycle_setting(action.cycle_direction(), PROMPT_STYLES, |config| {
                &mut config.prompt.prompt_style
            }),
            Self::FontLevel => app.cycle_setting(action.cycle_direction(), FONT_LEVELS, |config| {
                &mut config.prompt.font_level
            }),
            // 過去のプロンプトの簡略化は決定でだけ切り替える
            Self::Transient => match action {
                ItemAction::Activate => app.toggle_setting(|config| &mut config.prompt.transient),
                ItemAction::Adjust(_) => {}
            },
            Self::HomeSymbol => app.edit_on_activate(action, EditTarget::HomeSymbol),
            Self::TruncationLength => match action {
                ItemAction::Adjust(direction) => {
                    app.step_count(direction, |config| &mut config.prompt.dir.truncation_length)
                }
                ItemAction::Activate => app.activate_edit(EditTarget::TruncationLength),
            },
            Self::TruncationSymbol => app.edit_on_activate(action, EditTarget::TruncationSymbol),
            Self::IpInterface => app.edit_on_activate(action, EditTarget::IpInterface),
            Self::Newline => app.toggle_setting(|config| &mut config.prompt.newline),
            Self::BlankLines => match action {
                ItemAction::Adjust(direction) => step_blank_lines(app, direction),
                ItemAction::Activate => app.activate_edit(EditTarget::BlankLines),
            },
        }
    }

    fn row(self, app: &App, focused: bool) -> ListItem<'static> {
        let lang = app.lang;
        let prompt = &app.config.prompt;
        match self {
            Self::Style => field_item(
                lang.text("Prompt Style", "スタイル"),
                &prompt.prompt_style,
                focused,
                false,
            ),
            Self::FontLevel => field_item(
                lang.text("Font Level", "フォント種別"),
                &prompt.font_level,
                focused,
                false,
            ),
            Self::Transient => field_item(
                lang.text("Transient Prompt", "過去のプロンプトを簡略化"),
                bool_str(prompt.transient, lang),
                focused,
                false,
            ),
            Self::HomeSymbol => {
                let target = EditTarget::HomeSymbol;
                field_item(
                    lang.text("Home Symbol", "ホームの記号"),
                    &app.shown_value(&target, &prompt.dir.home_symbol),
                    focused,
                    app.is_editing(&target),
                )
            }
            Self::TruncationLength => field_item(
                lang.text("Truncation Length", "表示する階層数"),
                &prompt.dir.truncation_length.to_string(),
                focused,
                false,
            ),
            Self::TruncationSymbol => field_item(
                lang.text("Truncation Symbol", "省略記号"),
                &prompt.dir.truncation_symbol,
                focused,
                false,
            ),
            Self::IpInterface => field_item(
                lang.text(
                    "IP Interface (auto if empty)",
                    "IP インターフェース（空欄で自動）",
                ),
                &prompt.ip_interface,
                focused,
                false,
            ),
            Self::Newline => field_item(
                lang.text("Command input", "コマンド入力位置"),
                if prompt.newline {
                    lang.text("Next line", "次の行")
                } else {
                    lang.text("Same line", "同じ行")
                },
                focused,
                false,
            ),
            Self::BlankLines => field_item(
                lang.text(
                    "Blank lines between prompts (0–10)",
                    "プロンプト間の空行（0–10）",
                ),
                &prompt
                    .blank_lines
                    .min(config::MAX_PROMPT_BLANK_LINES)
                    .to_string(),
                focused,
                false,
            ),
        }
    }
}

/// 空行数を 0〜上限の範囲で 1 つ増減する。境界で値が変わらなくても未保存にする。
fn step_blank_lines(app: &mut App, direction: Direction) {
    let lines = app.config.prompt.blank_lines;
    app.config.prompt.blank_lines = match direction {
        Direction::Previous => lines.min(config::MAX_PROMPT_BLANK_LINES).saturating_sub(1),
        Direction::Next => lines.saturating_add(1).min(config::MAX_PROMPT_BLANK_LINES),
    };
    app.dirty = true;
}

pub(super) fn render(frame: &mut Frame, app: &App, area: Rect) {
    render_items(
        frame,
        &PromptItem::ALL,
        app.prompt_focus,
        app.lang.text(" Prompt Settings ", " プロンプト設定 "),
        area,
        |item, focused| item.row(app, focused),
    );
}
