pub use super::open;
use super::{File, Index, Kind, Project, Revoked, SCHEMA, Version, hex64, inspect, valid_id};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ENTRIES: &str = "mods";
pub const REVOKED: &str = "revoked.toml";
const RESERVED: [&str; 1] = ["submit"];
const MAX_ICON: u64 = 256 << 10;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub owners: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<EntryVersion>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Page {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cache {
    #[serde(default)]
    pub page: Page,
    #[serde(default)]
    pub versions: Vec<Version>,
}

pub const CACHE: &str = "versions";
pub const PAGE_FILE: &str = "vise-registry.toml";
const MAX_CHANGELOG: usize = 6000;

impl Cache {
    pub fn read(registry: &Path, id: &str) -> Result<Cache, String> {
        let path = registry.join(CACHE).join(format!("{id}.json"));
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Cache::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

pub fn valid_repo(repo: &str) -> bool {
    let part = |p: &str, max: usize| {
        !p.is_empty()
            && p.len() <= max
            && p != "."
            && p != ".."
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    repo.split_once('/')
        .is_some_and(|(owner, name)| part(owner, 39) && part(name, 100))
}

impl Entry {
    pub fn resolved(&self, page: &Page) -> Entry {
        let mut e = self.clone();
        let fill = |own: &mut String, other: &Option<String>| {
            if own.trim().is_empty() {
                *own = other.clone().unwrap_or_default().trim().to_owned();
            }
        };
        fill(&mut e.name, &page.name);
        fill(&mut e.summary, &page.summary);
        fill(&mut e.description, &page.description);
        fill(&mut e.license, &page.license);
        if e.authors.is_empty() {
            e.authors = page.authors.clone();
        }
        if e.source.is_empty()
            && let Some(repo) = &self.repo
        {
            e.source = format!("https://github.com/{repo}");
        }
        e
    }

    pub fn check_fields(&self) -> Vec<String> {
        let id = &self.id;
        let mut errors = Vec::new();
        if !valid_id(id) {
            errors.push(format!("{id:?} is not [a-z0-9_]{{1,64}}"));
        }
        if RESERVED.contains(&id.as_str()) {
            errors.push(format!("{id:?} is reserved: pick another id"));
        }
        if let Err(e) = open(&self.kind) {
            errors.push(format!("{id}: {e}"));
        }
        if self.repo.as_deref().is_some_and(|r| !valid_repo(r)) {
            errors.push(format!("{id}: repo must be owner/name on GitHub"));
        }
        if self.owners.is_empty() {
            errors.push(format!("{id}: needs at least one owner (a GitHub login)"));
        }
        for (field, value) in [
            ("name", &self.name),
            ("summary", &self.summary),
            ("license", &self.license),
            ("source", &self.source),
        ] {
            if value.trim().is_empty() {
                errors.push(format!(
                    "{id}: has no {field} (in the entry, or in {PAGE_FILE} of its repo)"
                ));
            }
        }
        if self.summary.chars().count() > 160 {
            errors.push(format!("{id}: the summary is over 160 characters"));
        }
        errors
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EntryVersion {
    pub version: String,
    pub published: String,
    pub game: String,
    pub file: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub changelog: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevokedFile {
    #[serde(default)]
    revoked: Vec<Revoked>,
}

pub struct Sources<'a> {
    pub registry: &'a Path,
    pub files: &'a Path,
    pub release: &'a str,
    pub site: &'a str,
    pub generated: String,
}

pub fn tag(id: &str, version: &str) -> String {
    format!("{id}-{version}")
}

pub fn load(registry: &Path) -> Result<(Vec<Entry>, Vec<Revoked>), Vec<String>> {
    let mut errors = Vec::new();
    let mut entries = Vec::new();
    let dir = registry.join(ENTRIES);
    let mut paths: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(read) => read.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(e) => return Err(vec![format!("{}: {e}", dir.display())]),
    };
    paths.sort();
    for path in paths {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let Some(stem) = name.strip_suffix(".toml") else {
            errors.push(format!("{ENTRIES}/{name}: only <id>.toml files go here"));
            continue;
        };
        let parsed = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| toml::from_str::<Entry>(&text).map_err(|e| e.to_string()));
        match parsed {
            Ok(entry) if entry.id == stem => entries.push(entry),
            Ok(entry) => errors.push(format!(
                "{ENTRIES}/{name}: its id is {:?}; the file must be {ENTRIES}/{}.toml",
                entry.id, entry.id
            )),
            Err(e) => errors.push(format!("{ENTRIES}/{name}: {e}")),
        }
    }
    let revoked = match std::fs::read_to_string(registry.join(REVOKED)) {
        Ok(text) => match toml::from_str::<RevokedFile>(&text) {
            Ok(file) => file.revoked,
            Err(e) => {
                errors.push(format!("{REVOKED}: {e}"));
                Vec::new()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => {
            errors.push(format!("{REVOKED}: {e}"));
            Vec::new()
        }
    };
    if errors.is_empty() {
        Ok((entries, revoked))
    } else {
        Err(errors)
    }
}

fn plain_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.as_bytes()[0].is_ascii_alphanumeric()
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
}

fn plain_date(d: &str) -> bool {
    let b = d.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

pub fn listed(entry: &Entry, src: &Sources) -> bool {
    let cached = entry.repo.is_some()
        && Cache::read(src.registry, &entry.id).is_ok_and(|c| !c.versions.is_empty());
    !entry.versions.is_empty() || cached || entry.repo.is_none()
}

pub fn project(entry: &Entry, src: &Sources) -> Result<Project, Vec<String>> {
    let cache = match &entry.repo {
        Some(_) => Cache::read(src.registry, &entry.id).map_err(|e| vec![e])?,
        None => Cache::default(),
    };
    let entry = &entry.resolved(&cache.page);
    let mut errors = entry.check_fields();
    let id = &entry.id;
    if !valid_id(id) || RESERVED.contains(&id.as_str()) {
        return Err(errors);
    }
    let icon = match entry.icon.as_deref() {
        None => None,
        Some(url) if url.starts_with("https://") => Some(url.to_owned()),
        Some(path) => {
            let ok = path
                .strip_prefix("icons/")
                .is_some_and(|name| super::plain_name(name) && name.ends_with(".png"));
            let size = std::fs::metadata(src.registry.join(path)).map(|m| m.len());
            match (ok, size) {
                (true, Ok(n)) if n <= MAX_ICON => Some(format!("{}{path}", src.site)),
                (true, Ok(_)) => {
                    errors.push(format!("{id}: {path} is over {} KiB", MAX_ICON >> 10));
                    None
                }
                (true, Err(e)) => {
                    errors.push(format!("{id}: {path}: {e}"));
                    None
                }
                (false, _) => {
                    errors.push(format!(
                        "{id}: icon {path:?} is neither icons/<name>.png nor https://"
                    ));
                    None
                }
            }
        }
    };
    if entry.versions.is_empty() && cache.versions.is_empty() {
        errors.push(format!("{id}: has no version"));
    }
    let mut versions = Vec::new();
    for v in &entry.versions {
        match version(entry, v, src) {
            Ok(_) if file_kind(entry, v) != entry.kind => {}
            Ok(v) => versions.push(v),
            Err(e) => errors.push(format!("{id} {}: {e}", v.version)),
        }
    }
    versions.extend(cache.versions);
    versions.sort_by(|a: &Version, b| b.published.cmp(&a.published));
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(Project {
        id: id.clone(),
        kind: entry.kind.clone(),
        name: entry.name.clone(),
        summary: entry.summary.clone(),
        description: entry.description.clone(),
        authors: entry.authors.clone(),
        owners: entry.owners.clone(),
        license: entry.license.clone(),
        source: entry.source.clone(),
        icon,
        key: entry.key.as_ref().map(|k| k.trim().to_owned()),
        versions,
        downloads: 0,
    })
}

fn file_kind(entry: &Entry, v: &EntryVersion) -> Kind {
    match entry.kind {
        Kind::NativeMod if v.file.ends_with(".vmod") => Kind::Mod,
        _ => entry.kind.clone(),
    }
}

fn version(entry: &Entry, v: &EntryVersion, src: &Sources) -> Result<Version, String> {
    hex64(&v.sha256)?;
    let url = format!("{}{}/{}", src.release, tag(&entry.id, &v.version), v.file);
    version_at(entry, v, &src.files.join(&v.sha256), url)
}

fn version_at(
    entry: &Entry,
    v: &EntryVersion,
    path: &Path,
    url: String,
) -> Result<Version, String> {
    if !plain_version(&v.version) {
        return Err("a version is letters, digits, '.', '-' and '_'".into());
    }
    if !plain_date(&v.published) {
        return Err(format!("published {:?} is not YYYY-MM-DD", v.published));
    }
    hex64(&v.sha256)?;
    let size = std::fs::metadata(path)
        .map_err(|e| format!("its file {}: {e}", v.sha256))?
        .len();
    if super::sha256_file(path)? != v.sha256 {
        return Err("the file isn't the one its sha256 names".into());
    }
    match (&entry.key, &v.signature) {
        (Some(key), Some(sig)) => super::verify_signature(key, sig, path)?,
        (Some(_), None) => return Err("the project has a key: sign the file with it".into()),
        (None, Some(_)) => return Err("signed, but the project has no key".into()),
        (None, None) => {}
    }
    let facts = inspect(&file_kind(entry, v), path)?;
    if facts.id.as_ref().is_some_and(|id| *id != entry.id) {
        return Err(format!(
            "the file's id is {:?}, not {:?}",
            facts.id.unwrap_or_default(),
            entry.id
        ));
    }
    if facts.version.as_ref().is_some_and(|x| *x != v.version) {
        return Err(format!(
            "the file says version {:?}",
            facts.version.unwrap_or_default()
        ));
    }
    if facts.game.as_ref().is_some_and(|g| *g != v.game) {
        return Err(format!(
            "the file is for {:?}, the entry says {:?}",
            facts.game.unwrap_or_default(),
            v.game
        ));
    }
    Ok(Version {
        version: v.version.clone(),
        published: v.published.clone(),
        changelog: v.changelog.clone(),
        api: facts.api,
        side: facts.side,
        capabilities: facts.capabilities,
        api_level: facts.api_level,
        game: v.game.clone(),
        requires: facts.requires,
        file: File {
            url,
            name: v.file.clone(),
            size,
            sha256: v.sha256.clone(),
            signature: v.signature.clone(),
        },
    })
}

pub struct Release<'a> {
    pub tag: &'a str,
    pub published: &'a str,
    pub notes: &'a str,
    pub asset: &'a str,
    pub game: &'a str,
    pub signature: Option<&'a str>,
}

pub fn release_version(entry: &Entry, r: &Release, path: &Path) -> Result<Version, String> {
    let repo = entry
        .repo
        .as_deref()
        .filter(|r| valid_repo(r))
        .ok_or("the entry names no GitHub repository")?;
    let plain = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'+'))
    };
    if !plain(r.tag) || !super::plain_name(r.asset) {
        return Err(format!(
            "tag {:?} or file {:?} has characters the launcher cannot download: use letters, digits, '.', '-', '_' and '+'",
            r.tag, r.asset
        ));
    }
    let want = entry.kind.extension();
    if want.is_some_and(|e| !r.asset.ends_with(&format!(".{e}"))) {
        return Err(format!(
            "{:?} is not a .{} file",
            r.asset,
            want.unwrap_or("")
        ));
    }
    let facts = inspect(&entry.kind, path)?;
    let version = facts
        .version
        .clone()
        .unwrap_or_else(|| r.tag.trim_start_matches(['v', 'V']).to_owned());
    let mut notes: String = r.notes.trim().chars().take(MAX_CHANGELOG).collect();
    if r.notes.trim().chars().count() > MAX_CHANGELOG {
        notes.push_str("\n\n(Cut: the full notes are on the release.)");
    }
    let v = EntryVersion {
        game: facts.game.clone().unwrap_or_else(|| r.game.to_owned()),
        file: r.asset.to_owned(),
        sha256: super::sha256_file(path)?,
        signature: r
            .signature
            .map(str::to_owned)
            .filter(|s| !s.trim().is_empty()),
        changelog: notes,
        published: r.published.to_owned(),
        version,
    };
    let url = format!(
        "https://github.com/{repo}/releases/download/{}/{}",
        r.tag, r.asset
    );
    version_at(entry, &v, path, url)
}

pub fn page(entry: &Entry, text: &str) -> Result<Page, String> {
    let page: Page = toml::from_str(text).map_err(|e| format!("{PAGE_FILE}: {e}"))?;
    let errors = entry.resolved(&page).check_fields();
    if errors.is_empty() {
        Ok(page)
    } else {
        Err(errors.join("; "))
    }
}

impl Entry {
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|e| e.to_string())
    }
}

pub fn build(src: &Sources) -> Result<Index, Vec<String>> {
    let (mut entries, revoked) = load(src.registry)?;
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let mut errors = Vec::new();
    let mut projects = Vec::new();
    for entry in entries.iter().filter(|e| listed(e, src)) {
        match project(entry, src) {
            Ok(p) => projects.push(p),
            Err(e) => errors.extend(e),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let index = Index {
        schema: SCHEMA,
        generated: src.generated.clone(),
        projects,
        revoked,
    };
    let local = src.release.starts_with("file:///");
    index.check_with(local).map_err(|e| vec![e])?;
    Ok(index)
}

#[derive(Debug, Default, PartialEq)]
pub struct Review {
    pub errors: Vec<String>,
    pub notes: Vec<String>,
}

pub fn review(base: &[Entry], head: &[Entry], author: &str) -> Review {
    let mut r = Review::default();
    for old in base {
        let Some(new) = head.iter().find(|e| e.id == old.id) else {
            r.errors.push(format!(
                "{}: removed; take its files down in {REVOKED} instead",
                old.id
            ));
            continue;
        };
        if new == old {
            continue;
        }
        if !old.owners.iter().any(|o| o.eq_ignore_ascii_case(author)) {
            r.notes.push(format!(
                "{}: changed by {author}, who isn't one of its owners ({})",
                old.id,
                old.owners.join(", ")
            ));
        }
        if (&old.kind, &new.kind) == (&Kind::Mod, &Kind::NativeMod) {
            r.notes.push(format!(
                "{}: turns native; its .vmod versions stay published, and launchers keep the installed copies",
                old.id
            ));
        } else if new.kind != old.kind {
            r.errors.push(format!("{}: its kind can't change", old.id));
        }
        for v in &old.versions {
            match new.versions.iter().find(|w| w.version == v.version) {
                None => r.errors.push(format!(
                    "{} {}: removed; take it down in {REVOKED} instead",
                    old.id, v.version
                )),
                Some(w) if w.sha256 != v.sha256 || w.file != v.file => r.errors.push(format!(
                    "{} {}: a published file never changes: publish a new version",
                    old.id, v.version
                )),
                Some(_) => {}
            }
        }
        if new.repo != old.repo {
            r.notes.push(format!(
                "{}: its repository {} -> {}: new releases will come from the second; versions already read stay",
                old.id,
                old.repo.as_deref().unwrap_or("(none)"),
                new.repo.as_deref().unwrap_or("(none)")
            ));
        }
        if new.owners != old.owners {
            r.notes.push(format!(
                "{}: owners {} -> {}",
                old.id,
                old.owners.join(", "),
                new.owners.join(", ")
            ));
        }
        if new.key != old.key {
            r.notes.push(format!(
                "{}: its signing key changes; updates signed with the old one stop working",
                old.id
            ));
        }
        let added = new.versions.len().saturating_sub(old.versions.len());
        if added > 0 {
            r.notes.push(format!("{}: {added} new version(s)", old.id));
        }
    }
    for new in head {
        if !base.iter().any(|e| e.id == new.id) {
            r.notes.push(format!(
                "{}: a new {} by {}{}",
                new.id,
                new.kind.name(),
                new.owners.join(", "),
                new.repo.as_deref().map_or(String::new(), |repo| format!(
                    "; its releases come from https://github.com/{repo} with no further review: check the repository is the author\'s and read its source"
                ))
            ));
        }
    }
    r
}

#[cfg(test)]
mod tests;
