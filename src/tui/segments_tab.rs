use super::*;

/// 配置タブの列
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SegColumn {
    Left,
    Right,
    Available,
}

pub(super) fn available_segments(config: &Config) -> Vec<String> {
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

/// 左右キーは列の移動、決定は追加候補の列から追加する
pub(super) fn handle(app: &mut App, action: ItemAction) {
    match action {
        ItemAction::Adjust(Direction::Previous) => app.seg_column_left(),
        ItemAction::Adjust(Direction::Next) => app.seg_column_right(),
        ItemAction::Activate => app.seg_add(),
    }
}

/// 配置タブ専用のキー
pub(super) fn handle_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('L') => app.seg_add_to(SegColumn::Left),
        KeyCode::Char('R') => app.seg_add_to(SegColumn::Right),
        KeyCode::Delete | KeyCode::Backspace => app.seg_remove(),
        KeyCode::Char('u') => app.seg_reorder(true),
        KeyCode::Char('d') => app.seg_reorder(false),
        _ => {}
    }
}

impl App {
    /// 選択中の列の中で選択を上下に動かす
    pub(super) fn seg_focus_move(&mut self, direction: Direction) {
        let (state, len) = match self.seg_column {
            SegColumn::Left => (
                &mut self.seg_left_state,
                self.config.prompt.left_segments.len(),
            ),
            SegColumn::Right => (
                &mut self.seg_right_state,
                self.config.prompt.right_segments.len(),
            ),
            SegColumn::Available => (
                &mut self.seg_avail_state,
                available_segments(&self.config).len(),
            ),
        };
        match direction {
            Direction::Previous => list_prev(state),
            Direction::Next => list_next(state, len),
        }
    }

    pub(super) fn seg_column_left(&mut self) {
        self.seg_column = match self.seg_column {
            SegColumn::Left => SegColumn::Left,
            SegColumn::Right => SegColumn::Left,
            SegColumn::Available => SegColumn::Right,
        };
        if matches!(self.seg_column, SegColumn::Left | SegColumn::Right) {
            self.seg_last_side = self.seg_column;
        }
    }

    pub(super) fn seg_column_right(&mut self) {
        self.seg_column = match self.seg_column {
            SegColumn::Left => SegColumn::Right,
            SegColumn::Right => SegColumn::Available,
            SegColumn::Available => SegColumn::Available,
        };
        if matches!(self.seg_column, SegColumn::Left | SegColumn::Right) {
            self.seg_last_side = self.seg_column;
        }
    }

    pub(super) fn seg_add(&mut self) {
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

    pub(super) fn seg_add_to(&mut self, side: SegColumn) {
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

    pub(super) fn seg_remove(&mut self) {
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

    pub(super) fn seg_reorder(&mut self, up: bool) {
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

    /// 定義や配置が変わった後に、各列の選択を範囲内へ収める
    pub(super) fn refresh_segment_selection(&mut self) {
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
}

pub(super) fn render(frame: &mut Frame, app: &mut App, area: Rect) {
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
