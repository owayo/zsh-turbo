pub(crate) mod helpers;
mod palette;
pub use palette::{RAINBOW_PALETTES, rainbow_colors, rainbow_palette};
mod segments;

pub use segments::{ALL_SEGMENT_NAMES, Segment};

use crate::config::Config;
use crate::icons::{FontLevel, Icons};
use crate::style::{colored, colored_bg, colored_bold, parse_color, powerline_sep};

pub fn render_prompt(
    config: &Config,
    last_status: i32,
    duration_ms: u64,
    side: &str,
    jobs: usize,
) -> String {
    let font_level = FontLevel::from_str(&config.prompt.font_level);
    let icons = Icons::for_level(font_level);
    let is_right = side == "right";

    let segment_names = if is_right {
        &config.prompt.right_segments
    } else {
        &config.prompt.left_segments
    };

    // スコープ付きスレッドによる並列セグメント実行（順序は維持）。
    let named_segments: Vec<(&str, Segment)> = std::thread::scope(|s| {
        let handles: Vec<_> = segment_names
            .iter()
            .map(|name| {
                s.spawn(|| {
                    segments::make_segment(name, config, &icons, last_status, duration_ms, jobs)
                        .map(|mut segment| {
                            match name.as_str() {
                                "dir" if !is_right && config.prompt.prompt_style == "classic" => {
                                    segment.bg = parse_color(&config.style.primary_color);
                                }
                                "dir" => segment.fg = parse_color(&config.style.primary_color),
                                "time" | "duration" => {
                                    segment.fg = parse_color(&config.style.muted_color)
                                }
                                _ => {}
                            }
                            (name.as_str(), segment)
                        })
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok().flatten())
            .collect()
    });

    let segs: Vec<_> = named_segments
        .into_iter()
        .enumerate()
        .map(|(index, (name, mut segment))| {
            if !is_right && config.prompt.prompt_style == "rainbow" {
                (segment.fg, segment.bg) = rainbow_colors(&config.style, name, index);
            }
            segment
        })
        .collect();

    if is_right {
        return render_lean(&segs);
    }

    let rendered = match config.prompt.prompt_style.as_str() {
        "classic" | "rainbow" => render_powerline(&segs, &icons),
        _ => render_lean(&segs),
    };

    let prompt_char = if config.prompt.prompt_style == "pure" {
        if last_status == 0 {
            colored_bold("\u{276f}", parse_color(&config.style.success_color))
        } else {
            colored_bold("\u{276f}", parse_color(&config.style.error_color))
        }
    } else if last_status == 0 {
        colored_bold(icons.prompt_ok, parse_color(&config.style.success_color))
    } else {
        colored_bold(icons.prompt_err, parse_color(&config.style.error_color))
    };

    let separator = if config.prompt.newline {
        "\n"
    } else if rendered.is_empty() {
        ""
    } else {
        " "
    };
    let padding = "\n".repeat(
        config
            .prompt
            .blank_lines
            .min(crate::config::MAX_PROMPT_BLANK_LINES),
    );
    format!("{padding}{rendered}{separator}{prompt_char} ")
}

// ─── レンダラー ────────────────────────────────────────────────

fn render_lean(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|s| {
            let icon_part = if s.icon.is_empty() {
                String::new()
            } else {
                format!("{} ", s.icon)
            };
            colored(&format!("{icon_part}{}", s.text), s.fg)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_powerline(segments: &[Segment], icons: &Icons) -> String {
    if segments.is_empty() {
        return String::new();
    }
    let mut result = String::new();
    for (i, seg) in segments.iter().enumerate() {
        let icon_part = if seg.icon.is_empty() {
            String::new()
        } else {
            format!("{} ", seg.icon)
        };
        result.push_str(&colored_bg(
            &format!(" {icon_part}{} ", seg.text),
            seg.fg,
            seg.bg,
        ));
        let next_bg = segments.get(i + 1).map(|s| s.bg);
        result.push_str(&powerline_sep(icons.separator_left, seg.bg, next_bg));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::FontLevel;
    use crate::style::Color;

    fn dummy_seg(text: &str, icon: &str) -> Segment {
        Segment {
            text: text.to_string(),
            icon: icon.to_string(),
            fg: Color::White,
            bg: Color::Blue,
        }
    }

    #[test]
    fn 設定の配色が実際の左右プロンプトとpureの記号に反映される() {
        let mut config = Config::default();
        config.prompt.left_segments = vec!["dir".into(), "time".into(), "duration".into()];
        config.prompt.right_segments = config.prompt.left_segments.clone();
        config.style.primary_color = "#123456".into();
        config.style.muted_color = "#234567".into();
        config.style.success_color = "#345678".into();
        config.style.error_color = "#456789".into();
        for prompt_style in ["lean", "pure", "classic", "rainbow"] {
            config.prompt.prompt_style = prompt_style.into();
            let right = render_prompt(&config, 0, 2500, "right", 0);
            assert!(right.contains("38;2;18;52;86"));
            assert!(right.contains("38;2;35;69;103"));
            let left = render_prompt(&config, 0, 2500, "left", 0);
            assert!(left.contains("1;38;2;52;86;120"));
            if prompt_style != "rainbow" {
                assert!(left.contains(if prompt_style == "classic" {
                    "48;2;18;52;86"
                } else {
                    "38;2;18;52;86"
                }));
                assert!(left.contains("38;2;35;69;103"));
            }
            let failure = render_prompt(&config, 1, 2500, "left", 0);
            assert!(failure.contains("1;38;2;69;103;137"));
        }
    }

    // ── render_lean ─────────────────────────────────────────────

    #[test]
    fn render_lean_空セグメントは空文字列() {
        assert_eq!(render_lean(&[]), "");
    }

    #[test]
    fn render_lean_単一セグメント() {
        let segs = vec![dummy_seg("test", "")];
        let result = render_lean(&segs);
        assert!(result.contains("test"));
    }

    #[test]
    fn render_lean_アイコン付きセグメント() {
        let segs = vec![dummy_seg("dir", "\u{f07c}")];
        let result = render_lean(&segs);
        assert!(result.contains("\u{f07c}"));
        assert!(result.contains("dir"));
    }

    #[test]
    fn render_lean_複数セグメントはスペース区切り() {
        let segs = vec![dummy_seg("a", ""), dummy_seg("b", "")];
        let result = render_lean(&segs);
        assert!(result.contains("a"));
        assert!(result.contains("b"));
    }

    // ── render_powerline ────────────────────────────────────────

    #[test]
    fn render_powerline_空セグメントは空文字列() {
        let icons = Icons::for_level(FontLevel::Nerd);
        assert_eq!(render_powerline(&[], &icons), "");
    }

    #[test]
    fn render_powerline_セパレータを含む() {
        let icons = Icons::for_level(FontLevel::Nerd);
        let segs = vec![dummy_seg("dir", ""), dummy_seg("git", "")];
        let result = render_powerline(&segs, &icons);
        assert!(
            result.contains(icons.separator_left),
            "Powerline セパレータが含まれていない"
        );
    }

    // ── render_rainbow ──────────────────────────────────────────

    #[test]
    fn render_rainbow_空セグメントは空文字列() {
        let icons = Icons::for_level(FontLevel::Nerd);
        assert_eq!(render_powerline(&[], &icons), "");
    }

    #[test]
    fn render_rainbow_パレット巡回() {
        let icons = Icons::for_level(FontLevel::Nerd);
        // パレット数を超えるセグメントでもパニックしない
        let segs: Vec<Segment> = (0..10)
            .map(|i| {
                let mut segment = dummy_seg(&format!("s{i}"), "");
                (segment.fg, segment.bg) = rainbow_colors(&Config::default().style, "", i);
                segment
            })
            .collect();
        let result = render_powerline(&segs, &icons);
        assert!(!result.is_empty());
        assert!(result.contains("s9"), "最後のセグメントが含まれていない");
    }
}
