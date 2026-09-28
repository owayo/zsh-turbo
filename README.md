<p align="center">
  <img src="docs/images/app.png" width="128" alt="zsh-turbo">
</p>

<h1 align="center">zsh-turbo</h1>

<p align="center">
  Zsh prompts, autosuggestions, syntax highlighting, and completions in one Rust binary
</p>

<!-- standard:badges:start -->
<h3 align="center">Supported Platforms</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Linux-FCC624?logo=linux&amp;logoColor=black" alt="Linux">
  <img src="https://img.shields.io/badge/macOS-000000?logo=apple&amp;logoColor=white" alt="macOS">
</p>

<p align="center">
  <a href="https://github.com/owayo/zsh-turbo/actions/workflows/ci.yml"><img src="https://github.com/owayo/zsh-turbo/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <a href="https://github.com/owayo/zsh-turbo/releases/latest"><img src="https://img.shields.io/github/v/release/owayo/zsh-turbo" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/owayo/zsh-turbo" alt="License"></a>
</p>

<p align="center">
  <a href="README.md">English</a> |
  <a href="README.ja.md">日本語</a>
</p>
<!-- standard:badges:end -->

---

A single Rust binary that provides prompt rendering, input suggestions, syntax highlighting, and completion styling for zsh.

![zsh-turbo prompt with command completions](docs/images/screenshot.png)

## Features

- **Prompt Styles**: Lean, Classic (powerline), Rainbow, Pure with four selectable font levels
- **30+ Segments**: dir, git, virtualenv, kubecontext, aws, gcloud, terraform, docker, direnv, nix_shell, ssh, node/python/rust/go/ruby/java/php/swift/dotnet, package, jobs, os_icon, user, host, load, battery, disk_usage, ram, vi_mode, proxy, cpu_arch, root_indicator, dir_writable, ip
- **Custom Command Segments**: Define any segment in the TUI or TOML
- **Parallel Execution**: All segments run concurrently via `std::thread::scope`
- **Async Autosuggestions**: History-based with prefix, substring, and fuzzy strategies
- **Syntax Highlighting**: Commands, assignments, redirections including fd duplication (`2>&1`), paths, glob patterns with shell escape handling, strings, variables including zsh special parameters, and newline command separators
- **PATH-Aware Highlighting**: Executable files in the current directory are recognized when `$PATH` contains an empty entry, as zsh does
- **Transient Prompt**: Previous prompts simplify after command execution
- **Semantic Prompt Markers**: OSC 133 prompt markers for iTerm2 3.7+ and Warp, including right prompt and secondary prompt (`PS2`) ranges
- **Robust zsh Integration**: Initialization, widgets, and `compinit` tolerate user shell options such as `SH_GLOB` and `NO_UNSET`, and CLI values derived from zsh buffers accept leading `-`
- **Safe Prompt Output**: Segment text escapes zsh prompt expansion and replaces terminal control characters
- **Interactive Configuration TUI**: Edit every setting in tabs (Prompt / Segments / Git / Suggest / Style / Custom / Shell / Completions), with live preview
- **`zsh-turbo install-font`**: Download MesloLGS NF into the OS user font directory
- **`zsh-turbo font`**: Choose a font and Ghostty/cmux ligature settings interactively
- **`zsh-turbo doctor`**: Diagnose terminal, fonts, tools, and config
- **Enhanced Completions**: Case-insensitive, grouped, cached, color-coded, and optional external completion directories

- **Managed CLI Completions**: Register any CLI in the Completions tab using help, a native generator, or a completion file. Select a registration and press Backspace (⌫) or `d` twice to remove it. Binary changes trigger background cache rebuilds; Tab uses the cache. See [configuration](docs/configuration.md#cli-completion-registration).

## Requirements

- **OS**: macOS, Linux
- **Shell**: zsh 5.4+
- **Rust**: 1.98+ (for building from source, edition 2024; the tested toolchain is pinned in `mise.toml`)

## Terminal Font Setup

The default `unicode` font level and `ascii` do not need a Nerd Font. The `powerline` and `nerd` levels use additional glyphs; select a level under **Prompt → Font level** in `zsh-turbo configure`. Run `zsh-turbo font` (also available from **Shell → Terminal Font Setup**) to choose a font. It offers the terminal default, MesloLGS NF, and installed font families. MesloLGS NF is a reliable choice for Nerd Font icons, but it does not provide programming ligatures. A font with ligatures is a better choice if joined `!=`, `=>`, and `->` glyphs matter to you. `zsh-turbo install-font` installs MesloLGS NF without changing your terminal's selected font.

| Terminal | Font setting for `powerline` or `nerd` |
|---|---|
| [Ghostty 1.2+](https://ghostty.org/docs/config) | Built-in Nerd Font symbols usually suffice. The wizard can set the font and `font-feature = +calt` / `font-feature = +liga` in a chosen Ghostty config file. Reload the configuration afterward. |
| [cmux](https://github.com/manaflow-ai/cmux/issues/3518) | The wizard targets `~/Library/Application Support/com.cmuxterm.app/config.ghostty` for cmux, separate from standalone Ghostty's settings. Restart cmux afterward. |
| [macOS Terminal](https://support.apple.com/guide/terminal/trmltxt/mac) | The wizard shows the selected family. In the active profile, open **Terminal → Settings → Profiles → Text → Font → Change** and select it. |
| [iTerm2](https://iterm2.com/documentation-preferences-profiles-text.html) | The wizard shows the selected family. In the active profile, open **Settings → Profiles → Text** and set **Font**. If **Use Non-ASCII Font** is enabled, set that font too. |

For Ghostty and cmux, the wizard offers default, enabled (`+calt`, `+liga`), and disabled (`-calt`, `-liga`) ligatures. It previews a managed block before writing, preserves other settings, and creates a backup when replacing an existing file. A font must support these features for the flags to affect its appearance.

Open a new terminal window after changing its font. If icons still appear as `?` or boxes, run `zsh-turbo doctor` to inspect sample glyphs.

## Installation

<!-- standard:install:start -->
### Homebrew (macOS/Linux)

```bash
brew install owayo/zsh-turbo/zsh-turbo
```

### Cargo

Requires Rust 1.98 or later.

```bash
cargo install --git https://github.com/owayo/zsh-turbo --locked
```

### From GitHub Releases

Download the archive for your platform from [Releases](https://github.com/owayo/zsh-turbo/releases/latest), extract it, and put `zsh-turbo` on your `PATH`. Each release also includes `SHA256SUMS` for checking the downloads.

| Platform | Archive |
|---|---|
| Linux (x86_64) | `zsh-turbo-x86_64-unknown-linux-gnu.tar.gz` |
| Linux (ARM64) | `zsh-turbo-aarch64-unknown-linux-gnu.tar.gz` |
| macOS (Intel) | `zsh-turbo-x86_64-apple-darwin.tar.gz` |
| macOS (Apple Silicon) | `zsh-turbo-aarch64-apple-darwin.tar.gz` |

On macOS, if you downloaded the archive with a browser, remove the quarantine attribute before running it: `xattr -d com.apple.quarantine zsh-turbo`.

### From Source

Requires [mise](https://mise.jdx.dev/) (the Rust toolchain is pinned in `mise.toml`).

```bash
git clone https://github.com/owayo/zsh-turbo.git
cd zsh-turbo
make install
```

`make install` installs to `/usr/local/bin`. Set `INSTALL_PATH` to change it (for example `make install INSTALL_PATH="$HOME/.local/bin"`).
<!-- standard:install:end -->



## Usage

```bash
# Interactive configuration TUI
zsh-turbo configure

# Add to ~/.zshrc
eval "$(zsh-turbo init)"
```

For example, to use completion definitions from [zsh-completions](https://github.com/zsh-users/zsh-completions), clone the repository once and add its `src` directory before initializing zsh-turbo:

```bash
git clone https://github.com/zsh-users/zsh-completions.git "$HOME/.zsh/zsh-completions"
```

Put these lines in `~/.zshrc`:

```bash
export ZSH_TURBO_COMPLETION_DIRS="$HOME/.zsh/zsh-completions/src"
eval "$(zsh-turbo init)"
```

Terminal shell integration is enabled automatically for iTerm2 3.7+ and Warp. It wraps prompts with OSC 133 markers so terminal features that inspect command ranges can exclude left prompt, right prompt, and secondary prompt cells. Set `ZSH_TURBO_TERM_SHELL_INTEGRATION=0` before initialization to disable it, or `1` to force it.

## Prompt Styles

| Style   | Description                                        |
| ------- | -------------------------------------------------- |
| Lean    | No background colors, minimal                      |
| Classic | Powerline separators with background-colored segments |
| Rainbow | Each segment cycles through a color palette        |
| Pure    | Minimal directory + git only                       |

In `zsh-turbo configure`, use **Style → Rainbow palette** to choose from 14 palettes, including blue, green, ocean, sunset, pastel, and grayscale. **Prompt → Command input** switches between the same line and the next line. The palettes now separate neighboring blocks with distinct hues and light/dark backgrounds. Use **Style → Rainbow block** to select OS, directory, Git, or a custom block, then set its foreground and background independently with the 256-color picker or RGB input. Individual colors stay attached to the block when reordered and survive palette changes. The full prompt preview remains visible above the color picker and updates immediately as you choose a color or type RGB values. **Prompt → Blank lines between prompts** sets the spacing after Enter from 0 to 10 blank lines. Press `S` to save and `Esc` to close; press `Esc` again to discard unsaved changes.

## Key Bindings

| Key         | Action                  |
| ----------- | ----------------------- |
| Right Arrow | Accept one path segment or the next word |
| Alt+F       | Accept one word         |
| Ctrl+Right  | Accept one word         |
| Up Arrow    | History search (prefix) |
| Down Arrow  | Select from a visible list (tasks, commands, files); otherwise search history (prefix) |
| Ctrl+R      | Ranked history menu (fuzzy search) |
| Tab         | Accept full suggestion; run standard completion when none is shown |

Starting with an empty input, Up browses all history and Down moves back toward newer entries and the empty input. With text entered, Up/Down retain the initial prefix while browsing matching entries. Editing the input starts a new search; asynchronous suggestions and highlighting do not reset it.

Suggestions are displayed and accepted only while the cursor is at the end of the buffer. Moving the cursor into existing text clears the ghost text; returning to the end restores the still-valid suggestion.
For example, if `ls -l` suggests `/path/to/hoge/fuga`, each Right Arrow press accepts `/path/`, then `/path/to/`, then `/path/to/hoge/`. If you have already typed `/path`, one press accepts `/to/` and leaves `/path/to/` in the input. Tab accepts the whole suggestion. You can assign the actions for Tab, Right Arrow, Alt+F, and Ctrl+Right separately in **Suggest** under `zsh-turbo configure`. Restart the shell after changing them.

Equally strong matches are ranked by frequency, then recency. `Ctrl+R` searches history using the current input, or the entire history when the input is empty. Choose with Up/Down or Tab, press Enter to insert, then Enter again to execute. Esc / Ctrl+C restores the original input and cursor. Set the menu limit in **Suggest → Max Suggestions**; the default is 10.

History suggestions prefer a prefix match over substring and fuzzy matches across the entire history file. Incomplete final entries and fragments of multiline commands are excluded, including fragments at the 64 KiB read boundary. Only the top candidates are selected, avoiding a full sort of all matches.

Project tasks in the current directory take priority over history suggestions. Typing `make` shows targets from the Makefile below the input, and `just` and `task` show their recipes and tasks the same way. Commands that run scripts through a subcommand show them after it: `npm run`, `pnpm run`, `bun run`, `yarn run`, `uv run`, `deno task`, and `mise run`. Typing only the command name, such as `uv` or `npm`, lists its subcommands with descriptions instead (options after `-`), read from its `--help` and cached. Further input filters the list by prefix. Press Down while the list is visible to select its first task, then use Up/Down (or Tab/Shift+Tab) to move through the tasks. The list stays in one column, with the selected task highlighted in cyan. Enter inserts the selected command without running it; Esc restores the original input, and Ctrl+C also closes the list until you edit the input. Task files are parsed without running the tools. Tab accepts the ghost suggestion before entering the list; **Suggest → Tab: Default** uses normal zsh completion. See [supported commands and task files](docs/configuration.md#project-task-completion).

Makefile targets with `make`:

![Makefile targets listed below a make prompt](docs/images/make.png)

Typing a path lists the entries of its directory below the input in the same way. `ls -l ~/Documents/` lists the files in `~/Documents`, and `ls -l ~/Documents/rep` narrows the list to names starting with `rep`. Directories come first and end with `/`. Press Down to select an entry, then Enter to insert it with any escaping it needs. Choosing a directory switches the list to its contents; a file is followed by a space. See [file list](docs/configuration.md#file-list) for the matching rules.

When no history suggestion is shown, Tab also completes subcommands and options, such as `zsh-turbo conf<Tab>` and `zsh-turbo install-font --fo<Tab>`. Completion is generated from the CLI definition. Other commands use standard zsh completion and your `fpath`. Clearing or accepting the input unregisters pending suggestion handlers and preserves normal command error output. Both sides of the prompt are rendered with one CLI invocation.

If error output has already disappeared in a shell running an older version, use `exec zsh 2>/dev/tty` to restore stderr to the terminal and restart.

## Commands

```bash
zsh-turbo configure                    # Interactive configuration TUI
zsh-turbo init                         # Output zsh init script
zsh-turbo prompt --side left           # Render prompt
zsh-turbo suggest "query"              # Get suggestion (prefix)
zsh-turbo suggest "qry" --strategy fuzzy  # Fuzzy suggestion
zsh-turbo complete "prefix"            # List completions
zsh-turbo highlight "FOO=bar echo 2>&1"  # Syntax highlight buffer
zsh-turbo install-font                 # Install MesloLGS NF (use --force to overwrite)
zsh-turbo font                         # Interactive terminal font setup
zsh-turbo doctor                       # Diagnose environment
```

The configuration TUI defaults to **Shell → Interface Language: Auto**. Auto checks the first non-empty `LC_ALL`, `LC_MESSAGES`, then `LANG`. Plain `C`/`POSIX` locales select English. Otherwise, the first supported language in the colon-separated `LANGUAGE` list takes precedence, followed by an explicit Japanese or other locale. `C.UTF-8`/`POSIX.UTF-8` and missing locales have no language preference: Auto selects Japanese for the local `Asia/Tokyo` or `Japan` time zone and English otherwise. `TZ` takes precedence over the system time zone. Select English or 日本語 in the TUI to save an explicit choice. Configuration keys and values stay the same in both languages.

### Nerd Font Installation

`zsh-turbo install-font` downloads MesloLGS NF into the OS user font directory using `curl`:

- macOS: `~/Library/Fonts/`
- Linux: `~/.local/share/fonts/` (and refreshes `fc-cache` automatically)

The preflight write check uses an exclusively created, unique probe file and never truncates or removes an existing file. Downloads also use independently reserved temporary files, so concurrent installers cannot remove or replace each other's partial downloads.

The font download is pinned to a reviewed upstream revision. Before downloading fonts, the installer saves the upstream copyright notice and full Apache-2.0 license as `MesloLGS NF License.txt` in the same directory. Running the command again also supplies this document when all fonts are already installed; license documents are excluded from the installed/skipped font counts.

After installation, configure the terminal if needed and open a new terminal window. Installing the font does not change a terminal's selected font. See [Terminal Font Setup](#terminal-font-setup) for each terminal's instructions.

## Configuration

Config file: `${XDG_CONFIG_HOME}/zsh-turbo/config.toml` when `XDG_CONFIG_HOME` is non-empty; otherwise `~/.config/zsh-turbo/config.toml`.

Configuration saves use atomic replacement. Concurrent saves use independent, exclusively created temporary files so one writer cannot overwrite another writer's in-progress data.

Run `zsh-turbo configure` to edit every setting below, or edit the config file directly. The TUI loads your existing configuration; press `S` to save it to the same file.

See the [configuration reference](docs/configuration.md) for all settings, TUI controls, colors, and segments.

## Development

<!-- standard:dev:start -->
Requires [mise](https://mise.jdx.dev/). Tool versions are pinned in `mise.toml`.

```bash
make setup   # Install the toolchain (mise) and dependencies
make ci      # Run the same checks as CI (no changes)
```

| Command | Description |
|---|---|
| `make setup` | Install the toolchain (mise) and dependencies |
| `make build` | Build a debug binary |
| `make release` | Build a release binary |
| `make run` | Run the debug binary (arguments via ARGS="...") |
| `make test` | Run the tests |
| `make lint` | Run clippy with warnings as errors |
| `make fmt` | Format the code (rewrites files) |
| `make fmt-check` | Check the formatting (no changes) |
| `make check` | Run fmt-check and lint (no changes) |
| `make ci` | Run the same checks as CI (no changes) |
| `make install` | Install the release binary to INSTALL_PATH (default /usr/local/bin) |
| `make uninstall` | Remove the binary from INSTALL_PATH |
| `make clean` | Remove build artifacts |

Run `make` to list every target. Releases are published from GitHub Actions (**Actions → Release → Run workflow**).
<!-- standard:dev:end -->

See the [development guide](docs/development.md) for distribution archives and license generation and verification.

## License

<!-- standard:license:start -->
[MIT](LICENSE)
<!-- standard:license:end -->

Dependencies retain their own licenses; their copyright notices, full license texts, and version-specific source links are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

The `option-ext` dependency is used unmodified under MPL-2.0. Its source is available under MPL-2.0 at the link in the notices; this does not change zsh-turbo's own MIT license. The separately downloaded MesloLGS NF font has its own [Apache-2.0 notice and terms](licenses/MesloLGS-NF.txt).
