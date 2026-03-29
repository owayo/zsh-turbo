#[derive(Debug, Clone, Copy)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Ansi256(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    pub fn fg_code(&self) -> String {
        match self {
            Color::Black => "30".into(),
            Color::Red => "31".into(),
            Color::Green => "32".into(),
            Color::Yellow => "33".into(),
            Color::Blue => "34".into(),
            Color::Magenta => "35".into(),
            Color::Cyan => "36".into(),
            Color::White => "37".into(),
            Color::BrightBlack => "90".into(),
            Color::BrightRed => "91".into(),
            Color::BrightGreen => "92".into(),
            Color::BrightYellow => "93".into(),
            Color::BrightBlue => "94".into(),
            Color::BrightMagenta => "95".into(),
            Color::BrightCyan => "96".into(),
            Color::BrightWhite => "97".into(),
            Color::Ansi256(n) => format!("38;5;{n}"),
            Color::Rgb(r, g, b) => format!("38;2;{r};{g};{b}"),
        }
    }

    pub fn bg_code(&self) -> String {
        match self {
            Color::Black => "40".into(),
            Color::Red => "41".into(),
            Color::Green => "42".into(),
            Color::Yellow => "43".into(),
            Color::Blue => "44".into(),
            Color::Magenta => "45".into(),
            Color::Cyan => "46".into(),
            Color::White => "47".into(),
            Color::BrightBlack => "100".into(),
            Color::BrightRed => "101".into(),
            Color::BrightGreen => "102".into(),
            Color::BrightYellow => "103".into(),
            Color::BrightBlue => "104".into(),
            Color::BrightMagenta => "105".into(),
            Color::BrightCyan => "106".into(),
            Color::BrightWhite => "107".into(),
            Color::Ansi256(n) => format!("48;5;{n}"),
            Color::Rgb(r, g, b) => format!("48;2;{r};{g};{b}"),
        }
    }
}

/// Colored text with zsh prompt escape sequences (%{ %})
pub fn colored(text: &str, color: Color) -> String {
    format!("%{{\x1b[{}m%}}{text}%{{\x1b[0m%}}", color.fg_code())
}

/// Bold colored text with zsh prompt escape sequences
pub fn colored_bold(text: &str, color: Color) -> String {
    format!("%{{\x1b[1;{}m%}}{text}%{{\x1b[0m%}}", color.fg_code())
}

/// Colored text with fg + bg for zsh prompt
pub fn colored_bg(text: &str, fg: Color, bg: Color) -> String {
    format!(
        "%{{\x1b[{};{}m%}}{text}%{{\x1b[0m%}}",
        fg.fg_code(),
        bg.bg_code()
    )
}

/// Powerline separator for zsh prompt: prev_bg → next_bg transition
pub fn powerline_sep(sep: &str, prev_bg: Color, next_bg: Option<Color>) -> String {
    match next_bg {
        Some(nb) => format!("%{{\x1b[{};{}m%}}{sep}", prev_bg.fg_code(), nb.bg_code()),
        None => format!("%{{\x1b[0m\x1b[{}m%}}{sep}%{{\x1b[0m%}}", prev_bg.fg_code()),
    }
}

/// Plain ANSI colored text (for terminal output, not zsh prompts)
pub fn plain_colored(text: &str, fg: Color) -> String {
    format!("\x1b[{}m{text}\x1b[0m", fg.fg_code())
}

/// Plain ANSI colored text with background
pub fn plain_colored_bg(text: &str, fg: Color, bg: Color) -> String {
    format!("\x1b[{};{}m{text}\x1b[0m", fg.fg_code(), bg.bg_code())
}

/// Plain powerline separator
pub fn plain_sep(sep: &str, prev_bg: Color, next_bg: Option<Color>) -> String {
    match next_bg {
        Some(nb) => format!("\x1b[{};{}m{sep}", prev_bg.fg_code(), nb.bg_code()),
        None => format!("\x1b[0m\x1b[{}m{sep}\x1b[0m", prev_bg.fg_code()),
    }
}

pub fn parse_color(name: &str) -> Color {
    match name.to_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "bright_black" | "gray" | "grey" => Color::BrightBlack,
        "bright_red" => Color::BrightRed,
        "bright_green" => Color::BrightGreen,
        "bright_yellow" => Color::BrightYellow,
        "bright_blue" => Color::BrightBlue,
        "bright_magenta" => Color::BrightMagenta,
        "bright_cyan" => Color::BrightCyan,
        "bright_white" => Color::BrightWhite,
        s if s.starts_with('#') && s.len() == 7 => {
            let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(255);
            let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(255);
            let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(255);
            Color::Rgb(r, g, b)
        }
        s => s.parse::<u8>().map(Color::Ansi256).unwrap_or(Color::White),
    }
}
