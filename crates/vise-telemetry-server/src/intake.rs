use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use rusqlite::params;
use serde_json::{Map, Value, json};

use crate::hub::{answer, json_body, refuse};
use crate::store::Store;
use crate::{App, github, take, utc_now};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS hub_queue (
    id INTEGER PRIMARY KEY, kind TEXT NOT NULL, day INTEGER NOT NULL, created TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'new', issue TEXT NOT NULL DEFAULT '', body TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS hub_queue_kind ON hub_queue (kind, state, id);
";

const RETRY_DAYS: u32 = 7;

struct Field(&'static str, bool, std::ops::RangeInclusive<usize>, bool);

const REPORT: &[Field] = &[
    Field("title", true, 5..=120, false),
    Field("did", true, 3..=2000, true),
    Field("happened", true, 3..=2000, true),
    Field("expected", true, 3..=2000, true),
    Field("version", false, 0..=60, false),
    Field("system", true, 3..=200, false),
    Field("name", false, 0..=64, false),
];

fn clean(text: &str, lines: bool) -> String {
    let text = text.replace("\r\n", "\n");
    let kept: String = text
        .chars()
        .map(|c| if !lines && c == '\n' { ' ' } else { c })
        .filter(|c| !c.is_control() || (lines && *c == '\n'))
        .collect();
    kept.trim().to_owned()
}

fn fields(body: &Value, rules: &[Field]) -> Result<Map<String, Value>, String> {
    let object = body.as_object().ok_or("send a JSON object")?;
    let mut out = Map::new();
    for Field(name, needed, length, lines) in rules {
        let text = match object.get(*name) {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => clean(s, *lines),
            Some(_) => return Err(format!("{name} is text")),
        };
        if text.is_empty() && !needed {
            continue;
        }
        if !length.contains(&text.chars().count()) {
            return Err(format!(
                "{name}: {} to {} characters",
                length.start(),
                length.end()
            ));
        }
        out.insert((*name).to_owned(), Value::String(text));
    }
    Ok(out)
}

impl Store {
    pub fn queued_on(&self, kind: &str, day: u32) -> rusqlite::Result<u32> {
        self.db.query_row(
            "SELECT count(*) FROM hub_queue WHERE kind = ?1 AND day = ?2",
            params![kind, day],
            |r| r.get(0),
        )
    }

    pub fn enqueue(
        &mut self,
        kind: &str,
        day: u32,
        since: u32,
        body: &Map<String, Value>,
    ) -> rusqlite::Result<(i64, Option<String>)> {
        let body = Value::Object(body.clone()).to_string();
        let same = self
            .db
            .prepare(
                "SELECT id, issue FROM hub_queue WHERE kind = ?1 AND body = ?2 AND day >= ?3 LIMIT 1",
            )?
            .query_row(params![kind, body, since], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            });
        match same {
            Ok((id, issue)) => return Ok((id, Some(issue).filter(|i| !i.is_empty()))),
            Err(rusqlite::Error::QueryReturnedNoRows) => {}
            Err(e) => return Err(e),
        }
        self.db.execute(
            "INSERT INTO hub_queue (kind, day, created, body) VALUES (?1, ?2, ?3, ?4)",
            params![kind, day, utc_now(), body],
        )?;
        Ok((self.db.last_insert_rowid(), None))
    }

    pub fn unfiled(&self, since: u32) -> rusqlite::Result<Vec<(i64, Map<String, Value>)>> {
        let mut statement = self.db.prepare(
            "SELECT id, body FROM hub_queue WHERE kind = 'report' AND state = 'new' AND day >= ?1 ORDER BY id",
        )?;
        let rows = statement.query_map([since], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                serde_json::from_str(&r.get::<_, String>(1)?).unwrap_or_default(),
            ))
        })?;
        rows.collect()
    }

    pub fn filed(&mut self, id: i64, issue: &str) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE hub_queue SET state = 'filed', issue = ?2 WHERE id = ?1",
            params![id, issue],
        )?;
        Ok(())
    }

    pub fn expire_queue(&mut self, oldest_day: u32, retry_from: u32) -> rusqlite::Result<usize> {
        self.db.execute(
            "UPDATE hub_queue SET state = 'failed' WHERE kind = 'report' AND state = 'new' AND day < ?1",
            [retry_from],
        )?;
        self.db
            .execute("DELETE FROM hub_queue WHERE day < ?1", [oldest_day])
    }
}

impl App {
    fn allow_post(&self, req: &Request) -> bool {
        take(
            &self.hub_post_buckets,
            self.config.hub_posts_per_hour,
            self.hashed(req),
        )
    }

    pub fn file_pending(&self) {
        let Some(filer) = &self.filer else { return };
        let since = (self.today)().saturating_sub(RETRY_DAYS);
        let pending = self.store().unfiled(since).unwrap_or_default();
        for (id, fields) in pending {
            match filer(&github::issue(&fields)) {
                Ok(url) => self.mark_filed(id, &url),
                Err(e) => {
                    eprintln!("vise-telemetry-server: report {id} still not opened: {e}");
                    return;
                }
            }
        }
    }

    fn mark_filed(&self, id: i64, url: &str) {
        let marked = self.store().filed(id, url);
        if let Err(e) = marked {
            eprintln!("vise-telemetry-server: report {id} opened as {url}, not marked: {e}");
        }
    }

    pub(crate) fn expire_intake(&self) -> rusqlite::Result<usize> {
        let today = (self.today)();
        self.store().expire_queue(
            today.saturating_sub(self.config.hub_keep_days),
            today.saturating_sub(RETRY_DAYS),
        )
    }
}

type Received = (i64, Option<String>, Map<String, Value>);

async fn receive(
    app: &App,
    req: Request,
    kind: &str,
    rules: &[Field],
    per_day: u32,
) -> Result<Received, Box<Response>> {
    let no = |code: StatusCode, message: &str| Box::new(refuse(app, code, message));
    if !app.allow_post(&req) {
        return Err(no(StatusCode::TOO_MANY_REQUESTS, "too many requests"));
    }
    let body = json_body(req)
        .await
        .map_err(|e| no(StatusCode::BAD_REQUEST, e))?;
    if body["website"].as_str().is_some_and(|s| !s.is_empty()) {
        let thanks = answer(app, StatusCode::ACCEPTED, json!({ "id": 0 }));
        return Err(Box::new(thanks));
    }
    let entry = fields(&body, rules).map_err(|e| no(StatusCode::BAD_REQUEST, &e))?;
    let today = (app.today)();
    let mut store = app.store();
    if store.queued_on(kind, today).unwrap_or(u32::MAX) >= per_day {
        let full = "too many today: please try again tomorrow";
        return Err(no(StatusCode::SERVICE_UNAVAILABLE, full));
    }
    match store.enqueue(kind, today, today.saturating_sub(1), &entry) {
        Ok((id, issue)) => Ok((id, issue, entry)),
        Err(e) => {
            eprintln!("vise-telemetry-server: could not queue a {kind}: {e}");
            Err(no(StatusCode::SERVICE_UNAVAILABLE, "try again later"))
        }
    }
}

pub async fn report(State(app): State<Arc<App>>, req: Request) -> Response {
    let Some(filer) = app.filer.clone() else {
        return refuse(
            &app,
            StatusCode::SERVICE_UNAVAILABLE,
            "reports are closed here: use GitHub or Discord",
        );
    };
    let per_day = app.config.hub_reports_per_day;
    let (id, issue, fields) = match receive(&app, req, "report", REPORT, per_day).await {
        Ok(received) => received,
        Err(response) => return *response,
    };
    if let Some(issue) = issue {
        return answer(
            &app,
            StatusCode::CREATED,
            json!({ "id": id, "issue": issue }),
        );
    }
    let opened = tokio::task::spawn_blocking(move || filer(&github::issue(&fields)))
        .await
        .map_err(|e| e.to_string())
        .and_then(|opened| opened);
    match opened {
        Ok(url) => {
            app.mark_filed(id, &url);
            answer(&app, StatusCode::CREATED, json!({ "id": id, "issue": url }))
        }
        Err(e) => {
            eprintln!("vise-telemetry-server: report {id} not opened yet: {e}");
            answer(
                &app,
                StatusCode::ACCEPTED,
                json!({ "id": id, "issue": null }),
            )
        }
    }
}
