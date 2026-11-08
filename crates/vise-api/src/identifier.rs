
use std::{fmt, str::FromStr};

pub const DEFAULT_NAMESPACE: &str = "minecraft";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentifierError {
    EmptyNamespace,
    EmptyPath,
    BadChar {
        part: &'static str,
        ch: char,
    },
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyNamespace => f.write_str("identifier namespace is empty"),
            Self::EmptyPath => f.write_str("identifier path is empty"),
            Self::BadChar { part, ch } => {
                write!(f, "identifier {part} has invalid character {ch:?}")
            }
        }
    }
}
impl std::error::Error for IdentifierError {}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Identifier {
    namespace: Box<str>,
    path: Box<str>,
}

fn check(part: &'static str, s: &str, slash: bool) -> Result<(), IdentifierError> {
    for ch in s.chars() {
        let ok = ch.is_ascii_lowercase()
            || ch.is_ascii_digit()
            || matches!(ch, '_' | '.' | '-')
            || (slash && ch == '/');
        if !ok {
            return Err(IdentifierError::BadChar { part, ch });
        }
    }
    Ok(())
}

impl Identifier {
    pub fn new(namespace: &str, path: &str) -> Result<Self, IdentifierError> {
        if namespace.is_empty() {
            return Err(IdentifierError::EmptyNamespace);
        }
        if path.is_empty() {
            return Err(IdentifierError::EmptyPath);
        }
        check("namespace", namespace, false)?;
        check("path", path, true)?;
        Ok(Self {
            namespace: namespace.into(),
            path: path.into(),
        })
    }

    #[track_caller]
    pub fn of(namespace: &str, path: &str) -> Self {
        Self::new(namespace, path).unwrap_or_else(|e| panic!("{namespace}:{path}: {e}"))
    }

    #[track_caller]
    pub fn vanilla(path: &str) -> Self {
        Self::of(DEFAULT_NAMESPACE, path)
    }

    pub fn parse(s: &str) -> Result<Self, IdentifierError> {
        match s.split_once(':') {
            Some((ns, path)) => Self::new(ns, path),
            None => Self::new(DEFAULT_NAMESPACE, s),
        }
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}
impl fmt::Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Identifier({self})")
    }
}
impl FromStr for Identifier {
    type Err = IdentifierError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_displays() {
        let id: Identifier = "mymod:blocks/ruby".parse().unwrap();
        assert_eq!((id.namespace(), id.path()), ("mymod", "blocks/ruby"));
        assert_eq!(id.to_string(), "mymod:blocks/ruby");
        assert_eq!(
            Identifier::parse("stone").unwrap(),
            Identifier::vanilla("stone")
        );
    }

    #[test]
    fn rejects_bad_names() {
        assert_eq!(Identifier::parse("a:"), Err(IdentifierError::EmptyPath));
        assert_eq!(
            Identifier::parse(":a"),
            Err(IdentifierError::EmptyNamespace)
        );
        assert!(matches!(
            Identifier::parse("Mod:a"),
            Err(IdentifierError::BadChar { .. })
        ));
        assert!(matches!(
            Identifier::parse("a/b:c"),
            Err(IdentifierError::BadChar { .. })
        ));
    }
}
