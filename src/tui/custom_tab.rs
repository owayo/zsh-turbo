use super::*;

/// カスタムタブの行。先頭で編集する定義を選び、その下に定義の項目が並ぶ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CustomItem {
    /// 編集する定義の選択（←/→）
    Selector,
    Field(CustomField),
}

impl CustomItem {
    pub(super) fn handle(self, app: &mut App, action: ItemAction) {
        match (self, action) {
            // 左右はどの行でも編集する定義を切り替える
            (Self::Selector | Self::Field(_), ItemAction::Adjust(direction)) => {
                app.custom_select(direction)
            }
            (Self::Selector, ItemAction::Activate) => {}
            (Self::Field(field), ItemAction::Activate) => {
                app.activate_edit(EditTarget::Custom(app.custom_index, field))
            }
        }
    }

    fn row(self, app: &App, custom: &mut CustomSegment, focused: bool) -> ListItem<'static> {
        match self {
            Self::Selector => field_item(
                app.lang.text("Selected (Left/Right)", "選択（←/→）"),
                &format!(
                    "{}/{}: {}",
                    app.custom_index + 1,
                    app.config.prompt.custom.len(),
                    custom.name
                ),
                focused,
                false,
            ),
            Self::Field(field) => {
                let target = EditTarget::Custom(app.custom_index, field);
                let value = field.value(custom).replace('\n', " ↵ ");
                if target.is_color() {
                    color_field_item(app.lang, field.label(app.lang), &value, focused, &target)
                } else {
                    field_item(field.label(app.lang), &value, focused, false)
                }
            }
        }
    }
}

/// 表示中の行。定義が無いときは選択の行だけにする。
pub(super) fn visible_items(app: &App) -> Vec<CustomItem> {
    let fields: &[CustomField] = if app.config.prompt.custom.is_empty() {
        &[]
    } else {
        &CustomField::ALL
    };
    std::iter::once(CustomItem::Selector)
        .chain(fields.iter().copied().map(CustomItem::Field))
        .collect()
}

pub(super) fn render(frame: &mut Frame, app: &App, area: Rect) {
    let Some(custom) = app.config.prompt.custom.get(app.custom_index) else {
        let message = app.lang.text(
            "No custom segments. Press n to create one.\nThen add it to Left/Right in the Segments tab.",
            "カスタムセグメントはありません。n キーで作成できます。\n作成後、「配置」タブで左側・右側に追加してください。",
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .title(app.lang.text(" Custom Segments ", " カスタムセグメント "));
        frame.render_widget(Paragraph::new(message).block(block), area);
        return;
    };
    let mut custom = custom.clone();
    render_items(
        frame,
        &visible_items(app),
        app.custom_focus,
        app.lang.text(
            " Custom: edit here, place in Segments ",
            " カスタム: 編集後に「配置」で追加 ",
        ),
        area,
        |item, focused| item.row(app, &mut custom, focused),
    );
}

/// カスタムタブ専用のキー
pub(super) fn handle_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('n') => app.custom_add(),
        KeyCode::Delete | KeyCode::Backspace if !app.config.prompt.custom.is_empty() => {
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
        _ => {}
    }
}

impl App {
    pub(super) fn custom_add(&mut self) {
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
        self.custom_focus = CustomItem::Field(CustomField::Name);
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

    pub(super) fn custom_remove(&mut self) {
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
            self.custom_focus = CustomItem::Selector;
        }
        self.refresh_segment_selection();
        self.dirty = true;
    }

    pub(super) fn custom_select(&mut self, direction: Direction) {
        let len = self.config.prompt.custom.len();
        if len > 0 {
            self.custom_index = cycle_index(self.custom_index, len, direction);
        }
    }

    /// 定義の改名に合わせて、左右の配置と個別配色が指す名前を新しい名前へ移す。
    pub(super) fn rename_custom(&mut self, index: usize, new_name: &str) {
        let old_name = self.config.prompt.custom[index].name.clone();
        // 組込み名や重複名を含む既存設定を修正するときは別の定義の配置を奪わない。
        if ALL_SEGMENTS.contains(&old_name.as_str())
            || self
                .config
                .prompt
                .custom
                .iter()
                .enumerate()
                .any(|(i, c)| i != index && c.name == old_name)
        {
            return;
        }
        if let Some(colors) = self.config.style.rainbow_overrides.remove(&old_name) {
            self.config
                .style
                .rainbow_overrides
                .insert(new_name.to_owned(), colors);
        }
        for name in self
            .config
            .prompt
            .left_segments
            .iter_mut()
            .chain(&mut self.config.prompt.right_segments)
        {
            if *name == old_name {
                *name = new_name.to_owned();
            }
        }
    }
}
