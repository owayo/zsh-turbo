<p align="center">
  <img src="docs/images/app.png" width="128" alt="zsh-turbo">
</p>

<h1 align="center">zsh-turbo</h1>

<p align="center">
  <strong>High-performance zsh enhancement tool written in Rust</strong>
</p>

<p align="center">
  <a href="https://github.com/owayo/zsh-turbo/actions/workflows/ci.yml">
    <img alt="CI" src="https://github.com/owayo/zsh-turbo/actions/workflows/ci.yml/badge.svg?branch=main">
  </a>
  <a href="https://github.com/owayo/zsh-turbo/releases/latest">
    <img alt="Version" src="https://img.shields.io/github/v/release/owayo/zsh-turbo">
  </a>
  <a href="LICENSE">
    <img alt="License" src="https://img.shields.io/github/license/owayo/zsh-turbo">
  </a>
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="README.ja.md">日本語</a>
</p>

---

## Overview

A single fast Rust binary that provides prompt rendering, input suggestions, syntax highlighting, and completion styling for zsh.

## Features

- **Prompt Styles** — Lean, Classic (powerline), Rainbow, Pure with font-level detection
- **20+ Segments** — dir, git, virtualenv, kubecontext, aws, terraform, docker, ssh, node/python/rust/go/ruby/java/php/swift/dotnet, package, jobs, os_icon, user, host
- **Custom Command Segments** — Define any segment declaratively in TOML
- **Parallel Execution** — All segments run concurrently via `std::thread::scope`
- **Async Autosuggestions** — History-based with prefix, substring, and fuzzy strategies
- **Transient Prompt** — Previous prompts simplify after command execution
- **Interactive Wizard** — Font detection + style selection
- **`zsh-turbo doctor`** — Diagnose terminal, fonts, tools, and config
- **Enhanced Completions** — Case-insensitive, grouped, cached, color-coded

## Requirements

- **OS**: macOS, Linux
- **Shell**: zsh 5.4+
- **Rust**: 1.85+ (for building from source, edition 2024)

## Installation

### From Source

```bash
cargo install --path .
```

### Binary Download

Download the latest release from [Releases](https://github.com/owayo/zsh-turbo/releases).

## Setup

```bash
# Interactive configuration wizard
zsh-turbo configure

# Add to ~/.zshrc
eval "$(zsh-turbo init)"
```

## Prompt Styles

| Style   | Description                                        |
| ------- | -------------------------------------------------- |
| Lean    | No background colors, minimal                      |
| Classic | Powerline separators with background-colored segments |
| Rainbow | Each segment cycles through a color palette        |
| Pure    | Minimal directory + git only                       |

## Configuration

Config file: `~/.config/zsh-turbo/config.toml`

```toml
[prompt]
prompt_style = "classic"  # lean, classic, rainbow, pure
font_level = "nerd"       # nerd, powerline, unicode, ascii
left_segments = ["os_icon", "dir", "git", "virtualenv"]
right_segments = ["duration", "status", "time"]
transient = true

[prompt.dir]
truncation_length = 3

[prompt.git]
show_ahead_behind = true
show_stash = true
show_status = true

# Custom command segments
[[prompt.custom]]
name = "wifi"
command = "networksetup -getairportnetwork en0 | cut -d: -f2 | xargs"
icon = "📶"
fg = "white"
bg = "24"
when = "always"  # or "env:VAR" or "file:path"

[style]
primary_color = "blue"
success_color = "green"
error_color = "red"
muted_color = "bright_black"
```

### Available Segments

| Segment          | Description                          |
| ---------------- | ------------------------------------ |
| `dir`            | Current directory with truncation    |
| `git`            | Branch, staged/modified/untracked    |
| `status`         | Non-zero exit code                   |
| `duration`       | Command execution time (>2s)         |
| `time`           | Current time (HH:MM:SS)             |
| `virtualenv`     | Python venv/conda environment        |
| `kubecontext`    | Kubernetes context                   |
| `aws`            | AWS profile                          |
| `terraform`      | Terraform workspace                  |
| `docker_context` | Docker context                       |
| `ssh`            | SSH session (user@host)              |
| `package`        | package.json name@version            |
| `jobs`           | Background job count                 |
| `os_icon`        | OS icon (macOS/Linux)                |
| `user`           | Username (root/SSH only)             |
| `host`           | Hostname (SSH only)                  |
| `node`           | Node.js version                      |
| `python`         | Python version                       |
| `rust`           | Rust version                         |
| `go`             | Go version                           |
| `ruby`           | Ruby version                         |
| `java`           | Java version                         |
| `php`            | PHP version                          |
| `swift`          | Swift version                        |
| `dotnet`         | .NET version                         |
| *(custom)*       | Any command defined in `[[prompt.custom]]` |

## Key Bindings

| Key         | Action                  |
| ----------- | ----------------------- |
| Right Arrow | Accept full suggestion  |
| Alt+F       | Accept one word         |
| Ctrl+Right  | Accept one word         |
| Up Arrow    | History search (prefix) |
| Down Arrow  | History search (prefix) |

## Commands

```bash
zsh-turbo configure                    # Interactive wizard
zsh-turbo init                         # Output zsh init script
zsh-turbo prompt --side left           # Render prompt
zsh-turbo suggest "query"              # Get suggestion (prefix)
zsh-turbo suggest "qry" --strategy fuzzy  # Fuzzy suggestion
zsh-turbo complete "prefix"            # List completions
zsh-turbo doctor                       # Diagnose environment
```

## Development

```bash
# Build
make build

# Run tests
make test

# Run clippy and format check
make check

# Build release
make release
```

## License

[MIT](LICENSE)
