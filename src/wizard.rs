use crate::config::{Config, PromptConfig, save_config};
use crate::icons::{FontLevel, Icons};
use crate::style::{Color, plain_colored, plain_colored_bg, plain_sep};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;
use std::io::{self, IsTerminal, Write};

pub fn run_wizard() -> io::Result<()> {
    if !io::stdin().is_terminal() {
        eprintln!("Error: wizard requires an interactive terminal");
        std::process::exit(1);
    }

    terminal::enable_raw_mode()?;
    let result = wizard_inner();
    terminal::disable_raw_mode()?;

    match result {
        Ok(config) => {
            save_config(&config)?;
            let path = crate::config::config_path();
            println!(
                "\r\n\x1b[1;32m\u{2714}\x1b[0m Configuration saved to {}\r\n",
                path.display()
            );
            println!("Add the following to your ~/.zshrc:\r\n");
            println!("  eval \"$(zsh-turbo init)\"\r\n");
        }
        Err(e) if e.kind() == io::ErrorKind::Interrupted => {
            println!("\r\n\r\nConfiguration cancelled.\r\n");
        }
        Err(e) => {
            eprintln!("\r\nError: {e}\r\n");
            std::process::exit(1);
        }
    }
    Ok(())
}

fn wizard_inner() -> io::Result<Config> {
    let mut out = io::stdout();

    // Welcome
    clear(&mut out)?;
    wln(&mut out, "")?;
    wln(
        &mut out,
        &format!(
            "  {}",
            plain_colored("zsh-turbo configurator", Color::BrightCyan)
        ),
    )?;
    wln(
        &mut out,
        &format!(
            "  {}",
            plain_colored(
                "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
                Color::BrightBlack
            )
        ),
    )?;
    wln(&mut out, "")?;
    wln(&mut out, "  Press any key to start, q to quit.")?;
    out.flush()?;

    match wait_key()? {
        KeyCode::Char('q') | KeyCode::Esc => {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        _ => {}
    }

    // Step 1: Font detection
    let font_level = step_font(&mut out)?;

    // Step 2: Prompt style
    let prompt_style = step_style(&mut out, font_level)?;

    // Step 3: Segments
    let show_time = step_yes_no(
        &mut out,
        "Show current time in prompt?",
        &[("y", "Yes (24-hour)"), ("n", "No")],
    )?;

    let transient = step_yes_no(
        &mut out,
        "Enable transient prompt?\n\r  (Previous prompts become minimal after execution)",
        &[("y", "Yes"), ("n", "No")],
    )?;

    // Step 4: Preview
    let icons = Icons::for_level(font_level);
    clear(&mut out)?;
    wln(&mut out, "")?;
    wln(
        &mut out,
        &format!("  {}", plain_colored("Preview", Color::BrightCyan)),
    )?;
    wln(&mut out, "")?;
    show_preview(&mut out, &prompt_style, &icons, font_level)?;
    wln(&mut out, "")?;
    wln(
        &mut out,
        &format!(
            "  {} Apply this configuration?",
            plain_colored("(y/n)", Color::BrightBlack)
        ),
    )?;
    out.flush()?;

    loop {
        match wait_key()? {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => break,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') | KeyCode::Esc => {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            _ => {}
        }
    }

    // Build config
    let mut right_segments = vec!["duration".to_string(), "status".to_string()];
    if show_time {
        right_segments.push("time".to_string());
    }

    let config = Config {
        prompt: PromptConfig {
            prompt_style,
            font_level: font_level.as_str().to_string(),
            left_segments: vec!["dir".into(), "git".into()],
            right_segments,
            transient,
            ..Default::default()
        },
        ..Default::default()
    };

    Ok(config)
}

// ─── Wizard Steps ───────────────────────────────────────────────

fn step_font(out: &mut io::Stdout) -> io::Result<FontLevel> {
    // Test 1: Unicode
    if !ask_char_test(
        out,
        "Does this look like a diamond (rotated square)?",
        "\u{25c6}",
        "If it looks like [?] or a box, choose No.",
    )? {
        return Ok(FontLevel::Ascii);
    }

    // Test 2: Powerline
    if !ask_char_test(
        out,
        "Does this look like a solid arrow/triangle pointing right?",
        "\u{e0b0}",
        "It should be a filled triangle, not a question mark.",
    )? {
        return Ok(FontLevel::Unicode);
    }

    // Test 3: Nerd Font icons
    if !ask_char_test(
        out,
        "Does this look like a folder icon?",
        "\u{f07c}",
        "It should be an open folder, not a box or question mark.",
    )? {
        return Ok(FontLevel::Powerline);
    }

    Ok(FontLevel::Nerd)
}

fn step_style(out: &mut io::Stdout, font_level: FontLevel) -> io::Result<String> {
    clear(out)?;
    let icons = Icons::for_level(font_level);
    wln(out, "")?;
    wln(
        out,
        &format!(
            "  {}",
            plain_colored("Select prompt style:", Color::BrightCyan)
        ),
    )?;
    wln(out, "")?;

    // 1. Lean
    wln(
        out,
        &format!(
            "  {} Lean - clean, no background colors",
            plain_colored("(1)", Color::BrightYellow)
        ),
    )?;
    wln(out, "")?;
    wln(
        out,
        &format!(
            "      {} {}",
            plain_colored("~/project", Color::Blue),
            plain_colored(&format!("{} main", icons.git_branch), Color::BrightBlack),
        ),
    )?;
    wln(
        out,
        &format!("      {} ", plain_colored("\u{276f}", Color::Green)),
    )?;
    wln(out, "")?;

    // 2. Classic (only if powerline font)
    if font_level.has_powerline() {
        wln(
            out,
            &format!(
                "  {} Classic - background segments with powerline separators",
                plain_colored("(2)", Color::BrightYellow)
            ),
        )?;
        wln(out, "")?;
        let dir_bg = Color::Ansi256(24);
        let git_bg = Color::Ansi256(70);
        write!(
            out,
            "      {}{}{}{}",
            plain_colored_bg(&format!(" {} ~/project ", icons.dir), Color::White, dir_bg),
            plain_sep(icons.separator_left, dir_bg, Some(git_bg)),
            plain_colored_bg(
                &format!(" {} main ", icons.git_branch),
                Color::Black,
                git_bg
            ),
            plain_sep(icons.separator_left, git_bg, None),
        )?;
        wln(out, "")?;
        wln(
            out,
            &format!("      {} ", plain_colored("\u{276f}", Color::Green)),
        )?;
        wln(out, "")?;

        // 3. Rainbow
        wln(
            out,
            &format!(
                "  {} Rainbow - each segment a different color",
                plain_colored("(3)", Color::BrightYellow)
            ),
        )?;
        wln(out, "")?;
        let rb = &[
            (Color::White, Color::Ansi256(31)),
            (Color::Black, Color::Ansi256(70)),
            (Color::Black, Color::Ansi256(178)),
        ];
        write!(
            out,
            "      {}{}{}{}{}{}",
            plain_colored_bg(&format!(" {} ~/project ", icons.dir), rb[0].0, rb[0].1),
            plain_sep(icons.separator_left, rb[0].1, Some(rb[1].1)),
            plain_colored_bg(&format!(" {} main ", icons.git_branch), rb[1].0, rb[1].1),
            plain_sep(icons.separator_left, rb[1].1, Some(rb[2].1)),
            plain_colored_bg(" 21:30 ", rb[2].0, rb[2].1),
            plain_sep(icons.separator_left, rb[2].1, None),
        )?;
        wln(out, "")?;
        wln(
            out,
            &format!("      {} ", plain_colored("\u{276f}", Color::Green)),
        )?;
        wln(out, "")?;
    }

    // 4. Pure
    wln(
        out,
        &format!(
            "  {} Pure - minimal, just directory and git",
            plain_colored("(4)", Color::BrightYellow)
        ),
    )?;
    wln(out, "")?;
    wln(
        out,
        &format!(
            "      {} {}",
            plain_colored("~/project", Color::Blue),
            plain_colored("main", Color::BrightBlack)
        ),
    )?;
    wln(
        out,
        &format!("      {} ", plain_colored("\u{276f}", Color::Magenta)),
    )?;
    wln(out, "")?;

    out.flush()?;

    loop {
        match wait_key()? {
            KeyCode::Char('1') => return Ok("lean".into()),
            KeyCode::Char('2') if font_level.has_powerline() => return Ok("classic".into()),
            KeyCode::Char('3') if font_level.has_powerline() => return Ok("rainbow".into()),
            KeyCode::Char('4') => return Ok("pure".into()),
            KeyCode::Char('q') | KeyCode::Esc => {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            _ => {}
        }
    }
}

fn step_yes_no(out: &mut io::Stdout, question: &str, options: &[(&str, &str)]) -> io::Result<bool> {
    clear(out)?;
    wln(out, "")?;
    wln(
        out,
        &format!("  {}", plain_colored(question, Color::BrightCyan)),
    )?;
    wln(out, "")?;
    for (key, label) in options {
        wln(
            out,
            &format!(
                "  {} {label}",
                plain_colored(&format!("({key})"), Color::BrightYellow)
            ),
        )?;
    }
    wln(out, "")?;
    out.flush()?;

    loop {
        match wait_key()? {
            KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
            KeyCode::Char('n') | KeyCode::Char('N') => return Ok(false),
            KeyCode::Char('q') | KeyCode::Esc => {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            _ => {}
        }
    }
}

fn show_preview(
    out: &mut io::Stdout,
    style: &str,
    icons: &Icons,
    font_level: FontLevel,
) -> io::Result<()> {
    match style {
        "classic" if font_level.has_powerline() => {
            let dir_bg = Color::Ansi256(24);
            let git_bg = Color::Ansi256(178); // dirty
            write!(
                out,
                "  {}{}{}{}",
                plain_colored_bg(
                    &format!(" {} ~/GitHub/my-app ", icons.dir),
                    Color::White,
                    dir_bg
                ),
                plain_sep(icons.separator_left, dir_bg, Some(git_bg)),
                plain_colored_bg(
                    &format!(" {} main !2 ?1 ", icons.git_branch),
                    Color::Black,
                    git_bg
                ),
                plain_sep(icons.separator_left, git_bg, None),
            )?;
            wln(out, "")?;
            wln(
                out,
                &format!("  {} ", plain_colored("\u{276f}", Color::Green)),
            )?;
        }
        "rainbow" if font_level.has_powerline() => {
            let rb = &[
                (Color::White, Color::Ansi256(31)),
                (Color::Black, Color::Ansi256(70)),
            ];
            write!(
                out,
                "  {}{}{}{}",
                plain_colored_bg(
                    &format!(" {} ~/GitHub/my-app ", icons.dir),
                    rb[0].0,
                    rb[0].1
                ),
                plain_sep(icons.separator_left, rb[0].1, Some(rb[1].1)),
                plain_colored_bg(&format!(" {} main ", icons.git_branch), rb[1].0, rb[1].1),
                plain_sep(icons.separator_left, rb[1].1, None),
            )?;
            wln(out, "")?;
            wln(
                out,
                &format!("  {} ", plain_colored("\u{276f}", Color::Green)),
            )?;
        }
        "pure" => {
            wln(
                out,
                &format!(
                    "  {} {}",
                    plain_colored("~/GitHub/my-app", Color::Blue),
                    plain_colored("main", Color::BrightBlack)
                ),
            )?;
            wln(
                out,
                &format!("  {} ", plain_colored("\u{276f}", Color::Magenta)),
            )?;
        }
        _ => {
            // lean
            wln(
                out,
                &format!(
                    "  {} {}",
                    plain_colored("~/GitHub/my-app", Color::Blue),
                    plain_colored(
                        &format!("{} main !2 ?1", icons.git_branch),
                        Color::BrightBlack
                    ),
                ),
            )?;
            wln(
                out,
                &format!("  {} ", plain_colored("\u{276f}", Color::Green)),
            )?;
        }
    }
    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────

fn ask_char_test(
    out: &mut io::Stdout,
    question: &str,
    test_char: &str,
    hint: &str,
) -> io::Result<bool> {
    clear(out)?;
    wln(out, "")?;
    wln(
        out,
        &format!("  {}", plain_colored(question, Color::BrightCyan)),
    )?;
    wln(out, "")?;
    wln(
        out,
        &format!("                    \x1b[1m{test_char}\x1b[0m"),
    )?;
    wln(out, "")?;
    wln(
        out,
        &format!("  {}", plain_colored(hint, Color::BrightBlack)),
    )?;
    wln(out, "")?;
    wln(
        out,
        &format!(
            "  {} Yes    {} No    {} Quit",
            plain_colored("(y)", Color::BrightYellow),
            plain_colored("(n)", Color::BrightYellow),
            plain_colored("(q)", Color::BrightBlack),
        ),
    )?;
    out.flush()?;

    loop {
        match wait_key()? {
            KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
            KeyCode::Char('n') | KeyCode::Char('N') => return Ok(false),
            KeyCode::Char('q') | KeyCode::Esc => {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            _ => {}
        }
    }
}

fn wait_key() -> io::Result<KeyCode> {
    loop {
        if let Event::Key(key) = event::read()? {
            // Handle Ctrl+C
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            return Ok(key.code);
        }
    }
}

fn clear(out: &mut io::Stdout) -> io::Result<()> {
    write!(out, "\x1b[2J\x1b[H")?;
    out.flush()
}

fn wln(out: &mut io::Stdout, text: &str) -> io::Result<()> {
    write!(out, "{text}\r\n")
}
