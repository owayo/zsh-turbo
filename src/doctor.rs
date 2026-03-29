use crate::config;
use crate::style::{Color, plain_colored};
use std::process::Command;

pub fn run_doctor() {
    println!();
    println!("  {}", plain_colored("zsh-turbo doctor", Color::BrightCyan));
    println!(
        "  {}",
        plain_colored(
            "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
            Color::BrightBlack,
        )
    );
    println!();

    section("Terminal");
    check_env("TERM");
    check_env("TERM_PROGRAM");
    check_env_hint("COLORTERM", "truecolor support");

    section("Font Rendering");
    char_test("Powerline arrow", "\u{e0b0}");
    char_test("Nerd Font folder", "\u{f07c}");
    char_test("Unicode diamond", "\u{25c6}");
    char_test("Emoji", "\u{1f680}");

    section("Shell");
    check_env("SHELL");
    check_env("ZSH_VERSION");

    section("External Tools");
    check_tool("git", &["--version"]);
    check_tool("node", &["--version"]);
    check_tool("python3", &["--version"]);
    check_tool("rustc", &["--version"]);
    check_tool("go", &["version"]);
    check_tool("kubectl", &["version", "--client", "--short"]);
    check_tool("docker", &["--version"]);
    check_tool("terraform", &["--version"]);

    section("Configuration");
    let path = config::config_path();
    if path.exists() {
        ok(&format!("Config file: {}", path.display()));
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<config::Config>(&content) {
                Ok(cfg) => {
                    ok(&format!("  Style: {}", cfg.prompt.prompt_style));
                    ok(&format!("  Font:  {}", cfg.prompt.font_level));
                    ok(&format!(
                        "  Left:  [{}]",
                        cfg.prompt.left_segments.join(", ")
                    ));
                    ok(&format!(
                        "  Right: [{}]",
                        cfg.prompt.right_segments.join(", ")
                    ));
                    if !cfg.prompt.custom.is_empty() {
                        ok(&format!("  Custom segments: {}", cfg.prompt.custom.len()));
                    }
                }
                Err(e) => warn(&format!("  Parse error: {e}")),
            },
            Err(e) => warn(&format!("  Read error: {e}")),
        }
    } else {
        info(&format!(
            "No config file (using defaults): {}",
            path.display()
        ));
        info("  Run `zsh-turbo configure` to create one");
    }

    section("Additional Features");
    ok("Parallel segment execution (std::thread::scope)");
    ok("Custom command segments (TOML declarative)");
    ok("Fuzzy/substring suggestion strategies");
    ok("Environment diagnostics (this command)");

    println!();
}

fn section(name: &str) {
    println!(
        "\n  {} {}",
        plain_colored("\u{25cf}", Color::BrightBlue),
        plain_colored(name, Color::BrightWhite)
    );
}

fn ok(msg: &str) {
    println!("    {} {}", plain_colored("\u{2714}", Color::Green), msg);
}

fn warn(msg: &str) {
    println!("    {} {}", plain_colored("\u{26a0}", Color::Yellow), msg);
}

fn info(msg: &str) {
    println!(
        "    {} {}",
        plain_colored("\u{2139}\u{fe0e}", Color::BrightBlack),
        msg
    );
}

fn fail(msg: &str) {
    println!("    {} {}", plain_colored("\u{2718}", Color::Red), msg);
}

fn check_env(var: &str) {
    match std::env::var(var) {
        Ok(val) => ok(&format!("{var} = {val}")),
        Err(_) => info(&format!("{var} not set")),
    }
}

fn check_env_hint(var: &str, hint: &str) {
    match std::env::var(var) {
        Ok(val) => ok(&format!("{var} = {val} ({hint})")),
        Err(_) => info(&format!("{var} not set ({hint})")),
    }
}

fn check_tool(name: &str, args: &[&str]) {
    match Command::new(name)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => {
            let ver = String::from_utf8_lossy(&output.stdout);
            let ver = ver.trim().lines().next().unwrap_or("").trim();
            let short = if ver.len() > 60 { &ver[..60] } else { ver };
            ok(&format!("{name}: {short}"));
        }
        Ok(_) => fail(&format!("{name}: found but returned error")),
        Err(_) => info(&format!("{name}: not found")),
    }
}

fn char_test(label: &str, ch: &str) {
    println!(
        "    {} {label}: [ {ch} ] {}",
        plain_colored("\u{25b6}", Color::BrightBlack),
        plain_colored("(should display correctly)", Color::BrightBlack),
    );
}
