
use crate::{event::Event, events::PlayerRef};
use std::{
    fmt,
    sync::{Arc, LazyLock},
};

pub static COMMAND_REGISTRATION: LazyLock<Event<CommandRegistry>> = LazyLock::new(Event::new);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    BadUsage(String),
    Duplicate(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadUsage(u) => write!(
                f,
                "command usage {u:?} must start with a name of a-z 0-9 _ -"
            ),
            Self::Duplicate(n) => write!(f, "command {n} is already registered"),
        }
    }
}
impl std::error::Error for CommandError {}

pub struct CommandContext {
    pub player: PlayerRef,
    pub args: String,
    feedback: Vec<String>,
}

impl CommandContext {
    pub fn words(&self) -> Vec<&str> {
        self.args.split_whitespace().collect()
    }
    pub fn arg(&self, index: usize) -> Option<&str> {
        self.args.split_whitespace().nth(index)
    }
    pub fn reply(&mut self, message: impl Into<String>) {
        self.feedback.push(message.into());
    }
}

type Executor = dyn Fn(&mut CommandContext) + Send + Sync;

#[derive(Clone)]
pub struct Command {
    name: String,
    usage: String,
    level: u8,
    run: Option<Arc<Executor>>,
}

impl Command {
    pub fn new(usage: &str) -> Result<Self, CommandError> {
        let usage = usage.trim();
        let name = usage.split_whitespace().next().unwrap_or("");
        let ok = !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
        if !ok {
            return Err(CommandError::BadUsage(usage.into()));
        }
        Ok(Self {
            name: name.into(),
            usage: usage.into(),
            level: 0,
            run: None,
        })
    }
    pub fn level(mut self, level: u8) -> Self {
        self.level = level.min(4);
        self
    }
    pub fn executes(mut self, f: impl Fn(&mut CommandContext) + Send + Sync + 'static) -> Self {
        self.run = Some(Arc::new(f));
        self
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn usage(&self) -> &str {
        &self.usage
    }
}

#[derive(Clone, Default)]
pub struct CommandRegistry {
    commands: Vec<Command>,
}

impl CommandRegistry {
    pub fn register(&mut self, command: Command) -> Result<(), CommandError> {
        if self.commands.iter().any(|c| c.name == command.name) {
            return Err(CommandError::Duplicate(command.name));
        }
        self.commands.push(command);
        Ok(())
    }

    pub fn usages(&self) -> Vec<(String, u8)> {
        self.commands
            .iter()
            .map(|c| (c.usage.clone(), c.level))
            .collect()
    }

    pub fn run(&self, player: &PlayerRef, input: &str) -> Option<Vec<String>> {
        let input = input.trim();
        let (name, args) = input.split_once(char::is_whitespace).unwrap_or((input, ""));
        let command = self.commands.iter().find(|c| c.name == name)?;
        if player.permission < command.level {
            return None;
        }
        let mut ctx = CommandContext {
            player: player.clone(),
            args: args.trim().into(),
            feedback: Vec::new(),
        };
        if let Some(run) = &command.run {
            run(&mut ctx);
        }
        Some(ctx.feedback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identifier::Identifier;

    pub(crate) fn player(level: u8) -> PlayerRef {
        PlayerRef {
            uuid: 1,
            name: "Steve".into(),
            permission: level,
            dimension: Identifier::vanilla("overworld"),
            position: [0.0; 3],
            game_mode: 0,
        }
    }

    #[test]
    fn runs_with_args_and_checks_level() {
        let mut r = CommandRegistry::default();
        r.register(
            Command::new("heal [amount:int]")
                .unwrap()
                .level(2)
                .executes(|c| {
                    let line = format!("{} heals {}", c.player.name, c.arg(0).unwrap_or("all"));
                    c.reply(line);
                }),
        )
        .unwrap();
        assert_eq!(r.usages(), [("heal [amount:int]".to_string(), 2)]);
        assert_eq!(
            r.run(&player(2), "heal 5"),
            Some(vec!["Steve heals 5".to_string()])
        );
        assert_eq!(r.run(&player(0), "heal 5"), None);
        assert_eq!(r.run(&player(4), "nope"), None);
    }

    #[test]
    fn refuses_bad_usage_and_duplicates() {
        assert!(Command::new("  ").is_err());
        assert!(Command::new("/heal").is_err());
        let mut r = CommandRegistry::default();
        r.register(Command::new("a").unwrap()).unwrap();
        assert_eq!(
            r.register(Command::new("a <x:int>").unwrap()),
            Err(CommandError::Duplicate("a".into()))
        );
    }
}
