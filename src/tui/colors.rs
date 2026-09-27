use super::*;

pub(super) struct ColorPicker {
    selected: Option<u8>,
    columns: u16,
}

const NAMED_COLORS: &[&str] = &[
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright_black",
    "bright_red",
    "bright_green",
    "bright_yellow",
    "bright_blue",
    "bright_magenta",
    "bright_cyan",
    "bright_white",
];

pub(super) fn color_value(color: AppColor) -> String {
    match color {
        AppColor::Ansi256(n) => n.to_string(),
        AppColor::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => NAMED_COLORS
            .iter()
            .find(|name| style::parse_color(name) == color)
            .unwrap()
            .to_string(),
    }
}

impl ColorPicker {
    pub(super) fn new(value: &str, target: &EditTarget) -> Self {
        let color = if *target == EditTarget::HighlightColor {
            let normalized = crate::suggest_highlight_style(value);
            normalized
                .split(',')
                .filter_map(|part| part.trim().strip_prefix("fg="))
                .next_back()
                .unwrap_or("")
                .to_owned()
        } else {
            value.to_owned()
        };
        let lower = color.to_ascii_lowercase();
        let selected = lower
            .parse::<u8>()
            .ok()
            .or_else(|| {
                NAMED_COLORS
                    .iter()
                    .position(|name| *name == lower)
                    .map(|i| i as u8)
            })
            .or_else(|| matches!(lower.as_str(), "gray" | "grey").then_some(8));
        Self {
            selected,
            columns: 16,
        }
    }
}

fn selected_value(original: &str, target: &EditTarget, index: u8) -> String {
    if *target != EditTarget::HighlightColor {
        return index.to_string();
    }
    let normalized = crate::suggest_highlight_style(original);
    let mut parts: Vec<_> = normalized
        .split(',')
        .filter(|part| !part.trim().starts_with("fg="))
        .map(str::to_owned)
        .collect();
    parts.push(format!("fg={index}"));
    parts.join(",")
}

fn sample_style(value: &str, target: &EditTarget) -> Style {
    if *target == EditTarget::HighlightColor {
        let mut sample = Style::default();
        for part in crate::suggest_highlight_style(value)
            .split(',')
            .map(str::trim)
        {
            if let Some(color) = part.strip_prefix("fg=") {
                sample = sample.fg(to_ratatui_color(style::parse_color(color)));
            } else if let Some(color) = part.strip_prefix("bg=") {
                sample = sample.bg(to_ratatui_color(style::parse_color(color)));
            } else {
                sample = match part {
                    "bold" => sample.bold(),
                    "underline" => sample.underlined(),
                    "standout" => sample.reversed(),
                    "none" => Style::default(),
                    _ => sample,
                };
            }
        }
        sample
    } else if matches!(target, EditTarget::Custom(_, CustomField::Bg)) {
        Style::default().bg(to_ratatui_color(style::parse_color(value)))
    } else {
        Style::default().fg(to_ratatui_color(style::parse_color(value)))
    }
}

pub(super) fn color_field_item(
    lang: Lang,
    label: &str,
    value: &str,
    focused: bool,
    target: &EditTarget,
) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(format!("  {label}: {value}  "), field_style(focused, false)),
        Span::styled(lang.text(" Sample ", " 見本 "), sample_style(value, target)),
        Span::raw(lang.text("  Enter:palette", "  Enter:色選択")),
    ]))
}

pub(super) fn handle_picker(app: &mut App, key: KeyEvent) {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return;
    }
    match key.code {
        KeyCode::Esc => {
            app.color_picker = None;
            app.cancel_edit();
        }
        KeyCode::Enter => {
            app.color_picker = None;
            app.confirm_edit();
        }
        KeyCode::Char('e') => {
            app.color_picker = None;
            app.edit_cursor = app.edit_buffer.len();
        }
        code => {
            let picker = app.color_picker.as_mut().unwrap();
            let current = i32::from(picker.selected.unwrap_or(0));
            let step = i32::from(picker.columns);
            let next = match code {
                KeyCode::Left | KeyCode::Char('h') => current - 1,
                KeyCode::Right | KeyCode::Char('l') => current + 1,
                KeyCode::Up | KeyCode::Char('k') => current - step,
                KeyCode::Down | KeyCode::Char('j') => current + step,
                KeyCode::Home => 0,
                KeyCode::End => 255,
                _ => return,
            }
            .clamp(0, 255) as u8;
            picker.selected = Some(next);
            app.edit_buffer = selected_value(&app.edit_buffer, app.editing.as_ref().unwrap(), next);
            app.edit_cursor = app.edit_buffer.len();
        }
    }
}

pub(super) fn render_picker(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    frame.render_widget(Clear, area);
    let target = app.editing.as_ref().unwrap();
    let block = Block::default().borders(Borders::ALL).title(format!(
        " {}: {} ",
        match target {
            EditTarget::RainbowFg(name) | EditTarget::RainbowBg(name) =>
                format!("{name}: {}", target.label(app.lang)),
            _ => target.label(app.lang).to_string(),
        },
        app.lang.text("256 colors", "256 色")
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let help_height = if inner.height >= 6 {
        if inner.width < 70 { 5 } else { 3 }
    } else {
        inner.height.saturating_sub(1).min(1)
    };
    let parts =
        Layout::vertical([Constraint::Length(help_height), Constraint::Min(0)]).split(inner);
    let mut sample = sample_style(&app.edit_buffer, target);
    if let EditTarget::RainbowFg(name) | EditTarget::RainbowBg(name) = target {
        sample = app.rainbow_sample(
            name,
            Some((matches!(target, EditTarget::RainbowFg(_)), &app.edit_buffer)),
        );
    }
    if let EditTarget::Custom(index, field) = target {
        let custom = &app.config.prompt.custom[*index];
        sample = match field {
            CustomField::Fg => sample.bg(to_ratatui_color(style::parse_color(&custom.bg))),
            CustomField::Bg => sample.fg(to_ratatui_color(style::parse_color(&custom.fg))),
            _ => sample,
        };
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::raw(format!(
                    "{}: {}  ",
                    app.lang.text("Value", "値"),
                    app.edit_buffer
                )),
                Span::styled(app.lang.text(" Sample text ", " サンプル "), sample),
            ]),
            Line::from(app.lang.text(
                "e: type a color name, 0-255, #RRGGBB or highlight attributes",
                "e:色名・0–255・#RRGGBB・ハイライト属性を入力",
            )),
            Line::from(app.lang.text(
                "Arrows: choose a swatch (scrolls)   Enter: apply   Esc: cancel",
                "矢印:色を選択（スクロール）  Enter:適用  Esc:取消",
            )),
        ])
        .wrap(ratatui::widgets::Wrap { trim: true }),
        parts[0],
    );
    let picker = app.color_picker.as_mut().unwrap();
    picker.columns = (inner.width / 5).clamp(1, 16);
    let columns = usize::from(picker.columns);
    let rows: Vec<_> = (0..256)
        .collect::<Vec<_>>()
        .chunks(columns)
        .map(|row| {
            let spans: Vec<_> = row
                .iter()
                .flat_map(|&index| {
                    let index = index as u8;
                    let selected = picker.selected == Some(index);
                    [
                        Span::raw(if selected { ">" } else { " " }),
                        Span::styled(
                            format!("{index:>3}"),
                            Style::default()
                                .bg(Color::Indexed(index))
                                .fg(contrast_color(index)),
                        ),
                        Span::raw(if selected { "<" } else { " " }),
                    ]
                })
                .collect();
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut state =
        ListState::default().with_selected(picker.selected.map(|i| usize::from(i) / columns));
    frame.render_stateful_widget(List::new(rows), parts[1], &mut state);
}

fn contrast_color(index: u8) -> Color {
    let light = if index < 16 {
        matches!(index, 2 | 3 | 6 | 7 | 10 | 11 | 14 | 15)
    } else if index >= 232 {
        index >= 244
    } else {
        let n = u32::from(index - 16);
        let component = |v| if v == 0 { 0 } else { 55 + 40 * v };
        299 * component(n / 36) + 587 * component(n / 6 % 6) + 114 * component(n % 6) >= 128_000
    };
    if light { Color::Black } else { Color::White }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn press(app: &mut App, code: KeyCode) {
        assert!(!handle_event(app, KeyEvent::from(code)));
    }

    #[test]
    fn 全色項目にパレットから色を設定できる() {
        for (tab, field, path) in [
            (4, 0, "/style/primary_color"),
            (4, 1, "/style/success_color"),
            (4, 2, "/style/error_color"),
            (4, 3, "/style/muted_color"),
            (3, 1, "/suggest/highlight_color"),
            (5, 4, "/prompt/custom/0/fg"),
            (5, 5, "/prompt/custom/0/bg"),
        ] {
            let mut app = App::new(Config::default(), Lang::En);
            app.custom_add();
            app.dirty = false;
            app.tab = tab;
            app.set_focus(field);
            press(&mut app, KeyCode::Enter);
            assert!(app.color_picker.is_some(), "{path}");
            press(&mut app, KeyCode::End);
            press(&mut app, KeyCode::Right);
            assert!(!app.dirty);
            press(&mut app, KeyCode::Enter);
            let serialized = toml::to_string(&app.config).unwrap();
            let loaded: Config = toml::from_str(&serialized).unwrap();
            let json = serde_json::to_value(loaded).unwrap();
            assert_eq!(
                json.pointer(path).unwrap().as_str().unwrap(),
                if tab == 3 { "fg=255" } else { "255" }
            );
            assert!(app.dirty);
        }
    }

    #[test]
    fn rgb色の開閉と取消しでは元の値を保持する() {
        let mut app = App::new(Config::default(), Lang::En);
        app.config.style.primary_color = "#123456".into();
        app.activate_edit(EditTarget::PrimaryColor);
        assert!(app.color_picker.as_ref().unwrap().selected.is_none());
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.config.style.primary_color, "#123456");
        app.dirty = false;
        app.activate_edit(EditTarget::PrimaryColor);
        press(&mut app, KeyCode::Home);
        press(&mut app, KeyCode::Left);
        assert_eq!(app.edit_buffer, "0");
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.config.style.primary_color, "#123456");
        assert!(!app.dirty);
        assert!(app.editing.is_none());
        assert!(app.color_picker.is_none());
    }

    #[test]
    fn パレットからrgb値の直接入力へ切り替えられる() {
        let mut app = App::new(Config::default(), Lang::En);
        app.activate_edit(EditTarget::ErrorColor);
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Home);
        for _ in 0..3 {
            press(&mut app, KeyCode::Delete);
        }
        for c in "#abcdef".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.config.style.error_color, "#abcdef");
    }

    #[test]
    fn サジェストの色選択は背景色と属性を保持し前景色を重複させない() {
        let mut app = App::new(Config::default(), Lang::En);
        app.config.suggest.highlight_color = "fg=8,bg=17,bold,fg=9,none,underline".into();
        app.activate_edit(EditTarget::HighlightColor);
        press(&mut app, KeyCode::Home);
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.config.suggest.highlight_color,
            "bg=17,bold,none,underline,fg=1"
        );
        let old_value = app.config.suggest.highlight_color.clone();
        app.activate_edit(EditTarget::HighlightColor);
        press(&mut app, KeyCode::End);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.config.suggest.highlight_color, old_value);
        assert_eq!(
            selected_value("bright_black", &EditTarget::HighlightColor, 10),
            "fg=10"
        );
    }

    #[test]
    fn 狭い端末でも選択色がスクロールして表示される() {
        let mut app = App::new(Config::default(), Lang::En);
        app.activate_edit(EditTarget::PrimaryColor);
        for (width, height) in [(80, 24), (40, 18), (80, 15), (40, 12), (20, 4)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            press(&mut app, KeyCode::End);
            terminal.draw(|f| ui(f, &mut app)).unwrap();
            assert!(format!("{:?}", terminal.backend().buffer()).contains(">255<"));
            let cols = app.color_picker.as_ref().unwrap().columns;
            press(&mut app, KeyCode::Up);
            assert_eq!(app.edit_buffer, (255 - cols).to_string());
            press(&mut app, KeyCode::Home);
            terminal.draw(|f| ui(f, &mut app)).unwrap();
            assert!(format!("{:?}", terminal.backend().buffer()).contains(">  0<"));
        }
        let mut terminal = Terminal::new(TestBackend::new(1, 1)).unwrap();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn 色選択中も全ブロックと境界色が更新され取消しで元に戻る() {
        for newline in [false, true] {
            let mut app = App::new(Config::default(), Lang::Ja);
            app.config.prompt.prompt_style = "rainbow".into();
            app.config.prompt.font_level = "ascii".into();
            app.config.prompt.newline = newline;
            app.config.prompt.left_segments = ["os_icon", "dir", "git", "virtualenv"]
                .map(str::to_owned)
                .to_vec();
            app.config.prompt.right_segments.clear();
            let before = toml::to_string(&app.config).unwrap();
            let original = render_preview(&app.config, 98, None);
            app.activate_edit(EditTarget::RainbowBg("dir".into()));
            handle_picker(&mut app, KeyCode::End.into());
            let preview = render_preview(
                &app.config,
                98,
                Some((app.editing.as_ref().unwrap(), &app.edit_buffer)),
            );
            assert_eq!(
                preview.lines[0].spans[1].style.bg,
                Some(Color::Indexed(255))
            );
            assert_eq!(
                preview.lines[0].spans[2].style.bg,
                Some(Color::Indexed(255))
            );
            assert_eq!(
                preview.lines[0].spans[3].style.fg,
                Some(Color::Indexed(255))
            );
            assert_eq!(preview.lines[0].spans[4], original.lines[0].spans[4]);
            for (width, height) in [(100, 24), (40, 24)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|f| ui(f, &mut app)).unwrap();
                let rendered = format!("{:?}", terminal.backend().buffer());
                for text in ["~/project", "main", "[virtualenv]", ">255<"] {
                    assert!(rendered.contains(text), "{width}: {text}: {rendered}");
                }
            }
            handle_picker(&mut app, KeyCode::Char('e').into());
            app.edit_buffer = "#123456".into();
            app.edit_cursor = app.edit_buffer.len();
            let preview = render_preview(
                &app.config,
                98,
                Some((app.editing.as_ref().unwrap(), &app.edit_buffer)),
            );
            assert_eq!(
                preview.lines[0].spans[2].style.bg,
                Some(Color::Rgb(18, 52, 86))
            );
            let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
            terminal.draw(|f| ui(f, &mut app)).unwrap();
            assert!(
                terminal
                    .backend()
                    .buffer()
                    .content()
                    .iter()
                    .any(|cell| cell.symbol() == "/" && cell.bg == Color::Rgb(18, 52, 86))
            );
            assert_eq!(toml::to_string(&app.config).unwrap(), before);
            assert!(!app.dirty);
            assert!(!handle_event(&mut app, KeyCode::Esc.into()));
            assert_eq!(render_preview(&app.config, 98, None), original);
            assert_eq!(toml::to_string(&app.config).unwrap(), before);
        }
    }

    #[test]
    fn 空欄は保存済み個別色ではなくセットの色を試せる() {
        let mut app = App::new(Config::default(), Lang::En);
        app.config.prompt.prompt_style = "rainbow".into();
        app.config.prompt.left_segments = vec!["dir".into()];
        app.set_rainbow_color("dir".into(), false, "88".into());
        let preview = render_preview(
            &app.config,
            80,
            Some((&EditTarget::RainbowBg("dir".into()), "")),
        );
        assert_eq!(
            preview.lines[0].spans[0].style.bg,
            Some(to_ratatui_color(rainbow_palette("default")[0].1))
        );
        assert_eq!(
            app.config.style.rainbow_overrides["dir"].bg.as_deref(),
            Some("88")
        );
    }

    #[test]
    fn 長いプレビューは文字幅と色を保って折り返す() {
        let text = Text::from(Line::from(vec![
            Span::styled("日本語", Style::default().bg(Color::Blue)),
            Span::styled("e\u{301}abc", Style::default().fg(Color::Green)),
        ]));
        let wrapped = wrap_preview(text, 4);
        assert!(wrapped.lines.iter().all(|line| line.width() <= 4));
        assert_eq!(
            wrapped
                .lines
                .iter()
                .map(ToString::to_string)
                .collect::<String>(),
            "日本語e\u{301}abc"
        );
        assert_eq!(wrapped.lines[0].spans[0].style.bg, Some(Color::Blue));
        assert!(
            wrapped
                .lines
                .iter()
                .flat_map(|line| &line.spans)
                .any(
                    |span| span.content.contains("e\u{301}") && span.style.fg == Some(Color::Green)
                )
        );
    }
}
