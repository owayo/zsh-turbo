use super::*;

/// 配色タブの設定項目。表示順は `ALL` の並びで決まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StyleItem {
    PrimaryColor,
    SuccessColor,
    ErrorColor,
    MutedColor,
    Palette,
    /// 色を変える Rainbow のブロックの選択（←/→）
    Block,
    BlockFg,
    BlockBg,
    ResetBlock,
}

impl StyleItem {
    pub(super) const ALL: [Self; 9] = [
        Self::PrimaryColor,
        Self::SuccessColor,
        Self::ErrorColor,
        Self::MutedColor,
        Self::Palette,
        Self::Block,
        Self::BlockFg,
        Self::BlockBg,
        Self::ResetBlock,
    ];

    /// 選択できる Rainbow のブロックがあるときだけ表示する項目か
    fn needs_block(self) -> bool {
        match self {
            Self::BlockFg | Self::BlockBg | Self::ResetBlock => true,
            Self::PrimaryColor
            | Self::SuccessColor
            | Self::ErrorColor
            | Self::MutedColor
            | Self::Palette
            | Self::Block => false,
        }
    }

    pub(super) fn handle(self, app: &mut App, action: ItemAction) {
        match self {
            Self::PrimaryColor => app.edit_on_activate(action, EditTarget::PrimaryColor),
            Self::SuccessColor => app.edit_on_activate(action, EditTarget::SuccessColor),
            Self::ErrorColor => app.edit_on_activate(action, EditTarget::ErrorColor),
            Self::MutedColor => app.edit_on_activate(action, EditTarget::MutedColor),
            Self::Palette => {
                app.cycle_setting(action.cycle_direction(), RAINBOW_PALETTES, |config| {
                    &mut config.style.rainbow_palette
                })
            }
            // ブロックの選択は設定を変えずに巡回する
            Self::Block => app.rainbow_select(action.cycle_direction()),
            Self::BlockFg => edit_block_color(app, action, true),
            Self::BlockBg => edit_block_color(app, action, false),
            Self::ResetBlock => match action {
                ItemAction::Activate => app.rainbow_reset(),
                ItemAction::Adjust(_) => {}
            },
        }
    }

    fn row(self, app: &App, focused: bool) -> ListItem<'static> {
        let lang = app.lang;
        let style = &app.config.style;
        match self {
            Self::PrimaryColor => color_row(
                app,
                lang.text("Primary Color", "基本色"),
                &style.primary_color,
                focused,
                EditTarget::PrimaryColor,
            ),
            Self::SuccessColor => color_row(
                app,
                lang.text("Success Color", "成功の色"),
                &style.success_color,
                focused,
                EditTarget::SuccessColor,
            ),
            Self::ErrorColor => color_row(
                app,
                lang.text("Error Color", "エラーの色"),
                &style.error_color,
                focused,
                EditTarget::ErrorColor,
            ),
            Self::MutedColor => color_row(
                app,
                lang.text("Muted Color", "補助色"),
                &style.muted_color,
                focused,
                EditTarget::MutedColor,
            ),
            Self::Palette => field_item(
                lang.text("Rainbow palette", "Rainbow の配色"),
                palette_label(&style.rainbow_palette, lang),
                focused,
                false,
            ),
            Self::Block => block_colors::block_row(app, focused),
            Self::BlockFg => block_colors::block_color_row(app, true, focused),
            Self::BlockBg => block_colors::block_color_row(app, false, focused),
            Self::ResetBlock => field_item(
                lang.text("Reset block colors", "このブロックの個別色を解除"),
                "Enter",
                focused,
                false,
            ),
        }
    }
}

/// 選択中のブロックの文字色・背景色の色選択を決定で開く
fn edit_block_color(app: &mut App, action: ItemAction, foreground: bool) {
    match action {
        ItemAction::Activate => {
            if let Some(name) = app.rainbow_selected().map(str::to_owned) {
                app.activate_edit(if foreground {
                    EditTarget::RainbowFg(name)
                } else {
                    EditTarget::RainbowBg(name)
                });
            }
        }
        ItemAction::Adjust(_) => {}
    }
}

/// 表示中の項目。選択できるブロックが無いときは、ブロックの色の項目を除く。
pub(super) fn visible_items(app: &App) -> Vec<StyleItem> {
    let has_block = app.rainbow_selected().is_some();
    StyleItem::ALL
        .into_iter()
        .filter(|item| has_block || !item.needs_block())
        .collect()
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

pub(super) fn render(frame: &mut Frame, app: &App, area: Rect) {
    render_items(
        frame,
        &visible_items(app),
        app.style_focus,
        app.lang.text(" Style Settings ", " 配色設定 "),
        area,
        |item, focused| item.row(app, focused),
    );
}
