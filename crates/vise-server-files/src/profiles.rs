use crate::{
    json::{self, Json},
    user_lists::NameAndId,
};
use vise_protocol::rsa::RsaPublicKey;
use std::time::Duration;

pub const DEFAULT_DISCOVERY: &str = "https://discovery.minecraftservices.com/minecraft/client";

pub fn discovery_url() -> String {
    std::env::var("VISE_API_DISCOVERY")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| DEFAULT_DISCOVERY.to_owned())
}

pub fn valid_player_name(name: &str) -> bool {
    name.encode_utf16().count() <= 16 && name.encode_utf16().all(|c| c > 32 && c < 127)
}

pub fn resolve(name: &str, online: bool) -> Option<NameAndId> {
    if online && valid_player_name(name) {
        find_by_name(&discovery_url(), &name.to_ascii_lowercase())
    } else {
        (!online).then(|| NameAndId::offline(name))
    }
}

pub fn find_by_name(discovery: &str, name: &str) -> Option<NameAndId> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent(&format!("Vise/{}", vise_core::release::VERSION))
        .build();
    let doc = json::parse(&agent.get(discovery).call().ok()?.into_string().ok()?, true).ok()?;
    let url = endpoint(&doc, "profiles", "getByName")?.replace("{name}", &name.to_lowercase());
    found(&agent.get(&url).call().ok()?.into_string().ok()?, name)
}

pub fn fetch_profile_key_keys(discovery: &str) -> Result<Vec<RsaPublicKey>, &'static str> {
    const FAILED: &str = "Failed to request Minecraft Services public key";
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent(&format!("Vise/{}", vise_core::release::VERSION))
        .build();
    let get = |url: &str| agent.get(url).call().ok()?.into_string().ok();
    let doc = get(discovery)
        .and_then(|d| json::parse(&d, true).ok())
        .ok_or(FAILED)?;
    let url = endpoint(&doc, "authentication", "getPublicKeys").ok_or(FAILED)?;
    profile_key_keys(&get(&url).ok_or(FAILED)?)
}

pub fn profile_key_keys(answer: &str) -> Result<Vec<RsaPublicKey>, &'static str> {
    use base64::Engine;
    const MALFORMED: &str = "Received malformed Minecraft Services public key data";
    let doc = json::parse(answer, true).map_err(|_| MALFORMED)?;
    let list = |name: &str| -> Result<Vec<RsaPublicKey>, &'static str> {
        let Some(entries) = doc.get(name) else {
            return Ok(Vec::new());
        };
        let Json::Array(entries) = entries else {
            return Err(MALFORMED);
        };
        entries
            .iter()
            .map(|e| {
                let text = e
                    .get("publicKey")
                    .and_then(Json::as_string)
                    .ok_or(MALFORMED)?;
                let der = base64::engine::general_purpose::STANDARD
                    .decode(text)
                    .map_err(|_| MALFORMED)?;
                RsaPublicKey::from_der(&der).map_err(|_| MALFORMED)
            })
            .collect()
    };
    list("profilePropertyKeys")?;
    list("playerCertificateKeys")
}

fn endpoint(doc: &Json, service: &str, name: &str) -> Option<String> {
    doc.get("discovery")?
        .get(service)?
        .get("endpoints")?
        .get(name)?
        .get("uri")?
        .as_string()
}

pub fn found(answer: &str, name: &str) -> Option<NameAndId> {
    let profile = json::parse(answer, true).ok()?;
    let found = profile.get("name")?.as_string()?;
    let id = profile.get("id")?.as_string()?.replace('-', "");
    (found.eq_ignore_ascii_case(name) && id.len() == 32).then_some(())?;
    Some(NameAndId {
        id: u128::from_str_radix(&id, 16).ok()?,
        name: found,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_answers() {
        assert!(valid_player_name("Alex_123"));
        assert!(!valid_player_name("two words"));
        assert!(!valid_player_name("ThisNameIsTooLong"));
        assert!(!valid_player_name("caf\u{e9}"));
        assert_eq!(resolve("Alex", false), Some(NameAndId::offline("Alex")));
        assert_eq!(resolve("not valid", true), None);
        let answer = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#;
        assert_eq!(
            found(answer, "notch"),
            Some(NameAndId {
                id: 0x069a79f444e94726a5befca90e38aaf5,
                name: "Notch".into()
            })
        );
        assert_eq!(found(answer, "Steve"), None);
        assert_eq!(found("{}", "Notch"), None);
        assert_eq!(found("", "Notch"), None);
    }

    #[test]
    fn services_key_sets() {
        let key = "MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQCrAmB0+SRQqPcm84lbsRF2yo8GXzEAzUFDusT302dO3P+VtBoFJdI2WlScEG7GaFiBs5IXdCD9Mf9cFE/xG6JDEedRzSxrXRgXtq9mhqQ8ldYSQ2EwBeaUW2CTzZUiPOxeiFaCbanS7hsyM2KsHOvqs2WcvEUxmptUDdqBakeY/wIDAQAB";
        let answer = format!(
            r#"{{"profilePropertyKeys":[{{"publicKey":"{key}"}}],"playerCertificateKeys":[{{"publicKey":"{key}"}},{{"publicKey":"{key}"}}]}}"#
        );
        let keys = profile_key_keys(&answer).unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].size(), 128);
        assert_eq!(profile_key_keys("{}").unwrap().len(), 0);
        let bad = r#"{"profilePropertyKeys":[{"publicKey":"AAAA"}],"playerCertificateKeys":[]}"#;
        assert!(profile_key_keys(bad).is_err());
        assert!(profile_key_keys("not json").is_err());
    }
}
