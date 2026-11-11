use super::helpers::run_cmd;
use std::collections::BTreeSet;

pub(super) const STATUS_ARGS: &[&str] = &[
    "--no-lazy-fetch",
    "--no-pager",
    "--no-optional-locks",
    "-c",
    "safe.bareRepository=explicit",
    "-c",
    "core.fsmonitor=",
    "-c",
    "status.submoduleSummary=false",
    "status",
    "--porcelain",
    "-b",
    "--ignore-submodules=dirty",
];

/// 設定のキーだけを読み、属性から起動されるフィルターを今回の呼び出しで無効にする。
pub(super) fn status() -> Option<String> {
    let keys = run_cmd(
        "git",
        &[
            "--no-lazy-fetch",
            "--no-pager",
            "-c",
            "safe.bareRepository=explicit",
            "config",
            "--null",
            "--name-only",
            "--list",
        ],
    )?;
    let args = status_args(&keys)?;
    run_cmd("git", &args.iter().map(String::as_str).collect::<Vec<_>>())
}

fn status_args(keys: &str) -> Option<Vec<String>> {
    if !keys.is_empty() && !keys.ends_with('\0') {
        return None;
    }
    let mut drivers = BTreeSet::new();
    for key in keys.split_terminator('\0') {
        if !key.starts_with("filter.") {
            continue;
        }
        let (driver, field) = key.rsplit_once('.')?;
        if !matches!(field, "clean" | "smudge" | "process" | "required") {
            continue;
        }
        // `-c key=value` の区切りや UTF-8 の置換で、違うキーを無効にしてしまわないようにする。
        if !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return None;
        }
        drivers.insert(driver);
    }
    let mut args = Vec::new();
    for driver in drivers {
        for field in ["clean", "smudge", "process"] {
            args.extend(["-c".into(), format!("{driver}.{field}=")]);
        }
        args.extend(["-c".into(), format!("{driver}.required=false")]);
    }
    args.extend(STATUS_ARGS.iter().map(|s| (*s).to_owned()));
    Some(args)
}

pub(super) fn stash_count() -> Option<usize> {
    // stash list は log.showSignature により署名検証プログラムを起動するため使わない。
    run_cmd(
        "git",
        &[
            "--no-lazy-fetch",
            "--no-pager",
            "--no-optional-locks",
            "-c",
            "safe.bareRepository=explicit",
            "rev-list",
            "--walk-reflogs",
            "--count",
            "refs/stash",
        ],
    )?
    .parse()
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn フィルターの名前を一意にして設定を上書きする() {
        let args = status_args("core.bare\0filter.MiXeD.Name-1.clean\0filter.MiXeD.Name-1.process\0filter.MiXeD.Name-1.required\0").unwrap();
        assert_eq!(
            args.iter()
                .filter(|s| *s == "filter.MiXeD.Name-1.clean=")
                .count(),
            1
        );
        assert!(args.iter().any(|s| s == "filter.MiXeD.Name-1.process="));
        assert!(
            args.iter()
                .any(|s| s == "filter.MiXeD.Name-1.required=false")
        );
        assert!(
            args.ends_with(
                &STATUS_ARGS
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect::<Vec<_>>()
            )
        );
    }

    #[test]
    fn 正しく上書きできないキーや途切れた一覧ではstatusを実行しない() {
        for keys in [
            "filter.a=b.clean\0",
            "filter.名前.clean\0",
            "filter.\u{fffd}.process\0",
            "filter.a\nb.clean\0",
            "core.bare",
        ] {
            assert!(status_args(keys).is_none(), "{keys:?}");
        }
        assert!(status_args("").is_some());
        assert!(status_args("core.bare\0").is_some());
    }
}
