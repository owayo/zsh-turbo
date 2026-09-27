#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// 端末制御文字を除去し、zsh プロンプト中の `%` を `%%` にエスケープする。
fn escape_prompt(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '%' => escaped.push_str("%%"),
            ch if ch.is_control() => escaped.push('\u{fffd}'),
            ch => escaped.push(ch),
        }
    }
    escaped
}

/// 端末へ渡す文字列から制御文字を除去する。
fn sanitize_terminal_text(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { '\u{fffd}' } else { ch })
        .collect()
}

/// zsh プロンプト用エスケープ付きカラーテキスト (%{ %})
pub fn colored(text: &str, color: Color) -> String {
    let escaped = escape_prompt(text);
    format!("%{{\x1b[{}m%}}{escaped}%{{\x1b[0m%}}", color.fg_code())
}

/// zsh プロンプト用太字カラーテキスト
pub fn colored_bold(text: &str, color: Color) -> String {
    let escaped = escape_prompt(text);
    format!("%{{\x1b[1;{}m%}}{escaped}%{{\x1b[0m%}}", color.fg_code())
}

/// zsh プロンプト用前景色+背景色付きテキスト
pub fn colored_bg(text: &str, fg: Color, bg: Color) -> String {
    let escaped = escape_prompt(text);
    format!(
        "%{{\x1b[{};{}m%}}{escaped}%{{\x1b[0m%}}",
        fg.fg_code(),
        bg.bg_code()
    )
}

/// zsh プロンプト用 Powerline セパレータ: prev_bg → next_bg 遷移
pub fn powerline_sep(sep: &str, prev_bg: Color, next_bg: Option<Color>) -> String {
    match next_bg {
        Some(nb) => format!("%{{\x1b[{};{}m%}}{sep}", prev_bg.fg_code(), nb.bg_code()),
        None => format!("%{{\x1b[0m\x1b[{}m%}}{sep}%{{\x1b[0m%}}", prev_bg.fg_code()),
    }
}

/// 素の ANSI カラーテキスト（ターミナル出力用、zsh プロンプト以外）
pub fn plain_colored(text: &str, fg: Color) -> String {
    format!(
        "\x1b[{}m{}\x1b[0m",
        fg.fg_code(),
        sanitize_terminal_text(text)
    )
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
        s if s.starts_with('#') && s.len() == 7 && s.is_ascii() => {
            // ASCII 限定にすることで &s[..] のスライスがバイト境界でパニックしないようにする
            let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(255);
            let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(255);
            let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(255);
            Color::Rgb(r, g, b)
        }
        s => s.parse::<u8>().map(Color::Ansi256).unwrap_or(Color::White),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_color ────────────────────────────────────────────────

    #[test]
    fn parse_color_named_lowercase() {
        assert_eq!(parse_color("red"), Color::Red);
        assert_eq!(parse_color("green"), Color::Green);
        assert_eq!(parse_color("black"), Color::Black);
    }

    #[test]
    fn parse_color_case_insensitive() {
        assert_eq!(parse_color("BLUE"), Color::Blue);
        assert_eq!(parse_color("Magenta"), Color::Magenta);
        assert_eq!(parse_color("CYAN"), Color::Cyan);
    }

    #[test]
    fn parse_color_bright_and_aliases() {
        assert_eq!(parse_color("bright_black"), Color::BrightBlack);
        assert_eq!(parse_color("gray"), Color::BrightBlack);
        assert_eq!(parse_color("grey"), Color::BrightBlack);
        assert_eq!(parse_color("bright_red"), Color::BrightRed);
    }

    #[test]
    fn parse_color_hex() {
        assert_eq!(parse_color("#ff0000"), Color::Rgb(255, 0, 0));
        assert_eq!(parse_color("#00ff00"), Color::Rgb(0, 255, 0));
        assert_eq!(parse_color("#0a1b2c"), Color::Rgb(10, 27, 44));
    }

    #[test]
    fn parse_color_hex_ignores_non_ascii_seven_bytes() {
        // UTF-8 で 7 バイト長になるが ASCII 文字でないケース。
        // # + a + b + c + à(2byte) + e の 7 バイト構成は hex として無効で、
        // バイト境界外スライスでパニックしてはならない。
        // フォールバックとして White が返る。
        assert_eq!(parse_color("#abcàe"), Color::White);
    }

    #[test]
    fn parse_color_hex_unicode_does_not_panic() {
        // 6 文字目以降にマルチバイト文字を含むケース。
        // catch_unwind で確実にパニックしないことを検証する。
        let result = std::panic::catch_unwind(|| parse_color("#aabàc"));
        assert!(result.is_ok(), "parse_color が非 ASCII 入力でパニックした");
    }

    // ── % エスケープのテスト (zsh prompt expansion 防止) ─────────────

    #[test]
    fn colored_は_text内のpercentをエスケープする() {
        // ディレクトリ名や git ブランチ名に `%F{red}` などが含まれても
        // zsh prompt expansion で解釈されないよう `%%` に変換される
        let result = colored("foo%F{red}bar", Color::White);
        assert!(
            result.contains("%%F{red}"),
            "% が %% にエスケープされていない: {result}"
        );
        assert!(
            !result.contains("foo%F{red}bar"),
            "未エスケープの %F が残っている: {result}"
        );
    }

    #[test]
    fn colored_bg_は_text内のpercentをエスケープする() {
        let result = colored_bg("50%complete", Color::White, Color::Blue);
        assert!(
            result.contains("50%%complete"),
            "% が %% にエスケープされていない: {result}"
        );
    }

    #[test]
    fn colored_bold_は_text内のpercentをエスケープする() {
        let result = colored_bold("a%b", Color::Red);
        assert!(result.contains("a%%b"), "% が %% にエスケープされていない");
    }

    #[test]
    fn colored_の括弧構造は_text由来のものとは別() {
        // text に `%` を含めても、囲い用の `%{` `%}` の構造が維持される
        let result = colored("%", Color::Green);
        // 開始 `%{` と終了 `%}` は1組ずつ存在する
        assert!(result.starts_with("%{"));
        assert!(result.ends_with("%}"));
        // text の単独 `%` は `%%` に変換される
        assert!(result.contains("%%"));
    }

    #[test]
    fn parse_color_ansi256() {
        assert_eq!(parse_color("42"), Color::Ansi256(42));
        assert_eq!(parse_color("0"), Color::Ansi256(0));
        assert_eq!(parse_color("255"), Color::Ansi256(255));
    }

    #[test]
    fn parse_color_unknown_fallback() {
        assert_eq!(parse_color("nonexistent"), Color::White);
        assert_eq!(parse_color(""), Color::White);
    }

    #[test]
    fn parse_color_範囲外と不正値はpanicしない() {
        // ANSI256 の範囲外・負値は u8 パース失敗で White フォールバック（panic しない）。
        assert_eq!(parse_color("256"), Color::White);
        assert_eq!(parse_color("999"), Color::White);
        assert_eq!(parse_color("-1"), Color::White);
        // 7 バイトだが不正な hex 桁を含む場合、各成分は 255 フォールバック（panic しない）。
        assert_eq!(parse_color("#zz0000"), Color::Rgb(255, 0, 0));
    }

    // ── fg_code ────────────────────────────────────────────────────

    #[test]
    fn fg_code_basic_colors() {
        assert_eq!(Color::Red.fg_code(), "31");
        assert_eq!(Color::Blue.fg_code(), "34");
        assert_eq!(Color::BrightBlack.fg_code(), "90");
    }

    #[test]
    fn fg_code_ansi256() {
        assert_eq!(Color::Ansi256(42).fg_code(), "38;5;42");
        assert_eq!(Color::Ansi256(0).fg_code(), "38;5;0");
    }

    #[test]
    fn fg_code_rgb() {
        assert_eq!(Color::Rgb(1, 2, 3).fg_code(), "38;2;1;2;3");
        assert_eq!(Color::Rgb(255, 128, 0).fg_code(), "38;2;255;128;0");
    }

    // ── bg_code ────────────────────────────────────────────────────

    #[test]
    fn bg_code_basic_colors() {
        assert_eq!(Color::Red.bg_code(), "41");
        assert_eq!(Color::Blue.bg_code(), "44");
        assert_eq!(Color::BrightBlack.bg_code(), "100");
    }

    #[test]
    fn bg_code_ansi256() {
        assert_eq!(Color::Ansi256(42).bg_code(), "48;5;42");
    }

    #[test]
    fn bg_code_rgb() {
        assert_eq!(Color::Rgb(10, 20, 30).bg_code(), "48;2;10;20;30");
    }

    // ── colored ────────────────────────────────────────────────────

    #[test]
    fn colored_wraps_with_zsh_escapes() {
        let result = colored("hello", Color::Red);
        assert!(result.starts_with("%{"));
        assert!(result.ends_with("%}"));
        assert!(result.contains("31m"));
        assert!(result.contains("hello"));
        assert!(result.contains("\x1b[0m"));
    }

    #[test]
    fn colored_contains_fg_code() {
        let result = colored("test", Color::Ansi256(42));
        assert!(result.contains("38;5;42"));
    }

    // ── colored_bold ───────────────────────────────────────────────

    #[test]
    fn colored_bold_includes_bold_code() {
        let result = colored_bold("hello", Color::Green);
        assert!(result.contains("1;32m"));
        assert!(result.starts_with("%{"));
        assert!(result.ends_with("%}"));
        assert!(result.contains("hello"));
    }

    #[test]
    fn colored_bold_with_rgb() {
        let result = colored_bold("x", Color::Rgb(10, 20, 30));
        assert!(result.contains("1;38;2;10;20;30"));
    }

    // ── colored_bg ────────────────────────────────────────────────

    #[test]
    fn colored_bg_は前景と背景の両コードを含む() {
        let result = colored_bg("text", Color::White, Color::Blue);
        assert!(result.contains("37"), "前景色コードが含まれていない");
        assert!(result.contains("44"), "背景色コードが含まれていない");
        assert!(result.contains("text"));
        assert!(result.starts_with("%{"));
        assert!(result.contains("\x1b[0m"));
    }

    #[test]
    fn colored_bg_ansi256() {
        let result = colored_bg("x", Color::Ansi256(15), Color::Ansi256(236));
        assert!(result.contains("38;5;15"));
        assert!(result.contains("48;5;236"));
    }

    // ── powerline_sep ─────────────────────────────────────────────

    #[test]
    fn powerline_sep_次の背景色あり() {
        let result = powerline_sep("\u{e0b0}", Color::Blue, Some(Color::Green));
        assert!(result.contains("34"), "prev_bg の前景色コード");
        assert!(result.contains("42"), "next_bg の背景色コード");
        assert!(result.contains("\u{e0b0}"));
    }

    #[test]
    fn powerline_sep_次の背景色なし() {
        let result = powerline_sep("\u{e0b0}", Color::Red, None);
        assert!(result.contains("\x1b[0m"), "リセットが含まれていない");
        assert!(result.contains("31"), "prev_bg の前景色コード");
    }

    // ── plain_colored ──────────────────────────────────────────────

    #[test]
    fn plain_colored_はzshエスケープを含まない() {
        let result = plain_colored("hello", Color::Green);
        assert!(result.contains("\x1b[32m"));
        assert!(result.contains("\x1b[0m"));
        assert!(!result.contains("%{"), "zsh エスケープが混入している");
    }

    #[test]
    fn escape_prompt_は制御文字を置換する() {
        assert_eq!(
            escape_prompt("ok\x1b]0;owned\x07\nnext"),
            "ok�]0;owned��next"
        );
    }

    #[test]
    fn plain_colored_は制御文字を置換しpercentを維持する() {
        let result = plain_colored("100%\x1b]0;owned\x07", Color::Green);
        assert!(result.contains("100%�]0;owned�"));
        assert!(!result.contains("\x1b]0;owned\x07"));
    }
}
