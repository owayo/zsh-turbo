# Configuration

[Back to README](../README.md)

Config file: `${XDG_CONFIG_HOME}/zsh-turbo/config.toml` when `XDG_CONFIG_HOME` is non-empty; otherwise `~/.config/zsh-turbo/config.toml`.

Configuration saves use atomic replacement. Concurrent saves use independent, exclusively created temporary files so one writer cannot overwrite another writer's in-progress data.

Run `zsh-turbo configure` to edit every setting below, or edit the config file directly. The TUI loads your existing configuration; press `S` to save it to the same file.

| Tab | Settings |
| --- | --- |
| Prompt | Style, font level, transient prompt, home symbol, truncation length and symbol, IP interface (empty for automatic), command input position |
| Segments | Left/right placement and ordering of built-in and custom segments |
| Git | Status, ahead/behind, and stash visibility |
| Suggest | Search strategy, highlight color, completion limit, key actions |
| Style | Primary, success, error, and muted colors; Rainbow palette |
| Custom | Add/delete segments; edit name, command, icon, foreground, background, and condition |
| Shell | Completion directories, terminal integration, interface language (`auto`, `en`, `ja`), and font setup |
| Completions | Register, enable/disable, remove, and rebuild CLI completions |

Use `Tab` / `Shift+Tab` to switch tabs and Up/Down to select fields. `Enter` edits or toggles a field; Left/Right changes choices and numbers. Numbers also support direct entry with `Enter`. While editing, use Left/Right, `Home`, and `End` to move the cursor, `Enter` to confirm, and `Esc` to cancel.

Press `Esc` from the main screen to close. If there are unsaved changes, press `Esc` again to discard them, or another key to return. `q` no longer closes the TUI.

**Shell → Interface Language** is `auto` by default. Auto follows an explicit locale and uses the local time zone for `C.UTF-8` or missing locales; `Asia/Tokyo` and `Japan` select Japanese. Choose `English` or `日本語` to save a fixed language. The choice is stored as `[ui] language = "auto"`, `"en"`, or `"ja"`. **Shell → Terminal Font Setup** starts `zsh-turbo font` after leaving the TUI; save pending changes first. The wizard can edit Ghostty or cmux font and ligature settings, and shows profile instructions for Terminal.app and iTerm2.

The four Style colors, Suggest highlight color, and Custom foreground/background colors include swatches. Press `Enter` to open the 256-color palette, choose with arrows or `h/j/k/l`, confirm with `Enter`, or cancel with `Esc`. Press `e` for direct entry of a name such as `blue`, an index from `0` to `255`, or `#RRGGBB`. Suggest also accepts attributes such as `fg=8,bg=0,bold`; palette selection changes only the foreground color.

Primary controls directory text (the background on the left in Classic); Muted controls time and duration text; Success / Error control the successful/failed prompt symbol. Custom backgrounds apply on the left in Classic. Rainbow uses per-block overrides, then the selected palette on the left; the right prompt uses each segment's foreground color.

The Rainbow palette choices are `default`, `blue`, `green`, `blue_green`, `purple`, `cyan`, `pink`, `red`, `orange`, `ocean`, `forest`, `sunset`, `pastel`, and `grayscale`. Select **Style → Rainbow palette** with Left/Right or Enter. Each palette cycles through six foreground/background pairs. The palettes separate adjacent blocks using hue and lightness, with contrasting text colors. The default alternates dark blue, yellow, green, light purple, dark red, and light blue. Unknown names fall back to it.

In **Style → Rainbow block**, use Left/Right to choose a block placed on the left, then edit **Block foreground** and **Block background** with Enter. The full prompt stays visible above the color picker with the configured block order, colors, and separators. Arrow selections and RGB typing update this preview before confirmation; Enter applies the edit and Esc restores the previous colors. Blocks wrap when they exceed the preview width. The small sample also shows the foreground and background together. The selector includes built-in and custom blocks, even when their display condition is currently false. Individual colors override the palette for that named block and stay attached when blocks are reordered or hidden. Changing the palette preserves them. Choose **Reset block colors** to restore both palette colors; enter an empty value with `e` to restore just one component. These settings apply only to the left Rainbow prompt; status text and the success/error input symbol retain their meaning. Custom block renames also update their color settings.

Set **Prompt → Blank lines between prompts** to add 0–10 blank lines before each new prompt after Enter. Use Left/Right or enter a number with Enter. The default is 0. While this field is selected, the preview shows two prompts with the chosen gap. The `prompt.blank_lines` setting is independent of the command input position and applies to all styles, only on the left. Values above 10 in the config file render as 10. Changes take effect on the next prompt after saving; the same padding also appears at shell startup and after clearing the screen. Transient prompts preserve the spacing too. After upgrading, run `exec zsh` in existing shells to load the updated shell integration.

Set **Prompt → Command input** to **Same line** or **Next line**. This applies to all four styles. `prompt.newline = true` preserves the original two-line layout; `false` puts the input symbol after the left segments. The right prompt stays on the input line, and transient prompts remain compact. Saved palette and layout changes take effect at the next prompt.
blank_lines = 0         # Blank lines before each prompt (0–10)

In Custom, press `n` to add a definition, Left/Right to select one, and Backspace (⌫) twice to remove it and its placements. Renaming updates both prompt lists. Use `Alt+Enter` for newlines in commands. Select a custom segment in Available in the Segments tab, then press `L` to add it to the left or `R` to add it to the right. Use `L` / `R` on an existing placement to display it on both sides. Backspace removes a placement; `u` / `d` changes its order. The preview uses sample text and never executes custom commands.

Styles and segments take effect on the next prompt render after saving. Open a new zsh session to apply transient prompt, suggestion, and Shell settings. Configuration variables named `ZSH_TURBO_*` that are set before initialization take precedence over TOML; `ZSH_TURBO_IP_INTERFACE` also overrides `prompt.ip_interface`.

`suggest.max_suggestions` limits the Ctrl+R history menu and `zsh-turbo complete` (default 10; 0 disables candidates; CLI `--max` overrides it). Inline suggestions display the best prefix extension in a dim color. The history menu searches the full input, ranking prefix, substring, then fuzzy matches; equally strong matches use frequency, then recency. Up/Down or Tab selects a candidate, Enter inserts it, and a second Enter executes it. Esc / Ctrl+C restores the original input. Standard Tab completion remains available. `suggest` uses the configured strategy; `complete` defaults to prefix matching. Both accept `--strategy prefix|substring|fuzzy`. History entries containing control characters are excluded. Run `exec zsh` after upgrading to load the new shell integration.

```toml
[prompt]
prompt_style = "classic"  # lean, classic, rainbow, pure
newline = true          # false: input on the same line
font_level = "nerd"       # nerd, powerline, unicode, ascii
left_segments = ["os_icon", "dir", "git", "virtualenv"]
right_segments = ["duration", "status", "time"]
transient = true
ip_interface = ""        # Automatic when empty; e.g. en0 / eth0

[prompt.dir]
truncation_length = 3
truncation_symbol = "…"
home_symbol = "~"

[prompt.git]
show_ahead_behind = true
show_stash = true
show_status = true

[suggest]
strategy = "prefix"      # prefix, substring, fuzzy
highlight_color = "fg=8" # zsh region_highlight style
max_suggestions = 10
# Note: ghost text is shown only when the candidate extends the current input.
# With substring/fuzzy, mid-string matches are used by `zsh-turbo suggest` /
# `complete` but are not displayed as inline ghost text.

# Custom command segments
[[prompt.custom]]
name = "wifi"
command = "networksetup -getairportnetwork en0 | cut -d: -f2 | xargs"
icon = "📶"
fg = "white"
bg = "24"
when = "always"  # or "env:VAR" or "file:path"

[style]
rainbow_palette = "default" # e.g. blue, green, ocean, sunset, pastel
primary_color = "blue"
success_color = "green"
error_color = "red"
muted_color = "bright_black"

[style.rainbow_overrides.os_icon]
fg = "231"
bg = "24"

[style.rainbow_overrides.dir]
fg = "16"
bg = "#f0c674"

[style.rainbow_overrides.git]
fg = "231"
bg = "88"

[shell]
completion_dirs = ""           # Colon-separated absolute paths; no ~ or $HOME expansion
term_shell_integration = "auto" # auto, 1 (on), 0 (off)
```

Custom segment commands run through the shell with a 500ms timeout. Only stdout is displayed; commands that fail, time out, or write only to stderr are skipped. On macOS and Linux, a timeout terminates the command's process group, including pipelines and child processes.

## Available Segments

| Segment          | Description                          |
| ---------------- | ------------------------------------ |
| `dir`            | Current directory with truncation    |
| `git`            | Branch, staged/modified/untracked    |
| `status`         | Non-zero exit code                   |
| `duration`       | Command execution time (>2s)         |
| `time`           | Current time (HH:MM:SS)             |
| `virtualenv`     | Python venv/conda environment        |
| `kubecontext`    | Kubernetes context                   |
| `aws`            | AWS profile (`AWS_SSO_PROFILE` > `AWS_VAULT` > `AWSUME_PROFILE` > `AWS_PROFILE` > `AWS_DEFAULT_PROFILE`) |
| `gcloud`         | GCP project from active gcloud config (`CLOUDSDK_CONFIG` when non-empty; otherwise `~/.config/gcloud`) |
| `terraform`      | Terraform / OpenTofu workspace (`*.tf`, `*.tf.json`, `*.tofu`, `*.tofu.json`, `.terraform`) |
| `docker_context` | Docker context                       |
| `direnv`         | direnv-loaded directory (`DIRENV_DIR`) |
| `nix_shell`      | Nix shell mode (`pure` / `impure`)   |
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
| `load`           | 1-min load average (warning >70%, critical >90% per CPU) |
| `battery`        | Battery percent + charge state (macOS/Linux) |
| `disk_usage`     | Filesystem usage of CWD (shown only when ≥75%) |
| `ram`            | Memory usage (shown only when ≥75%) |
| `vi_mode`        | vi keymap state (insert/normal/visual via `ZSH_TURBO_VI_MODE`) |
| `proxy`          | "proxy" badge when `HTTP(S)_PROXY` / `ALL_PROXY` is set |
| `cpu_arch`       | CPU architecture (e.g. `arm64`, `x86_64`) |
| `root_indicator` | Shown when `USER=root`; falls back to uid 0 only when `USER` is unset |
| `dir_writable`   | Lock icon when the current directory is not writable |
| `ip`             | Primary IPv4 (`ZSH_TURBO_IP_INTERFACE` to override; macOS: `en0`, Linux: global scope) |
| *(custom)*       | Any command defined in `[[prompt.custom]]` |

## Project task completion

Task names are read from files in the current directory:

| Command | Definition |
| --- | --- |
| `make <target>` | `GNUmakefile`, `makefile`, or `Makefile` (first found) |
| `npm run <script>`, `npm run-script <script>`, `pnpm [run] <script>`, `bun [run] <script>`, `yarn [run] <script>` | `package.json` scripts |
| `uv run <script>` | `pyproject.toml` `[project.scripts]` |
| `deno task <name>` | `deno.json` or `deno.jsonc` tasks, plus `package.json` scripts |
| `mise run <task>`, `mise r <task>` | `mise.toml` or `.mise.toml` `[tasks]`, plus executable file tasks in the default task directories (`mise-tasks/`, `.mise-tasks/`, `mise/tasks/`, `.mise/tasks/`, `.config/mise/tasks/`) |
| `just <recipe>` | `justfile`, `Justfile`, or `.justfile` recipes (private recipes excluded) |
| `task <name>` (Go Task) | `Taskfile.yml`/`.yaml`, including `.dist` variants (internal tasks excluded) |

Typing a supported command shows its project tasks below the input. `make` shows Makefile targets and `pnpm` shows package.json scripts. For `npm`, `uv`, `deno`, and `mise`, candidates include the required `run` or `task` subcommand. Further input filters names by prefix; moving the cursor away from the end hides the list. **Suggest → Max Suggestions** limits its size. Malformed or oversized files are ignored, and task managers are not executed while finding candidates. The first task also appears as ghost text and can be accepted with Tab. **Suggest → Tab: Default** uses normal zsh completion and falls back to existing completion definitions when no project task matches. `zsh-turbo complete --project-only -- "make"` lists matching project tasks without history.

## CLI completion registration

Open **Completions** in `zsh-turbo configure`. The first screen lists registered commands. Use Up/Down and Enter to open a command's settings. Press `n` to add a CLI; cancelling the name edit discards the draft. Use Enter to edit settings, Esc to return to the list, Backspace (⌫) or `d` twice to confirm removal, and `s` to save. Press `r` to rebuild changed caches or `Shift+R` to force all saved registrations to rebuild. Both actions also appear as selectable rows in the details. Progress and results stay visible while you continue using the TUI. Restart zsh after adding, disabling, or removing registrations. Cache refreshes for existing registrations take effect in running shells.

- **Help** (default): reads `COMMAND --help` and discovered subcommand help, generating option names, subcommands, descriptions, and file/path arguments. It handles common English help layouts; arbitrary help formats, dynamic values, and inherited global options may require native completions. Scans use at most four workers, four subcommand levels, 128 help pages, five seconds per page, and 120 seconds overall.
- **Native generator**: enter an executable and its arguments as a JSON array, for example `["tool", "completion", "zsh"]`. No shell expansion or pipelines are applied. Use the CLI's documented generation command. A generator has a ten-second deadline.
- **Completion file**: enter a path to a trusted zsh `#compdef` file. `~/` is supported. This also works for a CLI not installed yet.

The source is explicit: zsh-turbo does not guess commands that might perform an unrelated action. Generator output and files are executable zsh code, just like existing `fpath` completions.

```toml
[[completions]]
command = "tool"
enabled = true
source = "help" # help / generator / file
# generator = ["tool", "completion", "zsh"]
# file = "~/completions/_tool"
watch_files = [] # e.g. ["~/bin/tool-core"] for a wrapper's actual executable
```

At shell startup and subsequent prompts (at least 30 seconds between checks), a background worker resolves executable paths and compares file metadata. It calculates SHA-256 only when metadata changes, then rebuilds when content, target path, registration, or extra watched files change. For a wrapper, shim, or plugin-driven CLI, add its actual executable/configuration files to **Extra files to watch**. Changes hidden behind an unchanged wrapper are not detected automatically. File sources watch both the completion file and the CLI executable when installed.

Cache files live in `${XDG_CACHE_HOME:-~/.cache}/zsh-turbo/managed-completions`. Tab uses already loaded functions and checks the cache file with zsh's built-in stat module; the cache layer does not launch a CLI/help/hash process on every keypress or Tab. Native completion scripts can still run their own commands to fetch dynamic candidates. Generation is locked per CLI, syntax-checked, and atomically published. A failure preserves the last working cache and is shown in the TUI. Updates load on the next Tab after publication; a failed initial generation leaves existing zsh completion available. Missing/empty XDG cache settings fall back to `~/.cache`.

Run `zsh-turbo completion-refresh` for an immediate change check, or add `--force` to rebuild without waiting for metadata to change. These commands report each registration's result.
