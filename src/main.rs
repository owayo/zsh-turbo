use clap::{Parser, Subcommand};

mod config;
mod doctor;
mod highlight;
mod icons;
mod prompt;
mod style;
mod suggest;
mod wizard;

#[derive(Parser)]
#[command(
    name = "zsh-turbo",
    version,
    about = "High-performance zsh enhancement tool"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Output zsh initialization script
    Init,
    /// Render prompt string
    Prompt {
        /// Last command exit status
        #[arg(long, default_value = "0")]
        last_status: i32,
        /// Last command duration in milliseconds
        #[arg(long, default_value = "0")]
        duration_ms: u64,
        /// Prompt side: left or right
        #[arg(long, default_value = "left")]
        side: String,
        /// Number of background jobs
        #[arg(long, default_value = "0")]
        jobs: usize,
    },
    /// Get autosuggestion for current input
    Suggest {
        /// Current input prefix
        prefix: String,
        /// Path to zsh history file
        #[arg(long)]
        history_file: Option<String>,
        /// Search strategy: prefix, substring, fuzzy
        #[arg(long, default_value = "prefix")]
        strategy: String,
    },
    /// List history-based completions
    Complete {
        /// Current input prefix
        prefix: String,
        /// Path to zsh history file
        #[arg(long)]
        history_file: Option<String>,
        /// Maximum completions to return
        #[arg(long, default_value = "10")]
        max: usize,
    },
    /// Interactive configuration wizard
    Configure,
    /// Diagnose terminal, fonts, tools, and configuration
    Doctor,
    /// Syntax-highlight a command buffer (returns "start end style" lines)
    Highlight {
        /// The command buffer to highlight
        buffer: String,
        /// Newline-separated list of aliases (from zsh)
        #[arg(long, default_value = "")]
        aliases: String,
        /// Newline-separated list of functions (from zsh)
        #[arg(long, default_value = "")]
        functions: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            let cfg = config::load_config();
            print!("{}", include_str!("../shell/init.zsh"));
            // Emit config-driven shell variables
            if cfg.prompt.transient {
                println!("ZSH_TURBO_TRANSIENT=1");
            }
            println!("ZSH_TURBO_SUGGEST_STRATEGY={}", cfg.suggest.strategy);
            println!(
                "ZSH_TURBO_SUGGEST_HIGHLIGHT={}",
                cfg.suggest.highlight_color
            );
        }
        Commands::Prompt {
            last_status,
            duration_ms,
            side,
            jobs,
        } => {
            let config = config::load_config();
            let output = prompt::render_prompt(&config, last_status, duration_ms, &side, jobs);
            print!("{output}");
        }
        Commands::Suggest {
            prefix,
            history_file,
            strategy,
        } => {
            let strat = suggest::Strategy::from_str(&strategy);
            if let Some(suggestion) =
                suggest::get_suggestion(&prefix, history_file.as_deref(), &strat)
            {
                print!("{suggestion}");
            }
        }
        Commands::Complete {
            prefix,
            history_file,
            max,
        } => {
            let completions = suggest::get_completions(&prefix, history_file.as_deref(), max);
            for c in completions {
                println!("{c}");
            }
        }
        Commands::Configure => {
            if let Err(e) = wizard::run_wizard() {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Doctor => {
            doctor::run_doctor();
        }
        Commands::Highlight {
            buffer,
            aliases,
            functions,
        } => {
            let spans = highlight::highlight_buffer(&buffer, &aliases, &functions);
            print!("{}", highlight::format_spans(&spans));
        }
    }
}
