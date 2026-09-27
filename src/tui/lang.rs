#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Lang {
    En,
    Ja,
}

impl Lang {
    pub(super) fn from_env() -> Self {
        Self::resolve(|key| std::env::var(key).ok(), system_timezone_name)
    }

    pub(super) fn from_preference(preference: crate::config::UiLanguage) -> Self {
        match preference {
            crate::config::UiLanguage::Auto => Self::from_env(),
            crate::config::UiLanguage::En => Self::En,
            crate::config::UiLanguage::Ja => Self::Ja,
        }
    }

    fn resolve(
        env: impl Fn(&str) -> Option<String>,
        system_zone: impl FnOnce() -> Option<String>,
    ) -> Self {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(&env)
            .find(|value| !value.trim().is_empty());
        // 素の C/POSIX は明示的な英語指定として扱う。
        if locale
            .as_deref()
            .is_some_and(|value| is_c_locale(value) && !is_utf8_c_locale(value))
        {
            return Self::En;
        }
        if let Some(list) = env("LANGUAGE")
            && let Some(lang) = list.split(':').find_map(Self::parse)
        {
            return lang;
        }
        if let Some(value) = locale.as_deref()
            && !is_utf8_c_locale(value)
        {
            return Self::parse(value).unwrap_or(Self::En);
        }
        let zone = env("TZ")
            .filter(|value| !value.trim().is_empty())
            .or_else(system_zone);
        if zone.as_deref().is_some_and(is_japan_zone) {
            Self::Ja
        } else {
            Self::En
        }
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

fn is_utf8_c_locale(value: &str) -> bool {
    let without_modifier = value.trim().split('@').next().unwrap_or("");
    let Some((base, charset)) = without_modifier.split_once('.') else {
        return false;
    };
    (base.eq_ignore_ascii_case("C") || base.eq_ignore_ascii_case("POSIX"))
        && (charset.eq_ignore_ascii_case("UTF-8") || charset.eq_ignore_ascii_case("UTF8"))
}

fn is_japan_zone(value: &str) -> bool {
    matches!(value.trim().trim_start_matches(':'), "Asia/Tokyo" | "Japan")
}

fn system_timezone_name() -> Option<String> {
    #[cfg(unix)]
    if let Ok(link) = std::fs::read_link("/etc/localtime")
        && let Some(name) = link.to_str()
        && let Some((_, zone)) = name.split_once("/zoneinfo/")
    {
        return Some(zone.into());
    }
    iana_time_zone::get_timezone().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(pairs: &[(&str, &str)]) -> Lang {
        resolve_with_zone(pairs, None)
    }

    fn resolve_with_zone(pairs: &[(&str, &str)], zone: Option<&str>) -> Lang {
        Lang::resolve(
            |key| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| v.to_string())
            },
            || zone.map(str::to_owned),
        )
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
        assert_eq!(resolve(&[("TZ", "Asia/Tokyo")]), Lang::Ja);
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
        for locale in ["C", "POSIX", " C@foo "] {
            assert_eq!(
                resolve(&[("LC_ALL", locale), ("LANGUAGE", "ja"), ("LANG", "ja")]),
                Lang::En
            );
        }
        for locale in ["c.UTF-8", "POSIX.UTF-8"] {
            assert_eq!(
                resolve(&[("LC_ALL", locale), ("LANGUAGE", "ja"), ("LANG", "en")]),
                Lang::Ja
            );
        }
    }

    #[test]
    fn c_utf8では日本のタイムゾーンをフォールバックに使う() {
        for locale in ["C.UTF-8", "c.utf8", "POSIX.UTF-8"] {
            assert_eq!(
                resolve_with_zone(&[("LC_ALL", locale)], Some("Asia/Tokyo")),
                Lang::Ja
            );
            assert_eq!(
                resolve_with_zone(&[("LC_ALL", locale)], Some("Japan")),
                Lang::Ja
            );
            assert_eq!(
                resolve_with_zone(&[("LC_ALL", locale)], Some("Asia/Seoul")),
                Lang::En
            );
        }
        assert_eq!(
            resolve_with_zone(&[("LC_ALL", "C")], Some("Asia/Tokyo")),
            Lang::En
        );
        assert_eq!(
            resolve_with_zone(&[("LANG", "en_US.UTF-8")], Some("Asia/Tokyo")),
            Lang::En
        );
        assert_eq!(
            resolve_with_zone(&[("LANG", "fr_FR.UTF-8")], Some("Asia/Tokyo")),
            Lang::En
        );
        assert_eq!(
            resolve_with_zone(
                &[("LC_ALL", "C.UTF-8"), ("LANGUAGE", "en")],
                Some("Asia/Tokyo")
            ),
            Lang::En
        );
    }

    #[test]
    fn tz環境変数をシステムタイムゾーンより優先する() {
        assert_eq!(
            resolve_with_zone(&[("TZ", ":Asia/Tokyo")], Some("UTC")),
            Lang::Ja
        );
        assert_eq!(
            resolve_with_zone(&[("TZ", "UTC")], Some("Asia/Tokyo")),
            Lang::En
        );
        assert_eq!(resolve_with_zone(&[], Some("Asia/Tokyo")), Lang::Ja);
        assert_eq!(resolve_with_zone(&[], Some("Asia/Seoul")), Lang::En);
        assert_eq!(resolve_with_zone(&[], None), Lang::En);
    }
}
