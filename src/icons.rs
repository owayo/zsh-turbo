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

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Nerd => "nerd",
            Self::Powerline => "powerline",
            Self::Unicode => "unicode",
            Self::Ascii => "ascii",
        }
    }

    pub fn has_powerline(&self) -> bool {
        matches!(self, Self::Nerd | Self::Powerline)
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
            os_icon: "\u{f179}",
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
            os_icon: "\u{e0b0}",
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
            os_icon: if cfg!(target_os = "macos") {
                "\u{f179}"
            } else {
                "\u{1f427}"
            },
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
        }
    }
}
