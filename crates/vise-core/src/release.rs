
pub const RELEASE: Option<&str> = match option_env!("VISE_VERSION") {
    Some(version) if !version.is_empty() => Some(version),
    _ => None,
};

pub const VERSION: &str = match RELEASE {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

pub const NIGHTLY: Option<&str> = match option_env!("VISE_NIGHTLY") {
    Some(nightly) if !nightly.is_empty() => Some(nightly),
    _ => None,
};

pub fn nightly_label() -> Option<String> {
    NIGHTLY.map(label)
}

pub fn nightly_version() -> Option<String> {
    NIGHTLY.map(|n| format!("nightly-{}", n.replace(' ', "-")))
}

fn label(nightly: &str) -> String {
    match nightly.split_once(' ') {
        Some((date, commit)) => format!("Nightly {date} · {commit}"),
        None => format!("Nightly {nightly}"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_nightly_names_its_date_and_commit() {
        assert_eq!(
            super::label("2026-10-28 0c02067"),
            "Nightly 2026-10-28 · 0c02067"
        );
        assert_eq!(super::label("2026-10-28"), "Nightly 2026-10-28");
        assert_eq!(super::nightly_label().is_some(), super::NIGHTLY.is_some());
    }

    #[test]
    fn the_version_is_the_release_or_the_crate() {
        assert_eq!(
            super::RELEASE,
            option_env!("VISE_VERSION").filter(|v| !v.is_empty())
        );
        assert_eq!(
            super::VERSION,
            super::RELEASE.unwrap_or(env!("CARGO_PKG_VERSION"))
        );
        assert!(!super::VERSION.starts_with('v'));
    }
}
