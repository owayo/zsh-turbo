use crate::config::StyleConfig;
use crate::style::{Color, parse_color};

pub const RAINBOW_PALETTES: &[&str] = &[
    "default",
    "blue",
    "green",
    "blue_green",
    "purple",
    "cyan",
    "pink",
    "red",
    "orange",
    "ocean",
    "forest",
    "sunset",
    "pastel",
    "grayscale",
];

pub fn rainbow_palette(name: &str) -> &'static [(Color, Color)] {
    use Color::Ansi256 as C;
    match name {
        "blue" => &[
            (C(231), C(24)),
            (C(16), C(153)),
            (C(231), C(18)),
            (C(16), C(117)),
            (C(231), C(25)),
            (C(16), C(223)),
        ],
        "green" => &[
            (C(231), C(22)),
            (C(16), C(157)),
            (C(231), C(28)),
            (C(16), C(222)),
            (C(231), C(23)),
            (C(16), C(114)),
        ],
        "blue_green" => &[
            (C(231), C(24)),
            (C(16), C(157)),
            (C(231), C(22)),
            (C(16), C(153)),
            (C(16), C(29)),
            (C(16), C(222)),
        ],
        "purple" => &[
            (C(231), C(54)),
            (C(16), C(183)),
            (C(231), C(91)),
            (C(16), C(223)),
            (C(231), C(53)),
            (C(16), C(147)),
        ],
        "cyan" => &[
            (C(231), C(23)),
            (C(16), C(159)),
            (C(16), C(30)),
            (C(16), C(223)),
            (C(231), C(24)),
            (C(16), C(123)),
        ],
        "pink" => &[
            (C(231), C(89)),
            (C(16), C(218)),
            (C(231), C(125)),
            (C(16), C(224)),
            (C(231), C(53)),
            (C(16), C(210)),
        ],
        "red" => &[
            (C(231), C(88)),
            (C(16), C(217)),
            (C(231), C(124)),
            (C(16), C(223)),
            (C(231), C(52)),
            (C(16), C(181)),
        ],
        "orange" => &[
            (C(231), C(94)),
            (C(16), C(222)),
            (C(231), C(130)),
            (C(16), C(153)),
            (C(16), C(166)),
            (C(16), C(223)),
        ],
        "ocean" => &[
            (C(231), C(17)),
            (C(16), C(159)),
            (C(231), C(24)),
            (C(16), C(223)),
            (C(231), C(23)),
            (C(16), C(117)),
        ],
        "forest" => &[
            (C(231), C(22)),
            (C(16), C(150)),
            (C(231), C(58)),
            (C(16), C(223)),
            (C(231), C(23)),
            (C(16), C(157)),
        ],
        "sunset" => &[
            (C(231), C(54)),
            (C(16), C(215)),
            (C(231), C(88)),
            (C(16), C(222)),
            (C(231), C(90)),
            (C(16), C(217)),
        ],
        "pastel" => &[
            (C(16), C(153)),
            (C(16), C(223)),
            (C(16), C(183)),
            (C(16), C(157)),
            (C(16), C(218)),
            (C(16), C(159)),
        ],
        "grayscale" => &[
            (C(231), C(235)),
            (C(16), C(252)),
            (C(231), C(239)),
            (C(16), C(255)),
            (C(231), C(242)),
            (C(16), C(249)),
        ],
        _ => &[
            (C(231), C(24)),
            (C(16), C(221)),
            (C(231), C(28)),
            (C(16), C(176)),
            (C(231), C(88)),
            (C(16), C(117)),
        ],
    }
}

pub fn rainbow_colors(style: &StyleConfig, name: &str, index: usize) -> (Color, Color) {
    let palette = rainbow_palette(&style.rainbow_palette);
    let (mut fg, mut bg) = palette[index % palette.len()];
    if let Some(colors) = style.rainbow_overrides.get(name) {
        if let Some(value) = colors.fg.as_deref().filter(|s| !s.trim().is_empty()) {
            fg = parse_color(value);
        }
        if let Some(value) = colors.bg.as_deref().filter(|s| !s.trim().is_empty()) {
            bg = parse_color(value);
        }
    }
    (fg, bg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Color) -> f64 {
        let Color::Ansi256(index) = color else {
            panic!("プリセットには端末テーマに依存する基本色を使わない");
        };
        assert!(index >= 16);
        let rgb = if index >= 232 {
            [8 + 10 * (index - 232); 3]
        } else {
            let n = index - 16;
            let levels = [0, 95, 135, 175, 215, 255];
            [
                levels[(n / 36) as usize],
                levels[(n / 6 % 6) as usize],
                levels[(n % 6) as usize],
            ]
        };
        rgb.into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(v, weight)| {
                let s = f64::from(v) / 255.0;
                weight
                    * if s <= 0.04045 {
                        s / 12.92
                    } else {
                        ((s + 0.055) / 1.055).powf(2.4)
                    }
            })
            .sum()
    }

    #[test]
    fn 全配色の文字コントラストと標準配色の明暗差を確保する() {
        for name in RAINBOW_PALETTES {
            let colors = rainbow_palette(name);
            for (fg, bg) in colors {
                let (f, b) = (luminance(*fg), luminance(*bg));
                assert!(
                    (f.max(b) + 0.05) / (f.min(b) + 0.05) >= 4.5,
                    "{name}: {fg:?}/{bg:?}"
                );
            }
            if *name != "pastel" {
                for i in 0..colors.len() {
                    let a = luminance(colors[i].1);
                    let b = luminance(colors[(i + 1) % colors.len()].1);
                    assert!((a - b).abs() > 0.15, "{name}: {i}");
                }
            }
        }
    }

    #[test]
    fn 個別色は名前で保持し省略した成分だけセットから継承する() {
        let mut style = StyleConfig::default();
        style.rainbow_overrides.insert(
            "dir".into(),
            crate::config::SegmentColors {
                fg: Some("white".into()),
                bg: Some("#123456".into()),
            },
        );
        for index in [0, 1, 8, usize::MAX] {
            assert_eq!(
                rainbow_colors(&style, "dir", index),
                (Color::White, Color::Rgb(18, 52, 86))
            );
        }
        assert_eq!(
            rainbow_colors(&style, "git", 0),
            rainbow_palette("default")[0]
        );
        style.rainbow_overrides.get_mut("dir").unwrap().fg = None;
        assert_eq!(
            rainbow_colors(&style, "dir", 1).0,
            rainbow_palette("default")[1].0
        );
        style.rainbow_overrides.get_mut("dir").unwrap().bg = Some("  ".into());
        assert_eq!(
            rainbow_colors(&style, "dir", 1),
            rainbow_palette("default")[1]
        );
    }

    #[test]
    fn 全プリセットが異なる配色を持ち未知の名前は標準に戻る() {
        let mut names = std::collections::HashSet::new();
        for (i, name) in RAINBOW_PALETTES.iter().enumerate() {
            assert!(names.insert(name));
            let colors = rainbow_palette(name);
            assert_eq!(colors.len(), 6);
            assert!(colors.iter().all(|(fg, bg)| fg != bg));
            for previous in &RAINBOW_PALETTES[..i] {
                assert_ne!(colors, rainbow_palette(previous));
            }
        }
        assert_eq!(rainbow_palette("unknown"), rainbow_palette("default"));
        assert_eq!(rainbow_palette(""), rainbow_palette("default"));
        assert_eq!(
            rainbow_palette("default")[0],
            (Color::Ansi256(231), Color::Ansi256(24))
        );
    }
}
