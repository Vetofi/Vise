use super::Game;
use vise_client::{Result, text::Text};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub(super) fn spans(text: &Text) -> Value {
    let mut out: Vec<(String, Value)> = Vec::new();
    for span in &text.spans {
        if span.text.is_empty() {
            continue;
        }
        let s = &span.style;
        let style = json!([
            s.color,
            s.bold == Some(true),
            s.italic == Some(true),
            s.underlined == Some(true),
            s.strikethrough == Some(true),
            s.obfuscated == Some(true),
        ]);
        match out.last_mut() {
            Some((t, last)) if *last == style => t.push_str(&span.text),
            _ => out.push((span.text.clone(), style)),
        }
    }
    Value::Array(out.into_iter().map(|(t, s)| json!([t, s])).collect())
}

impl Game<'_> {
    pub(super) fn wait_command_harness(&mut self) -> Result<()> {
        let server = self.integrated.take().ok_or("command harness: no server")?;
        while !server.stopped() {
            self.poll_stationary_client()?;
            std::thread::sleep(Duration::from_millis(2));
        }
        server.stop()
    }

    fn harness_owner<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let owner = self
            .serving
            .as_ref()
            .ok_or("command harness: not a server")?
            .owner;
        let mut host = self.lan.take().ok_or("command harness: no hub")?;
        let guest = host.hub.guests.iter().find(|g| g.uuid == owner && g.placed);
        let Some(guest) = guest else {
            self.lan = Some(host);
            return Err("command harness: owner is not connected".into());
        };
        let (conn, dim, back) = (guest.conn, super::Dim::from_id(&guest.dimension), self.dim);
        if let Err(error) = self.enter_level(Some(&mut host), dim) {
            self.lan = Some(host);
            return Err(error);
        }
        let swap = self
            .enter_guest(&mut host, conn)
            .expect("the owner is not already acting");
        self.lan = Some(host);
        let result = f(self);
        let mut host = self.lan.take().expect("the operation puts the hub back");
        self.leave_guest(&mut host, swap);
        host.hub.sync_all_effects();
        self.run_level_teleports(&mut host);
        let restored = self.enter_level(Some(&mut host), back);
        self.lan = Some(host);
        restored?;
        result
    }

    fn harness_portal(&mut self) -> Result<()> {
        let (conn, plan) = self.harness_owner(|g| {
            let p = g
                .player
                .as_ref()
                .ok_or("command harness: portal without a player")?;
            let entry = vise_core::position::BlockPos::containing(p.position);
            let block = g.world.block(entry.x, entry.y, entry.z);
            if &*g.res.facts().get(block).name != "minecraft:nether_portal" {
                return Err("command harness: the player is not in a nether portal".into());
            }
            let plan = g.portal_plan(
                entry,
                p.position,
                p.dimensions.width,
                p.dimensions.height,
                true,
            );
            Ok((
                g.lan_acting.ok_or("command harness: no acting owner")?,
                plan,
            ))
        })?;
        self.in_owner_level(|g| {
            let mut host = g.lan.take().ok_or("command harness: no hub")?;
            g.start_server_trip(&mut host, conn, plan.dim, Some(plan));
            g.lan = Some(host);
            Ok(())
        })
    }
    fn harness_settle(&mut self, deadline: Instant) -> Result<()> {
        loop {
            self.update_world();
            if let Some(p) = &self.player {
                let (cx, cz) = (
                    (p.position.x.floor() as i32) >> 4,
                    (p.position.z.floor() as i32) >> 4,
                );
                if !self.streaming.busy()
                    && (-1..=1).all(|dz| (-1..=1).all(|dx| self.streaming.is_lit(cx + dx, cz + dz)))
                {
                    self.spawn_loaded_entities();
                    return Ok(());
                }
            }
            if std::env::var_os("VISE_HARNESS_DEBUG").is_some() {
                eprintln!(
                    "settle: player {:?} center {:?} chunks {} pending {}",
                    self.player.as_ref().map(|p| p.position),
                    self.streaming.center,
                    self.world.chunks.len(),
                    self.streaming.pending()
                );
            }
            if Instant::now() > deadline {
                return Err("command harness: terrain did not load".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    pub(super) fn run_command_harness(&mut self, script: &Path, output: &Path) -> Result<()> {
        let deadline = Instant::now()
            + Duration::from_secs(
                std::env::var("VISE_HARNESS_TIMEOUT")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(600),
            );
        while !self
            .lan
            .as_ref()
            .is_some_and(|h| h.hub.guests.iter().any(|g| g.placed && g.loaded))
        {
            self.tick_server(true)?;
            if Instant::now() > deadline {
                return Err("command harness: owner did not connect".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.harness_owner(|g| g.harness_settle(deadline))?;
        let mut out = String::new();
        for line in std::fs::read_to_string(script)?.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let op: Value = serde_json::from_str(line)?;
            match op["op"].as_str().unwrap_or("") {
                "cmd" => {
                    let command = op["cmd"].as_str().unwrap_or("");
                    self.chat_capture = Some(Vec::new());
                    self.cmds.results = Some(Vec::new());
                    self.harness_owner(|g| {
                        g.as_console(|g| {
                            g.run_command(command.strip_prefix('/').unwrap_or(command))
                        });
                        Ok(())
                    })?;
                    let messages = self.chat_capture.take().unwrap_or_default();
                    let results = self.cmds.results.take().unwrap_or_default();
                    if op["record"].as_bool() != Some(false) {
                        let record = json!({
                            "cmd": command,
                            "messages": messages.iter().map(spans).collect::<Vec<_>>(),
                            "results": results.iter().map(|(s, v)| json!([s, v])).collect::<Vec<_>>(),
                        });
                        out.push_str(&record.to_string());
                        out.push('\n');
                    }
                }
                "tick" => {
                    self.chat_capture = Some(Vec::new());
                    for _ in 0..op["n"].as_u64().unwrap_or(1) {
                        self.tick_server(false)?;
                    }
                    let messages = self.chat_capture.take().unwrap_or_default();
                    if op["record"].as_bool() == Some(true) {
                        let record = json!({
                            "cmd": "#tick",
                            "messages": messages.iter().map(spans).collect::<Vec<_>>(),
                            "results": [],
                        });
                        out.push_str(&record.to_string());
                        out.push('\n');
                    }
                }
                "settle" => {
                    while !self.server_trips.is_empty() {
                        self.tick_server(true)?;
                        if Instant::now() > deadline {
                            return Err("command harness: portal did not arrive".into());
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    self.harness_owner(|g| g.harness_settle(deadline))?;
                }
                "portal" => self.harness_portal()?,
                other => return Err(format!("command harness: unknown op {other}").into()),
            }
        }
        std::fs::write(output, out)?;
        Ok(())
    }
}
