use super::*;

const SHELL_INTEGRATIONS: &[&str] = &["auto", "1", "0"];

/// シェルタブの設定項目。表示順は `ALL` の並びで決まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShellItem {
    CompletionDirs,
    TermIntegration,
    Language,
    FontSetup,
}

impl ShellItem {
    pub(super) const ALL: [Self; 4] = [
        Self::CompletionDirs,
        Self::TermIntegration,
        Self::Language,
        Self::FontSetup,
    ];

    pub(super) fn handle(self, app: &mut App, action: ItemAction) {
        match self {
            Self::CompletionDirs => app.edit_on_activate(action, EditTarget::CompletionDirs),
            Self::TermIntegration => {
                app.cycle_setting(action.cycle_direction(), SHELL_INTEGRATIONS, |config| {
                    &mut config.shell.term_shell_integration
                })
            }
            // 表示言語は設定と画面の言語を同時に切り替える
            Self::Language => {
                let language = app.config.ui.language;
                app.config.ui.language = match action.cycle_direction() {
                    Direction::Previous => language.prev(),
                    Direction::Next => language.next(),
                };
                app.lang = Lang::from_preference(app.config.ui.language);
                app.dirty = true;
            }
            Self::FontSetup => match action {
                ItemAction::Activate => launch_font_setup(app),
                ItemAction::Adjust(_) => {}
            },
        }
    }

    fn row(self, app: &App, focused: bool) -> ListItem<'static> {
        let lang = app.lang;
        let shell = &app.config.shell;
        match self {
            Self::CompletionDirs => field_item(
                lang.text(
                    "Completion Dirs (: separated)",
                    "補完ディレクトリ（: 区切り）",
                ),
                &shell.completion_dirs,
                focused,
                false,
            ),
            Self::TermIntegration => field_item(
                lang.text("Terminal Integration", "端末との連携"),
                &shell.term_shell_integration,
                focused,
                false,
            ),
            Self::Language => {
                let language = match app.config.ui.language {
                    UiLanguage::Auto => format!(
                        "Auto ({})",
                        if Lang::from_env() == Lang::Ja {
                            "日本語"
                        } else {
                            "English"
                        }
                    ),
                    UiLanguage::En => "English".into(),
                    UiLanguage::Ja => "日本語".into(),
                };
                field_item(
                    lang.text("Interface Language", "表示言語"),
                    &language,
                    focused,
                    false,
                )
            }
            Self::FontSetup => field_item(
                lang.text("Terminal Font Setup", "端末フォント設定"),
                "zsh-turbo font ↵",
                focused,
                false,
            ),
        }
    }
}

/// フォント設定は保存済みの設定で起動するため、未保存の変更があれば先に保存を促す
fn launch_font_setup(app: &mut App) {
    if app.dirty {
        app.status_msg = Some(
            app.lang
                .text(
                    "Save settings first (s).",
                    "先に s で設定を保存してください。",
                )
                .into(),
        );
    } else {
        app.launch_font_wizard = true;
    }
}

pub(super) fn render(frame: &mut Frame, app: &App, area: Rect) {
    render_items(
        frame,
        &ShellItem::ALL,
        app.shell_focus,
        app.lang.text(
            " Shell (auto / 1:on / 0:off) ",
            " シェル（auto:自動 / 1:有効 / 0:無効） ",
        ),
        area,
        |item, focused| item.row(app, focused),
    );
}
