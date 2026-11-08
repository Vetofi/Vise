
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};

pub const SUMS: &str = "SHA256SUMS";
pub const SIGNATURE: &str = "SHA256SUMS.sig";

const KEYS: &str = include_str!("../release-keys.txt");
const LEGACY: &str = include_str!("../legacy-releases.txt");

pub type PublicKey = [u8; 32];

pub fn trusted_keys() -> Vec<PublicKey> {
    parse_keys(KEYS).unwrap_or_default()
}

pub fn parse_keys(text: &str) -> Result<Vec<PublicKey>, String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let bytes = STANDARD
                .decode(line)
                .map_err(|e| format!("{line}: not base64 ({e})"))?;
            PublicKey::try_from(bytes.as_slice())
                .map_err(|_| format!("{line}: not a 32-byte Ed25519 public key"))
        })
        .collect()
}

pub fn message(tag: &str, sums: &[u8]) -> Vec<u8> {
    let mut message = format!("vise release {tag}\n").into_bytes();
    message.extend_from_slice(sums);
    message
}

pub fn sums_file(files: &[(String, String)]) -> String {
    files
        .iter()
        .map(|(name, hex)| format!("{hex}  {name}\n"))
        .collect()
}

pub fn parse_sums(text: &str) -> Result<Vec<(String, String)>, String> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (hex, name) = line
                .split_once(' ')
                .ok_or_else(|| format!("{SUMS}: {line:?} is not `<sha256>  <file>`"))?;
            let name = name.strip_prefix([' ', '*']).unwrap_or(name);
            if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) || name.is_empty() {
                return Err(format!("{SUMS}: {line:?} is not `<sha256>  <file>`"));
            }
            Ok((name.to_string(), hex.to_ascii_lowercase()))
        })
        .collect()
}

pub fn verify(
    tag: &str,
    sums: &[u8],
    signature: &str,
    keys: &[PublicKey],
) -> Result<Vec<(String, String)>, String> {
    if keys.is_empty() {
        return Err(
            "This build has no release key built in, so it can't check downloads: none are installed."
                .into(),
        );
    }
    let message = message(tag, sums);
    let signed = signature
        .lines()
        .filter_map(|line| STANDARD.decode(line.trim()).ok())
        .any(|signature| {
            keys.iter().any(|key| {
                UnparsedPublicKey::new(&ED25519, key)
                    .verify(&message, &signature)
                    .is_ok()
            })
        });
    if !signed {
        return Err(format!(
            "{tag}'s signature doesn't match Vise's release key, so it was not installed. \
             The download may have been tampered with: please report it on Discord or GitHub."
        ));
    }
    let text = std::str::from_utf8(sums).map_err(|_| format!("{SUMS} is not text"))?;
    parse_sums(text)
}

pub const NIGHTLY: &str = "nightly";

pub fn is_nightly(tag: &str) -> bool {
    tag == NIGHTLY || tag.starts_with("nightly-")
}

pub fn nightly_version(title: &str, published: &str) -> String {
    let mut words = title
        .strip_prefix("Nightly ")
        .unwrap_or_default()
        .split(" · ");
    let date = words
        .next()
        .filter(|d| is_date(d))
        .unwrap_or_else(|| published.get(..10).unwrap_or(published));
    let commit = words
        .next()
        .filter(|c| !c.is_empty() && c.chars().all(|c| c.is_ascii_hexdigit()));
    match commit {
        Some(commit) => format!("{NIGHTLY}-{date}-{commit}"),
        None => format!("{NIGHTLY}-{date}"),
    }
}

pub fn nightly_date(version: &str) -> Option<&str> {
    let date = version.strip_prefix("nightly-")?.get(..10)?;
    is_date(date).then_some(date)
}

fn is_date(text: &str) -> bool {
    text.len() == 10
        && text.chars().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        })
}

pub fn signed_digest(
    tag: &str,
    asset: &str,
    sums: Option<&[u8]>,
    signature: Option<&str>,
    keys: &[PublicKey],
) -> Result<String, String> {
    if is_nightly(tag) {
        return Err(format!(
            "{tag} is a Nightly build: only a launcher with Nightly turned on installs it."
        ));
    }
    digest_in(tag, tag, asset, sums, signature, keys)
}

pub fn nightly_digest(
    asset: &str,
    sums: Option<&[u8]>,
    signature: Option<&str>,
    keys: &[PublicKey],
) -> Result<String, String> {
    digest_in(NIGHTLY, "The Nightly build", asset, sums, signature, keys)
}

fn digest_in(
    signed_for: &str,
    name: &str,
    asset: &str,
    sums: Option<&[u8]>,
    signature: Option<&str>,
    keys: &[PublicKey],
) -> Result<String, String> {
    let (Some(sums), Some(signature)) = (sums, signature) else {
        return Err(format!(
            "{name} is not signed, and Vise installs only signed releases: pick a newer version."
        ));
    };
    verify(signed_for, sums, signature, keys)?
        .into_iter()
        .find(|(n, _)| n == asset)
        .map(|(_, hex)| hex)
        .ok_or_else(|| {
            format!("{asset} is not in {name}'s signed checksums, so it was not installed.")
        })
}

pub fn legacy_digest(tag: &str, asset: &str) -> Option<&'static str> {
    legacy_entries(LEGACY).find_map(|(t, a, hex)| (t == tag && a == asset).then_some(hex))
}

fn legacy_entries(text: &str) -> impl Iterator<Item = (&str, &str, &str)> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some((parts.next()?, parts.next()?, parts.next()?))
        })
}

pub fn check(asset: &str, expected: &str, got: &str) -> Result<(), String> {
    if expected.eq_ignore_ascii_case(got) {
        Ok(())
    } else {
        Err(format!(
            "{asset} doesn't match its signed checksum (corrupted or tampered with), so it was \
             not installed. Try again."
        ))
    }
}

pub struct Sha256(ring::digest::Context);

impl Default for Sha256 {
    fn default() -> Self {
        Self(ring::digest::Context::new(&ring::digest::SHA256))
    }
}

impl Sha256 {
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub fn hex(self) -> String {
        self.0
            .finish()
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hash = Sha256::default();
    hash.update(bytes);
    hash.hex()
}

pub struct SigningKey(Ed25519KeyPair);

impl SigningKey {
    pub fn generate() -> Result<(Self, String), String> {
        const V2_PREFIX: [u8; 16] = [
            0x30, 0x51, 0x02, 0x01, 0x01, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        const V1_PREFIX: [u8; 16] = [
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        let v2 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
            .map_err(|_| "no randomness to make a key with".to_string())?;
        let seed = v2
            .as_ref()
            .strip_prefix(&V2_PREFIX)
            .and_then(|rest| rest.get(..32))
            .ok_or("ring's key document has an unexpected layout")?;
        let v1 = [&V1_PREFIX[..], seed].concat();
        let key = Ed25519KeyPair::from_pkcs8_maybe_unchecked(&v1)
            .map_err(|_| "the new key doesn't parse".to_string())?;
        let same = Ed25519KeyPair::from_pkcs8(v2.as_ref())
            .is_ok_and(|k| k.public_key().as_ref() == key.public_key().as_ref());
        if !same {
            return Err("the new key's two forms disagree".into());
        }
        Ok((Self(key), STANDARD.encode(&v1)))
    }

    pub fn from_base64(text: &str) -> Result<Self, String> {
        let document = STANDARD
            .decode(text.trim())
            .map_err(|_| "the signing key is not base64".to_string())?;
        Ed25519KeyPair::from_pkcs8_maybe_unchecked(&document)
            .map(Self)
            .map_err(|_| "the signing key is not an Ed25519 PKCS#8 key".to_string())
    }

    pub fn public(&self) -> PublicKey {
        PublicKey::try_from(self.0.public_key().as_ref()).expect("Ed25519 public keys are 32 bytes")
    }

    pub fn public_base64(&self) -> String {
        STANDARD.encode(self.public())
    }

    pub fn sign(&self, tag: &str, sums: &[u8]) -> String {
        STANDARD.encode(self.0.sign(&message(tag, sums)).as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(tag: &str) -> (SigningKey, String, String) {
        let (key, _) = SigningKey::generate().unwrap();
        let sums = sums_file(&[
            ("vise-linux-x86_64.tar.gz".into(), sha256_hex(b"linux")),
            ("vise-windows-x64.zip".into(), sha256_hex(b"windows")),
        ]);
        let signature = key.sign(tag, sums.as_bytes()) + "\n";
        (key, sums, signature)
    }

    #[test]
    fn the_built_in_keys_file_parses() {
        assert!(parse_keys(KEYS).is_ok());
    }

    #[test]
    fn releases_from_before_signing_keep_their_pinned_digests() {
        let mut seen = std::collections::HashSet::new();
        for line in LEGACY.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            assert_eq!(parts.len(), 3, "{line}");
            assert_eq!(parts[2].len(), 64, "{line}");
            assert!(
                parts[2]
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                "{line}"
            );
            assert!(seen.insert((parts[0], parts[1])), "listed twice: {line}");
        }
        if let Some((tag, asset, hex)) = legacy_entries(LEGACY).next() {
            assert_eq!(legacy_digest(tag, asset), Some(hex));
            assert_eq!(legacy_digest(tag, "not-an-asset.zip"), None);
        }
        assert_eq!(
            legacy_digest("v2099.01.01", "vise-windows-x64.zip"),
            None
        );
    }

    #[test]
    fn a_signed_release_gives_its_digests() {
        let (key, sums, signature) = signed("v2026.10.19");
        let digest = signed_digest(
            "v2026.10.19",
            "vise-windows-x64.zip",
            Some(sums.as_bytes()),
            Some(&signature),
            &[key.public()],
        );
        assert_eq!(digest, Ok(sha256_hex(b"windows")));
        assert!(check("x", &sha256_hex(b"windows"), &sha256_hex(b"windows")).is_ok());
        assert!(check("x", &sha256_hex(b"windows"), &sha256_hex(b"linux")).is_err());
    }

    #[test]
    fn nightly_and_release_signatures_never_pass_for_each_other() {
        let (nightly, sums, signature) = signed(NIGHTLY);
        let keys = [nightly.public()];
        let asset = "vise-windows-x64.zip";
        let digest = nightly_digest(asset, Some(sums.as_bytes()), Some(&signature), &keys);
        assert_eq!(digest, Ok(sha256_hex(b"windows")));
        for tag in ["v0.2.81", "v2026.10.28", "0.2.81"] {
            let as_release =
                signed_digest(tag, asset, Some(sums.as_bytes()), Some(&signature), &keys);
            assert!(as_release.is_err(), "{tag} took a nightly's signature");
        }
        for tag in [NIGHTLY, "nightly-2026-10-27"] {
            let err = signed_digest(tag, asset, Some(sums.as_bytes()), Some(&signature), &keys);
            assert!(err.unwrap_err().contains("Nightly build"), "{tag}");
        }
        let (release, sums, signature) = signed("v0.2.81");
        let keys = [release.public()];
        assert!(nightly_digest(asset, Some(sums.as_bytes()), Some(&signature), &keys).is_err());
        assert!(nightly_digest(asset, Some(sums.as_bytes()), None, &keys).is_err());
        assert!(is_nightly("nightly-2026-10-27") && !is_nightly("v0.2.81"));
    }

    #[test]
    fn a_nightly_is_named_by_its_date_and_commit() {
        let v = nightly_version("Nightly 2026-10-28 · 0c02067", "2026-10-28T20:40:00Z");
        assert_eq!(v, "nightly-2026-10-28-0c02067");
        assert_eq!(
            nightly_version("Something else", "2026-10-29T01:00:00Z"),
            "nightly-2026-10-29"
        );
        assert_eq!(nightly_date(&v), Some("2026-10-28"));
        assert_eq!(nightly_date("nightly-2026-10-27"), Some("2026-10-27"));
        assert_eq!(nightly_date("nightly"), None);
        assert_eq!(nightly_date("v0.2.95"), None);
        assert!(is_nightly(&v));
    }

    #[test]
    fn tampering_is_refused() {
        let (key, sums, signature) = signed("v2026.10.19");
        let keys = [key.public()];
        assert!(verify("v2026.10.20", sums.as_bytes(), &signature, &keys).is_err());
        let changed = sums.replacen(&sha256_hex(b"linux"), &sha256_hex(b"evil"), 1);
        assert!(verify("v2026.10.19", changed.as_bytes(), &signature, &keys).is_err());
        let (other, _) = SigningKey::generate().unwrap();
        assert!(
            verify(
                "v2026.10.19",
                sums.as_bytes(),
                &signature,
                &[other.public()]
            )
            .is_err()
        );
        let none = signed_digest("v2026.10.19", "a", None, None, &keys).unwrap_err();
        assert!(none.contains("not signed"), "{none}");
        assert!(verify("v2026.10.19", sums.as_bytes(), &signature, &[]).is_err());
        let missing = signed_digest(
            "v2026.10.19",
            "vise-macos-arm64.zip",
            Some(sums.as_bytes()),
            Some(&signature),
            &keys,
        );
        assert!(missing.unwrap_err().contains("not in"));
    }

    #[test]
    fn any_line_signed_by_any_trusted_key_passes() {
        let (old, sums, old_line) = signed("v1");
        let (new, _) = SigningKey::generate().unwrap();
        let both = format!("{old_line}{}\n", new.sign("v1", sums.as_bytes()));
        assert!(verify("v1", sums.as_bytes(), &both, &[old.public()]).is_ok());
        assert!(verify("v1", sums.as_bytes(), &both, &[new.public()]).is_ok());
        assert!(verify("v1", sums.as_bytes(), "garbage\n\n", &[old.public()]).is_err());
    }

    #[test]
    fn keys_round_trip_through_their_text_forms() {
        let (key, secret) = SigningKey::generate().unwrap();
        let document = STANDARD.decode(&secret).unwrap();
        assert_eq!((document.len(), document[4]), (48, 0));
        let again = SigningKey::from_base64(&format!("{secret}\n")).unwrap();
        assert_eq!(again.public(), key.public());
        assert_eq!(
            parse_keys(&format!("# comment\n\n{}\n", key.public_base64())),
            Ok(vec![key.public()])
        );
        assert!(parse_keys("AAAA\n").is_err());
        let error = SigningKey::from_base64("bm90IGEga2V5").err().unwrap();
        assert!(!error.contains("bm90"));
    }

    #[test]
    fn checksum_lists_read_both_sha256sum_forms() {
        let hex = sha256_hex(b"x");
        let list =
            parse_sums(&format!("{hex}  a.zip\n{} *b.tar.gz\n", hex.to_uppercase())).unwrap();
        assert_eq!(
            list,
            [("a.zip".into(), hex.clone()), ("b.tar.gz".into(), hex)]
        );
        assert!(parse_sums("nothex  a.zip\n").is_err());
    }
}
