mod helpers;
mod segments;

pub use segments::Segment;

use crate::config::Config;
use crate::icons::{FontLevel, Icons};
use crate::style::{Color, colored, colored_bg, colored_bold, parse_color, powerline_sep};

/// Rainbow background palette
const RAINBOW_PALETTE: &[(Color, Color)] = &[
    (Color::White, Color::Ansi256(31)),  // Blue
    (Color::Black, Color::Ansi256(70)),  // Green
    (Color::Black, Color::Ansi256(178)), // Gold
    (Color::White, Color::Ansi256(166)), // Orange
    (Color::White, Color::Ansi256(5)),   // Magenta
    (Color::Black, Color::Ansi256(37)),  // Teal
];

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

    // Parallel segment execution via scoped threads, preserving order.
    let segs: Vec<Segment> = std::thread::scope(|s| {
        let handles: Vec<_> = segment_names
            .iter()
            .map(|name| {
                s.spawn(|| {
                    segments::make_segment(name, config, &icons, last_status, duration_ms, jobs)
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok().flatten())
            .collect()
    });

    if is_right {
        return render_lean(&segs);
    }

    let rendered = match config.prompt.prompt_style.as_str() {
        "classic" => render_powerline(&segs, &icons),
        "rainbow" => render_rainbow(&segs, &icons),
        _ => render_lean(&segs),
    };

    let prompt_char = if config.prompt.prompt_style == "pure" {
        if last_status == 0 {
            colored_bold("\u{276f}", Color::Magenta)
        } else {
            colored_bold("\u{276f}", parse_color(&config.style.error_color))
        }
    } else if last_status == 0 {
        colored_bold(icons.prompt_ok, parse_color(&config.style.success_color))
    } else {
        colored_bold(icons.prompt_err, parse_color(&config.style.error_color))
    };

    format!("{rendered}\n{prompt_char} ")
}

// ─── Renderers ──────────────────────────────────────────────────

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

fn render_rainbow(segments: &[Segment], icons: &Icons) -> String {
    if segments.is_empty() {
        return String::new();
    }
    let mut result = String::new();
    for (i, seg) in segments.iter().enumerate() {
        let (fg, bg) = RAINBOW_PALETTE[i % RAINBOW_PALETTE.len()];
        let icon_part = if seg.icon.is_empty() {
            String::new()
        } else {
            format!("{} ", seg.icon)
        };
        result.push_str(&colored_bg(&format!(" {icon_part}{} ", seg.text), fg, bg));
        let next_bg = segments.get(i + 1).map(|_| {
            let (_, nb) = RAINBOW_PALETTE[(i + 1) % RAINBOW_PALETTE.len()];
            nb
        });
        result.push_str(&powerline_sep(icons.separator_left, bg, next_bg));
    }
    result
}
