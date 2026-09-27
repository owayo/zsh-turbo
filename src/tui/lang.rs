#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Lang {
    En,
    Ja,
}

impl Lang {
    pub(super) fn from_env() -> Self {
        Self::resolve(|key| std::env::var(key).ok())
    }

    pub(super) fn from_preference(preference: crate::config::UiLanguage) -> Self {
        match preference {
            crate::config::UiLanguage::Auto => Self::from_env(),
            crate::config::UiLanguage::En => Self::En,
            crate::config::UiLanguage::Ja => Self::Ja,
        }
    }

    fn resolve(env: impl Fn(&str) -> Option<String>) -> Self {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(&env)
            .find(|value| !value.trim().is_empty());
        // C ロケールの英語指定は LANGUAGE より優先する。
        if locale.as_deref().is_some_and(is_c_locale) {
            return Self::En;
        }
        if let Some(list) = env("LANGUAGE")
            && let Some(lang) = list.split(':').find_map(Self::parse)
        {
            return lang;
        }
        locale.as_deref().and_then(Self::parse).unwrap_or(Self::En)
    }

    fn parse(value: &str) -> Option<Self> {
        if is_c_locale(value) {
            return Some(Self::En);
        }
        match value
            .trim()
            .split(['_', '-', '.', '@'])
            .next()?
            .to_ascii_lowercase()
            .as_str()
        {
            "ja" | "jpn" | "japanese" => Some(Self::Ja),
            "en" | "eng" | "english" => Some(Self::En),
            _ => None,
        }
    }

    pub(super) fn text(self, en: &'static str, ja: &'static str) -> &'static str {
        match self {
            Self::En => en,
            Self::Ja => ja,
        }
    }

    pub(super) fn tabs(self) -> &'static [&'static str] {
        match self {
            Self::En => super::TAB_TITLES,
            Self::Ja => &[
                "プロンプト",
                "配置",
                "Git",
                "入力候補",
                "配色",
                "カスタム",
                "シェル",
                "補完",
            ],
        }
    }
}

fn is_c_locale(value: &str) -> bool {
    let base = value.trim().split(['.', '@']).next().unwrap_or("");
    base.eq_ignore_ascii_case("C") || base.eq_ignore_ascii_case("POSIX")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(pairs: &[(&str, &str)]) -> Lang {
        Lang::resolve(|key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        })
    }

    #[test]
    fn 日本語タグとそれ以外の環境を判定する() {
        for locale in [
            "ja",
            "JA",
            "ja_JP.UTF-8",
            "ja-JP",
            "jpn",
            "Japanese",
            "ja_JP@calendar=japanese",
        ] {
            assert_eq!(resolve(&[("LANG", locale)]), Lang::Ja, "{locale}");
        }
        for locale in [
            "en_US.UTF-8",
            "fr_FR",
            "de_DE.UTF-8",
            "ko_KR",
            "",
            "japaneseish",
        ] {
            assert_eq!(resolve(&[("LANG", locale)]), Lang::En, "{locale}");
        }
        assert_eq!(resolve(&[]), Lang::En);
        assert_eq!(resolve(&[("TZ", "Asia/Tokyo")]), Lang::En);
    }

    #[test]
    fn 最初の非空ロケールを優先する() {
        assert_eq!(
            resolve(&[
                ("LC_ALL", "en_US.UTF-8"),
                ("LC_MESSAGES", "ja"),
                ("LANG", "ja")
            ]),
            Lang::En
        );
        assert_eq!(resolve(&[("LC_MESSAGES", "ja"), ("LANG", "en")]), Lang::Ja);
        assert_eq!(
            resolve(&[("LC_ALL", "  "), ("LC_MESSAGES", ""), ("LANG", "ja")]),
            Lang::Ja
        );
        assert_eq!(resolve(&[("LC_ALL", "fr_FR"), ("LANG", "ja")]), Lang::En);
    }

    #[test]
    fn cロケール以外ではlanguageの対応言語を優先する() {
        assert_eq!(
            resolve(&[("LANGUAGE", "fr:ja:en"), ("LANG", "en_US.UTF-8")]),
            Lang::Ja
        );
        assert_eq!(
            resolve(&[("LANGUAGE", "en:ja"), ("LANG", "ja_JP.UTF-8")]),
            Lang::En
        );
        assert_eq!(resolve(&[("LANGUAGE", "fr:de"), ("LANG", "ja")]), Lang::Ja);
        assert_eq!(resolve(&[("LANGUAGE", "ja")]), Lang::Ja);
        for locale in ["C", "POSIX", "c.UTF-8", "POSIX.UTF-8", " C@foo "] {
            assert_eq!(
                resolve(&[("LC_ALL", locale), ("LANGUAGE", "ja"), ("LANG", "ja")]),
                Lang::En
            );
        }
    }
}
