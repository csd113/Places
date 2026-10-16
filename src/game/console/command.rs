//! Closed typed command vocabulary; adding a command starts in this registry.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Help,
    Clear,
    Noclip,
    Give,
}

pub struct CommandSpec {
    pub kind: CommandKind,
    pub name: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
}

pub const REGISTRY: [CommandSpec; 4] = [
    CommandSpec {
        kind: CommandKind::Help,
        name: "help",
        usage: "/help",
        description: "List commands and controls.",
    },
    CommandSpec {
        kind: CommandKind::Clear,
        name: "clear",
        usage: "/clear",
        description: "Clear console output.",
    },
    CommandSpec {
        kind: CommandKind::Noclip,
        name: "noclip",
        usage: "/noclip [on|off]",
        description: "Fly through geometry; exit restores safe walking.",
    },
    CommandSpec {
        kind: CommandKind::Give,
        name: "give",
        usage: "/give <catalog-id>",
        description: "Place a physical item or clearly labelled visual preview ahead.",
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Clear,
    Noclip(Option<bool>),
    Give(String),
}

/// Parse one command, never a script, path or shell expression.
/// # Errors
/// Reports missing slash, unknown command or invalid arguments.
pub fn parse(line: &str) -> Result<Command, String> {
    let mut words = line.split_whitespace();
    let name = words
        .next()
        .and_then(|word| word.strip_prefix('/'))
        .ok_or_else(|| "Commands start with /. Try /help.".to_string())?;
    let spec = REGISTRY
        .iter()
        .find(|spec| spec.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("Unknown command /{name}. Try /help."))?;
    let argument = words.next();
    if words.next().is_some() {
        return Err(format!("Usage: {}", spec.usage));
    }
    match (spec.kind, argument) {
        (CommandKind::Help, None) => Ok(Command::Help),
        (CommandKind::Clear, None) => Ok(Command::Clear),
        (CommandKind::Noclip, None) => Ok(Command::Noclip(None)),
        (CommandKind::Noclip, Some("on")) => Ok(Command::Noclip(Some(true))),
        (CommandKind::Noclip, Some("off")) => Ok(Command::Noclip(Some(false))),
        (CommandKind::Give, Some(id))
            if id.len() <= 128
                && id
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '_' | '-')) =>
        {
            Ok(Command::Give(id.to_string()))
        }
        (CommandKind::Help | CommandKind::Clear | CommandKind::Noclip | CommandKind::Give, _) => {
            Err(format!("Usage: {}", spec.usage))
        }
    }
}
