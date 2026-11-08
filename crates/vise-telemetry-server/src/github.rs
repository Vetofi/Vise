use std::time::Duration;

use serde_json::{Map, Value, json};

pub type Filer = dyn Fn(&Issue) -> Result<String, String> + Send + Sync;

pub struct Issue {
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
}

pub const LABELS: [&str; 2] = ["bug", "from-website"];

pub fn neutralise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let word_start = i == 0 || !chars[i - 1].is_alphanumeric();
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '`' => out.push('\''),
            '@' | '#' if next.is_some_and(|n| n.is_alphanumeric()) => {
                out.push(c);
                out.push('\u{200b}');
            }
            _ if word_start && closing_keyword(&chars[i..]) => {
                out.push(c);
                out.push('\u{200b}');
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out.replace("github.com/", "github.com\u{200b}/")
}

fn closing_keyword(rest: &[char]) -> bool {
    let word: String = rest
        .iter()
        .take_while(|c| c.is_alphabetic())
        .flat_map(|c| c.to_lowercase())
        .collect();
    [
        "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved",
    ]
    .contains(&word.as_str())
}

fn quote(text: &str) -> String {
    text.lines()
        .map(|l| format!("> {l}").trim_end().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn issue(fields: &Map<String, Value>) -> Issue {
    let field = |name: &str| {
        fields
            .get(name)
            .and_then(Value::as_str)
            .map(neutralise)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "not given".into())
    };
    let title = neutralise(&field("title").replace(['\n', '\r'], " "));
    let text = [
        format!("**What I did:**\n{}", field("did")),
        format!("**What happened:**\n{}", field("happened")),
        format!("**What I expected:**\n{}", field("expected")),
        format!("**Vise version:** {}", field("version")),
        format!("**OS, CPU, GPU:** {}", field("system")),
    ]
    .join("\n");
    let by = fields
        .get("name")
        .and_then(Value::as_str)
        .map(|n| neutralise(n).replace(['*', '_', '~', '|', '[', ']'], ""))
        .filter(|n| !n.is_empty())
        .map(|n| format!(" by **{n}**"))
        .unwrap_or_default();
    let body = format!(
        "{}\n\nReported on playvise.dev{by}\n\n<sub>Opened by the bug form on playvise.dev/hub. The reporter has this issue's link; they don't get GitHub notifications.</sub>",
        quote(&text)
    );
    Issue {
        title,
        body,
        labels: LABELS.iter().map(|l| (*l).to_owned()).collect(),
    }
}

pub fn github(repo: String, token: String) -> Box<Filer> {
    Box::new(move |issue: &Issue| {
        let response = ureq::post(&format!("https://api.github.com/repos/{repo}/issues"))
            .set("Authorization", &format!("Bearer {token}"))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .set("User-Agent", "vise-telemetry-server")
            .timeout(Duration::from_secs(20))
            .send_json(json!({ "title": issue.title, "body": issue.body, "labels": issue.labels }));
        match response {
            Ok(response) => response
                .into_json::<Value>()
                .ok()
                .and_then(|v| v["html_url"].as_str().map(str::to_owned))
                .ok_or_else(|| "GitHub's answer had no issue URL".into()),
            Err(ureq::Error::Status(code, _)) => Err(format!("GitHub answered {code}")),
            Err(ureq::Error::Transport(e)) => Err(format!("could not reach GitHub: {}", e.kind())),
        }
    })
}
