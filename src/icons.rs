#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontLevel {
    Nerd,
    Powerline,
    Unicode,
    Ascii,
}

impl FontLevel {
    pub fn from_str(s: &str) -> Self {
        match s {
            "nerd" => Self::Nerd,
            "powerline" => Self::Powerline,
            "ascii" => Self::Ascii,
            _ => Self::Unicode,
        }
    }
}

pub struct Icons {
    pub dir: &'static str,
    pub git_branch: &'static str,
    pub git_staged: &'static str,
    pub git_modified: &'static str,
    pub git_untracked: &'static str,
    pub git_ahead: &'static str,
    pub git_behind: &'static str,
    pub git_stash: &'static str,
    pub prompt_ok: &'static str,
    pub prompt_err: &'static str,
    pub duration: &'static str,
    pub time_icon: &'static str,
    pub separator_left: &'static str,
    #[allow(dead_code)]
    pub separator_right: &'static str,
    pub node: &'static str,
    pub python: &'static str,
    pub rust: &'static str,
    pub go_lang: &'static str,
    pub error: &'static str,
    pub virtualenv: &'static str,
    pub kubernetes: &'static str,
    pub ssh_icon: &'static str,
    pub package: &'static str,
    pub jobs: &'static str,
    pub ruby: &'static str,
    pub java: &'static str,
    pub php: &'static str,
    pub swift: &'static str,
    pub dotnet: &'static str,
    pub aws: &'static str,
    pub terraform: &'static str,
    pub docker: &'static str,
    pub os_icon: &'static str,
    pub gcloud: &'static str,
    pub direnv: &'static str,
    pub nix_shell: &'static str,
    pub load: &'static str,
    pub battery_low: &'static str,
    pub battery_charging: &'static str,
    pub battery_full: &'static str,
    pub disk: &'static str,
    pub ram: &'static str,
    pub vi_insert: &'static str,
    pub vi_normal: &'static str,
    pub vi_visual: &'static str,
    pub cpu_arch: &'static str,
    pub root: &'static str,
    pub lock: &'static str,
    pub network: &'static str,
}

impl Icons {
    pub fn for_level(level: FontLevel) -> Self {
        match level {
            FontLevel::Nerd => Self::nerd(),
            FontLevel::Powerline => Self::powerline(),
            FontLevel::Unicode => Self::unicode(),
            FontLevel::Ascii => Self::ascii(),
        }
    }

    fn nerd() -> Self {
        Self {
            dir: "\u{f07c}",
            git_branch: "\u{e0a0}",
            git_staged: "+",
            git_modified: "!",
            git_untracked: "?",
            git_ahead: "\u{f062}",
            git_behind: "\u{f063}",
            git_stash: "\u{f01c}",
            prompt_ok: "\u{276f}",
            prompt_err: "\u{276f}",
            duration: "\u{f017}",
            time_icon: "\u{f017}",
            separator_left: "\u{e0b0}",
            separator_right: "\u{e0b2}",
            node: "\u{e718}",
            python: "\u{e73c}",
            rust: "\u{e7a8}",
            go_lang: "\u{e724}",
            error: "\u{f00d}",
            virtualenv: "\u{e73c}",
            kubernetes: "\u{2388}",
            ssh_icon: "\u{f023}",
            package: "\u{f487}",
            jobs: "\u{f013}",
            ruby: "\u{e739}",
            java: "\u{e738}",
            php: "\u{e73d}",
            swift: "\u{e755}",
            dotnet: "\u{e77f}",
            aws: "\u{e7ad}",
            terraform: "tf",
            docker: "\u{e7b0}",
            os_icon: if cfg!(target_os = "macos") {
                "\u{f179}" // nf-fa-apple
            } else {
                "\u{f17c}" // nf-fa-linux (Tux)
            },
            gcloud: "\u{f1a0}",
            direnv: "\u{f422}",
            nix_shell: "\u{f313}",
            load: "\u{f0e4}",
            battery_low: "\u{f244}",
            battery_charging: "\u{f1e6}",
            battery_full: "\u{f240}",
            disk: "\u{f0a0}",
            ram: "\u{f538}",
            vi_insert: "I",
            vi_normal: "N",
            vi_visual: "V",
            cpu_arch: "\u{f4bc}",
            root: "\u{f0e7}",
            lock: "\u{f023}",
            network: "\u{f6ff}",
        }
    }

    fn powerline() -> Self {
        Self {
            dir: "",
            git_branch: "\u{e0a0}",
            git_staged: "+",
            git_modified: "!",
            git_untracked: "?",
            git_ahead: "\u{2191}",
            git_behind: "\u{2193}",
            git_stash: "\u{2261}",
            prompt_ok: "\u{276f}",
            prompt_err: "\u{276f}",
            duration: "\u{23f1}\u{fe0e}",
            time_icon: "",
            separator_left: "\u{e0b0}",
            separator_right: "\u{e0b2}",
            node: "\u{2b22}",
            python: "py",
            rust: "rs",
            go_lang: "go",
            error: "\u{2718}",
            virtualenv: "venv",
            kubernetes: "\u{2388}",
            ssh_icon: "\u{1f512}",
            package: "pkg",
            jobs: "&",
            ruby: "rb",
            java: "java",
            php: "php",
            swift: "swift",
            dotnet: ".net",
            aws: "aws",
            terraform: "tf",
            docker: "docker",
            // powerline フォントに OS グリフはないため ascii 階層と同じテキスト表記
            os_icon: if cfg!(target_os = "macos") {
                "mac"
            } else {
                "linux"
            },
            gcloud: "gcp",
            direnv: "env",
            nix_shell: "nix",
            load: "load",
            battery_low: "bat",
            battery_charging: "bat",
            battery_full: "bat",
            disk: "disk",
            ram: "ram",
            vi_insert: "I",
            vi_normal: "N",
            vi_visual: "V",
            cpu_arch: "arch",
            root: "root",
            lock: "\u{1f512}",
            network: "net",
        }
    }

    fn unicode() -> Self {
        Self {
            dir: "",
            git_branch: "\u{2387}",
            git_staged: "+",
            git_modified: "!",
            git_untracked: "?",
            git_ahead: "\u{2191}",
            git_behind: "\u{2193}",
            git_stash: "\u{2261}",
            prompt_ok: "\u{276f}",
            prompt_err: "\u{276f}",
            duration: "\u{23f1}\u{fe0e}",
            time_icon: "",
            separator_left: "\u{2502}",
            separator_right: "\u{2502}",
            node: "\u{2b22}",
            python: "py",
            rust: "rs",
            go_lang: "go",
            error: "\u{2718}",
            virtualenv: "venv",
            kubernetes: "\u{2388}",
            ssh_icon: "\u{1f512}",
            package: "pkg",
            jobs: "&",
            ruby: "rb",
            java: "java",
            php: "php",
            swift: "swift",
            dotnet: ".net",
            aws: "aws",
            terraform: "tf",
            docker: "docker",
            // unicode 階層は Nerd Font 不要が契約のため PUA グリフを使わない
            os_icon: if cfg!(target_os = "macos") {
                "\u{1f34e}"
            } else {
                "\u{1f427}"
            },
            gcloud: "gcp",
            direnv: "env",
            nix_shell: "nix",
            load: "load",
            battery_low: "bat",
            battery_charging: "bat",
            battery_full: "bat",
            disk: "disk",
            ram: "ram",
            vi_insert: "I",
            vi_normal: "N",
            vi_visual: "V",
            cpu_arch: "arch",
            root: "root",
            lock: "\u{1f512}",
            network: "net",
        }
    }

    fn ascii() -> Self {
        Self {
            dir: "",
            git_branch: "",
            git_staged: "+",
            git_modified: "!",
            git_untracked: "?",
            git_ahead: "^",
            git_behind: "v",
            git_stash: "*",
            prompt_ok: ">",
            prompt_err: ">",
            duration: "",
            time_icon: "",
            separator_left: "|",
            separator_right: "|",
            node: "node",
            python: "py",
            rust: "rs",
            go_lang: "go",
            error: "x",
            virtualenv: "venv",
            kubernetes: "k8s",
            ssh_icon: "ssh",
            package: "pkg",
            jobs: "&",
            ruby: "rb",
            java: "java",
            php: "php",
            swift: "swift",
            dotnet: ".net",
            aws: "aws",
            terraform: "tf",
            docker: "docker",
            os_icon: if cfg!(target_os = "macos") {
                "mac"
            } else {
                "linux"
            },
            gcloud: "gcp",
            direnv: "env",
            nix_shell: "nix",
            load: "load",
            battery_low: "bat",
            battery_charging: "bat",
            battery_full: "bat",
            disk: "disk",
            ram: "ram",
            vi_insert: "I",
            vi_normal: "N",
            vi_visual: "V",
            cpu_arch: "arch",
            root: "root",
            lock: "lock",
            network: "net",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── FontLevel::from_str のテスト ─────────────────────────────

    #[test]
    fn from_str_known_values() {
        assert_eq!(FontLevel::from_str("nerd"), FontLevel::Nerd);
        assert_eq!(FontLevel::from_str("powerline"), FontLevel::Powerline);
        assert_eq!(FontLevel::from_str("ascii"), FontLevel::Ascii);
    }

    #[test]
    fn from_str_unicode_explicit() {
        assert_eq!(FontLevel::from_str("unicode"), FontLevel::Unicode);
    }

    #[test]
    fn from_str_unknown_defaults_to_unicode() {
        assert_eq!(FontLevel::from_str("unknown"), FontLevel::Unicode);
        assert_eq!(FontLevel::from_str(""), FontLevel::Unicode);
        assert_eq!(FontLevel::from_str("NERD"), FontLevel::Unicode);
    }

    // ── Icons::for_level のテスト ────────────────────────────────

    #[test]
    fn nerd_icons_have_powerline_separator() {
        let icons = Icons::for_level(FontLevel::Nerd);
        assert_eq!(icons.separator_left, "\u{e0b0}");
        assert_eq!(icons.separator_right, "\u{e0b2}");
    }

    #[test]
    fn nerd_icons_have_specific_values() {
        let icons = Icons::for_level(FontLevel::Nerd);
        assert_eq!(icons.git_branch, "\u{e0a0}");
        assert_eq!(icons.dir, "\u{f07c}");
        assert_eq!(icons.error, "\u{f00d}");
    }

    #[test]
    fn powerline_icons_have_powerline_separator() {
        let icons = Icons::for_level(FontLevel::Powerline);
        assert_eq!(icons.separator_left, "\u{e0b0}");
    }

    #[test]
    fn ascii_icons_use_plain_characters() {
        let icons = Icons::for_level(FontLevel::Ascii);
        assert_eq!(icons.separator_left, "|");
        assert_eq!(icons.git_ahead, "^");
        assert_eq!(icons.git_behind, "v");
        assert_eq!(icons.error, "x");
    }

    #[test]
    fn unicode_icons_use_unicode_separator() {
        let icons = Icons::for_level(FontLevel::Unicode);
        assert_eq!(icons.separator_left, "\u{2502}");
    }

    // ── os_icon の OS 分岐 ──────────────────────────────────────

    #[test]
    fn os_icon_はプラットフォームに対応するグリフを返す() {
        if cfg!(target_os = "macos") {
            assert_eq!(Icons::nerd().os_icon, "\u{f179}"); // nf-fa-apple
            assert_eq!(Icons::unicode().os_icon, "\u{1f34e}");
            assert_eq!(Icons::powerline().os_icon, "mac");
            assert_eq!(Icons::ascii().os_icon, "mac");
        } else {
            assert_eq!(Icons::nerd().os_icon, "\u{f17c}"); // nf-fa-linux
            assert_eq!(Icons::unicode().os_icon, "\u{1f427}");
            assert_eq!(Icons::powerline().os_icon, "linux");
            assert_eq!(Icons::ascii().os_icon, "linux");
        }
    }

    #[test]
    fn unicode_os_icon_はnerd_font専用のpuaグリフを使わない() {
        // unicode 階層は「Nerd Font 不要」が契約。私用領域 (U+E000..U+F8FF) を禁止する
        for ch in Icons::unicode().os_icon.chars() {
            let cp = ch as u32;
            assert!(
                !(0xe000..=0xf8ff).contains(&cp),
                "unicode 階層の os_icon に PUA グリフ U+{cp:04X} が含まれている"
            );
        }
    }
}
