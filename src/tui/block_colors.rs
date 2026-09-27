use super::*;

impl App {
    fn rainbow_names(&self) -> Vec<&str> {
        let mut seen = std::collections::HashSet::new();
        self.config
            .prompt
            .left_segments
            .iter()
            .map(String::as_str)
            .filter(|name| {
                ALL_SEGMENTS.contains(name)
                    || self.config.prompt.custom.iter().any(|c| c.name == *name)
            })
            .filter(|name| seen.insert(*name))
            .collect()
    }

    pub(super) fn rainbow_selected(&self) -> Option<&str> {
        let names = self.rainbow_names();
        names
            .get(self.rainbow_index.min(names.len().saturating_sub(1)))
            .copied()
    }

    pub(super) fn rainbow_select(&mut self, next: bool) {
        let len = self.rainbow_names().len();
        if len > 0 {
            let current = self.rainbow_index.min(len - 1);
            self.rainbow_index = if next {
                (current + 1) % len
            } else {
                (current + len - 1) % len
            };
        }
    }

    fn rainbow_position(&self, name: &str) -> usize {
        self.config
            .prompt
            .left_segments
            .iter()
            .filter(|n| {
                ALL_SEGMENTS.contains(&n.as_str())
                    || self.config.prompt.custom.iter().any(|c| c.name == **n)
            })
            .position(|n| n == name)
            .unwrap_or(0)
    }

    pub(super) fn rainbow_color_value(&self, name: &str, foreground: bool) -> String {
        if let Some(value) = self
            .config
            .style
            .rainbow_overrides
            .get(name)
            .and_then(|c| {
                if foreground {
                    c.fg.as_ref()
                } else {
                    c.bg.as_ref()
                }
            })
            .filter(|value| !value.trim().is_empty())
        {
            return value.clone();
        }
        let (fg, bg) = rainbow_colors(&self.config.style, name, self.rainbow_position(name));
        colors::color_value(if foreground { fg } else { bg })
    }

    pub(super) fn set_rainbow_color(&mut self, name: String, foreground: bool, value: String) {
        let colors = self
            .config
            .style
            .rainbow_overrides
            .entry(name.clone())
            .or_default();
        let field = if foreground {
            &mut colors.fg
        } else {
            &mut colors.bg
        };
        *field = if value.trim().is_empty() {
            None
        } else {
            Some(value)
        };
        if colors.fg.is_none() && colors.bg.is_none() {
            self.config.style.rainbow_overrides.remove(&name);
        }
    }

    pub(super) fn rainbow_reset(&mut self) {
        if let Some(name) = self.rainbow_selected().map(str::to_owned)
            && self.config.style.rainbow_overrides.remove(&name).is_some()
        {
            self.dirty = true;
            self.status_msg = Some(
                self.lang
                    .text(
                        "Restored palette colors for this block.",
                        "このブロックを配色セットの色に戻しました。",
                    )
                    .into(),
            );
        }
    }

    pub(super) fn rainbow_sample(&self, name: &str, edit: Option<(bool, &str)>) -> Style {
        let index = self.rainbow_position(name);
        let (mut fg, mut bg) = rainbow_colors(&self.config.style, name, index);
        if let Some((foreground, value)) = edit {
            let color = if value.trim().is_empty() {
                let palette = rainbow_palette(&self.config.style.rainbow_palette);
                let (fg, bg) = palette[index % palette.len()];
                if foreground { fg } else { bg }
            } else {
                style::parse_color(value)
            };
            if foreground {
                fg = color;
            } else {
                bg = color;
            }
        }
        Style::default()
            .fg(to_ratatui_color(fg))
            .bg(to_ratatui_color(bg))
    }
}

pub(super) fn fields(app: &App) -> Vec<ListItem<'static>> {
    let mut items = vec![field_item(
        app.lang
            .text("Rainbow block (Left/Right)", "Rainbow のブロック（←/→）"),
        app.rainbow_selected().unwrap_or(
            app.lang
                .text("No left blocks", "左側のブロックがありません"),
        ),
        app.style_focus == 5,
        false,
    )];
    let Some(name) = app.rainbow_selected() else {
        return items;
    };
    let sample = app.rainbow_sample(name, None);
    for (foreground, label, focus) in [
        (
            true,
            app.lang.text("Block foreground", "ブロックの文字色"),
            6,
        ),
        (
            false,
            app.lang.text("Block background", "ブロックの背景色"),
            7,
        ),
    ] {
        let value = app.rainbow_color_value(name, foreground);
        let customized = app
            .config
            .style
            .rainbow_overrides
            .get(name)
            .and_then(|c| {
                if foreground {
                    c.fg.as_ref()
                } else {
                    c.bg.as_ref()
                }
            })
            .is_some_and(|value| !value.trim().is_empty());
        let source = if customized {
            app.lang.text("custom", "個別")
        } else {
            app.lang.text("palette", "セット")
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("  {label}: {value} ({source})  "),
                field_style(app.style_focus == focus, false),
            ),
            Span::styled(app.lang.text(" Sample ", " 見本 "), sample),
            Span::raw(app.lang.text(" Enter:palette", " Enter:色選択")),
        ])));
    }
    items.push(field_item(
        app.lang
            .text("Reset block colors", "このブロックの個別色を解除"),
        "Enter",
        app.style_focus == 8,
        false,
    ));
    items
}
