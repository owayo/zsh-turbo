use super::*;
use crate::config::SuggestKeys;

const SUGGEST_STRATEGIES: &[&str] = &["prefix", "substring", "fuzzy"];
const SUGGEST_KEY_ACTIONS: &[SuggestKeyAction] = &[
    SuggestKeyAction::Default,
    SuggestKeyAction::Full,
    SuggestKeyAction::Step,
    SuggestKeyAction::Word,
];

/// 候補の採用に割り当てられるキー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SuggestKey {
    Tab,
    Right,
    AltF,
    CtrlRight,
}

impl SuggestKey {
    fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Tab => "Tab",
            Self::Right => lang.text("Right Arrow", "右矢印"),
            Self::AltF => "Alt+F",
            Self::CtrlRight => lang.text("Ctrl+Right", "Ctrl+右矢印"),
        }
    }

    fn action(self, keys: &SuggestKeys) -> SuggestKeyAction {
        match self {
            Self::Tab => keys.tab,
            Self::Right => keys.right,
            Self::AltF => keys.alt_f,
            Self::CtrlRight => keys.ctrl_right,
        }
    }

    fn action_mut(self, keys: &mut SuggestKeys) -> &mut SuggestKeyAction {
        match self {
            Self::Tab => &mut keys.tab,
            Self::Right => &mut keys.right,
            Self::AltF => &mut keys.alt_f,
            Self::CtrlRight => &mut keys.ctrl_right,
        }
    }
}

/// 入力候補タブの設定項目。表示順は `ALL` の並びで決まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SuggestItem {
    Strategy,
    HighlightColor,
    MaxSuggestions,
    Key(SuggestKey),
    RecordTaskUsage,
    RecordDirectoryHistory,
}

impl SuggestItem {
    pub(super) const ALL: [Self; 9] = [
        Self::Strategy,
        Self::HighlightColor,
        Self::MaxSuggestions,
        Self::Key(SuggestKey::Tab),
        Self::Key(SuggestKey::Right),
        Self::Key(SuggestKey::AltF),
        Self::Key(SuggestKey::CtrlRight),
        Self::RecordTaskUsage,
        Self::RecordDirectoryHistory,
    ];

    pub(super) fn handle(self, app: &mut App, action: ItemAction) {
        match self {
            Self::Strategy => {
                app.cycle_setting(action.cycle_direction(), SUGGEST_STRATEGIES, |config| {
                    &mut config.suggest.strategy
                })
            }
            Self::HighlightColor => app.edit_on_activate(action, EditTarget::HighlightColor),
            Self::MaxSuggestions => match action {
                ItemAction::Adjust(direction) => {
                    app.step_count(direction, |config| &mut config.suggest.max_suggestions)
                }
                ItemAction::Activate => app.activate_edit(EditTarget::MaxSuggestions),
            },
            Self::Key(key) => {
                let slot = key.action_mut(&mut app.config.suggest.keys);
                *slot = cycle_option(*slot, SUGGEST_KEY_ACTIONS, action.cycle_direction());
                app.dirty = true;
            }
            Self::RecordTaskUsage => {
                app.toggle_setting(|config| &mut config.suggest.record_task_usage)
            }
            Self::RecordDirectoryHistory => {
                app.toggle_setting(|config| &mut config.suggest.record_directory_history)
            }
        }
    }

    fn row(self, app: &App, focused: bool) -> ListItem<'static> {
        let lang = app.lang;
        let suggest = &app.config.suggest;
        match self {
            Self::Strategy => field_item(
                lang.text("Strategy", "候補の検索方法"),
                &suggest.strategy,
                focused,
                false,
            ),
            Self::HighlightColor => color_row(
                app,
                lang.text("Highlight Color", "候補の色"),
                &suggest.highlight_color,
                focused,
                EditTarget::HighlightColor,
            ),
            Self::MaxSuggestions => field_item(
                lang.text("Max Suggestions", "候補の最大数"),
                &suggest.max_suggestions.to_string(),
                focused,
                false,
            ),
            Self::Key(key) => field_item(
                key.label(lang),
                suggest_key_action_label(key.action(&suggest.keys), lang),
                focused,
                false,
            ),
            Self::RecordTaskUsage => field_item(
                lang.text("Record Task Usage", "タスクの利用を記録"),
                bool_str(suggest.record_task_usage, lang),
                focused,
                false,
            ),
            Self::RecordDirectoryHistory => field_item(
                lang.text("Record Directory History", "ディレクトリ別の履歴を記録"),
                bool_str(suggest.record_directory_history, lang),
                focused,
                false,
            ),
        }
    }
}

fn suggest_key_action_label(action: SuggestKeyAction, lang: Lang) -> &'static str {
    match action {
        SuggestKeyAction::Default => lang.text("Normal key action", "通常のキー操作"),
        SuggestKeyAction::Full => lang.text("Accept all", "全体を採用"),
        SuggestKeyAction::Step => lang.text("Accept path segment", "階層・単語を採用"),
        SuggestKeyAction::Word => lang.text("Accept word", "単語を採用"),
    }
}

pub(super) fn render(frame: &mut Frame, app: &App, area: Rect) {
    render_items(
        frame,
        &SuggestItem::ALL,
        app.suggest_focus,
        app.lang.text(" Suggest Settings ", " 入力候補の設定 "),
        area,
        |item, focused| item.row(app, focused),
    );
}
