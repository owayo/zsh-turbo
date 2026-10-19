use super::*;

/// Git タブの設定項目。表示順は `ALL` の並びで決まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GitItem {
    Status,
    AheadBehind,
    Stash,
}

impl GitItem {
    pub(super) const ALL: [Self; 3] = [Self::Status, Self::AheadBehind, Self::Stash];

    /// 項目が切り替える表示設定
    fn flag(self, config: &mut Config) -> &mut bool {
        let git = &mut config.prompt.git;
        match self {
            Self::Status => &mut git.show_status,
            Self::AheadBehind => &mut git.show_ahead_behind,
            Self::Stash => &mut git.show_stash,
        }
    }

    pub(super) fn handle(self, app: &mut App, action: ItemAction) {
        match self {
            // 表示の有無は決定でだけ切り替える
            Self::Status | Self::AheadBehind | Self::Stash => match action {
                ItemAction::Activate => app.toggle_setting(|config| self.flag(config)),
                ItemAction::Adjust(_) => {}
            },
        }
    }

    fn row(self, app: &App, focused: bool) -> ListItem<'static> {
        let lang = app.lang;
        let git = &app.config.prompt.git;
        let (label, value) = match self {
            Self::Status => (lang.text("Show Status", "変更状態を表示"), git.show_status),
            Self::AheadBehind => (
                lang.text("Show Ahead/Behind", "先行・遅延を表示"),
                git.show_ahead_behind,
            ),
            Self::Stash => (lang.text("Show Stash", "スタッシュを表示"), git.show_stash),
        };
        field_item(label, bool_str(value, lang), focused, false)
    }
}

pub(super) fn render(frame: &mut Frame, app: &App, area: Rect) {
    render_items(
        frame,
        &GitItem::ALL,
        app.git_focus,
        app.lang.text(" Git Settings ", " Git 設定 "),
        area,
        |item, focused| item.row(app, focused),
    );
}
