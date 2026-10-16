//! Closed console vocabulary translated into reusable typed gameplay requests.

use crate::game::gameplay_actions::{
    GameplayAction, PhysicsAction, RemovalScope, RepresentationAction,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Help,
    Clear,
    Noclip,
    Give,
    Respawn,
    Position,
    Remove,
    Physics,
    Time,
    Trigger,
    Become,
}

pub struct CommandSpec {
    pub kind: CommandKind,
    pub name: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
    pub help: &'static [&'static str],
}

pub const REGISTRY: &[CommandSpec] = &[
    CommandSpec {
        kind: CommandKind::Help,
        name: "help",
        usage: "/help [command]",
        description: "List commands or show command help.",
        help: &["Every command also accepts help, for example /give help."],
    },
    CommandSpec {
        kind: CommandKind::Clear,
        name: "clear",
        usage: "/clear",
        description: "Clear console output.",
        help: &[
            "History is retained. Up/Down recalls commands; PageUp/PageDown or wheel scrolls output.",
        ],
    },
    CommandSpec {
        kind: CommandKind::Noclip,
        name: "noclip",
        usage: "/noclip [on|off]",
        description: "Toggle collision-free flight.",
        help: &[
            "Move/look normally; Jump rises and Crouch descends. Drop held items first.",
            "Disabling retains a clear airborne pose and restores gravity. Solid/void exits use a validated safe return.",
        ],
    },
    CommandSpec {
        kind: CommandKind::Give,
        name: "give",
        usage: "/give <catalog-id> | list",
        description: "Spawn a curated pickup item with physics enabled.",
        help: &[
            "/give list prints reviewed stable IDs and names, sorted by ID. Scroll for the complete list.",
            "Physics is always enabled and immediately movable. Fixed map decoration does not change asset eligibility.",
            "At most 16 session props (/give and /become place combined). A clear flat floor ahead is required.",
        ],
    },
    CommandSpec {
        kind: CommandKind::Respawn,
        name: "respawn",
        usage: "/respawn",
        description: "Return safely to the validated map spawn.",
        help: &[
            "Safely drops a held item, clears transient movement and preserves world entities. Refuses unusable spawn or blocked release.",
        ],
    },
    CommandSpec {
        kind: CommandKind::Position,
        name: "pos",
        usage: "/pos",
        description: "Print eye/feet position and facing for bug reports.",
        help: &[
            "World coordinates are metres, +Y up. Yaw/pitch are degrees: yaw 0 faces -Z, 90 faces +X, positive pitch looks up.",
        ],
    },
    CommandSpec {
        kind: CommandKind::Remove,
        name: "remove",
        usage: "/remove [all]",
        description: "Remove the held item, or matching physical instances.",
        help: &[
            "Requires a held physics item. /remove removes it; /remove all removes exact matching catalog types in this map, including authored movable props.",
            "Fixed/static decorations are preserved. Removal is session-only; reload restores authored content.",
        ],
    },
    CommandSpec {
        kind: CommandKind::Physics,
        name: "physics",
        usage: "/physics pause|resume|step",
        description: "Control fixed-tick world physics.",
        help: &[
            "Pause stops rigid-body simulation; player and held control remain unscaled. Resume respects /time scale.",
            "Step immediately advances exactly one 1/120-second world tick, clears catch-up remainder, then stays paused, including at scale 0.",
        ],
    },
    CommandSpec {
        kind: CommandKind::Time,
        name: "time",
        usage: "/time scale <0..4>",
        description: "Scale world time; player and held control stay normal.",
        help: &[
            "1 is normal, 0 stops world time. Only finite values from 0 through 4 are accepted.",
            "Doors, routes, timers, world animations/weather and physics use world time. Player movement, look, held control and representation animation use unscaled time.",
            "Physics pause takes precedence for bodies; resume at scale 0 keeps them frozen. Dropped/thrown items join world time at their valid release pose.",
        ],
    },
    CommandSpec {
        kind: CommandKind::Trigger,
        name: "trigger",
        usage: "/trigger <id> | show [on|off]",
        description: "Activate an authored trigger or show nearby IDs.",
        help: &[
            "IDs must name actionable trigger volumes; ordinary item/entity IDs are not triggers. Existing bindings, cooldowns and one-shot rules apply.",
            "Show toggles a bounded, occlusion-checked nearby view-cone overlay. /trigger show off hides it; flashlight power is irrelevant.",
        ],
    },
    CommandSpec {
        kind: CommandKind::Become,
        name: "become",
        usage: "/become <model> | place | phys on|off | reset",
        description: "Select a model and enter third person.",
        help: &[
            "/become list shows curated pickup models and supported characters; party_cup and spooner_man are aliases. Player collision/control retains the normal body.",
            "Place duplicates only the selected visual transform at your feet/yaw. Placements are session-local and share the 16-prop limit.",
            "Phys defaults off. /become phys on makes future placements physical and frozen until interaction/contact or activation. Move clear before enabling them.",
            "Reset restores first person. Selection/reset safely drops held items; a blocked release refuses the action.",
        ],
    },
];

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Help(Option<CommandKind>),
    Clear,
    GiveList,
    BecomeList,
    Action(GameplayAction),
}

fn spec_for(name: &str) -> Option<&'static CommandSpec> {
    REGISTRY
        .iter()
        .find(|spec| spec.name.eq_ignore_ascii_case(name.trim_start_matches('/')))
}

pub(crate) fn is_valid_command_id(id: &str) -> bool {
    id.len() <= 128 && crate::engine::assets::is_valid_asset_id(id)
}

fn on_off(value: &str) -> Option<bool> {
    match value {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// Parse one bounded command, never a script, path or shell expression.
/// # Errors
/// Reports unknown commands, malformed arguments and unsafe numerical values.
pub fn parse(line: &str) -> Result<Command, String> {
    if line.len() > 256 || !line.is_ascii() {
        return Err("Commands must contain at most 256 ASCII bytes.".into());
    }
    let words: Vec<_> = line.split_whitespace().collect();
    let name = words
        .first()
        .and_then(|word| word.strip_prefix('/'))
        .ok_or_else(|| "Commands start with /. Try /help.".to_string())?;
    let spec = spec_for(name).ok_or_else(|| format!("Unknown command /{name}. Try /help."))?;
    let args = words.get(1..).unwrap_or_default();
    if args == ["help"] {
        return Ok(Command::Help(Some(spec.kind)));
    }
    let action = match (spec.kind, args) {
        (CommandKind::Help, []) => return Ok(Command::Help(None)),
        (CommandKind::Help, [requested_name]) => {
            return spec_for(requested_name)
                .map(|requested| Command::Help(Some(requested.kind)))
                .ok_or_else(|| format!("Unknown command `{requested_name}`. Try /help."));
        }
        (CommandKind::Clear, []) => return Ok(Command::Clear),
        (CommandKind::Give, ["list"]) => return Ok(Command::GiveList),
        (CommandKind::Become, ["list"]) => return Ok(Command::BecomeList),
        (CommandKind::Noclip, []) => Some(GameplayAction::Noclip(None)),
        (CommandKind::Noclip, [value]) => {
            on_off(value).map(|enabled| GameplayAction::Noclip(Some(enabled)))
        }
        (CommandKind::Give, [id]) if is_valid_command_id(id) => {
            Some(GameplayAction::Give((*id).to_string()))
        }
        (CommandKind::Respawn, []) => Some(GameplayAction::Respawn),
        (CommandKind::Position, []) => Some(GameplayAction::Position),
        (CommandKind::Remove, []) => Some(GameplayAction::Remove(RemovalScope::Held)),
        (CommandKind::Remove, ["all"]) => {
            Some(GameplayAction::Remove(RemovalScope::MatchingPhysics))
        }
        (CommandKind::Physics, ["pause"]) => Some(GameplayAction::Physics(PhysicsAction::Pause)),
        (CommandKind::Physics, ["resume"]) => Some(GameplayAction::Physics(PhysicsAction::Resume)),
        (CommandKind::Physics, ["step"]) => Some(GameplayAction::Physics(PhysicsAction::Step)),
        (CommandKind::Time, ["scale", value]) => {
            let scale = value
                .parse::<f32>()
                .ok()
                .filter(|scale| scale.is_finite() && (0.0..=4.0).contains(scale))
                .ok_or_else(|| {
                    "Time scale must be a finite number from 0 through 4 (1 = normal).".to_string()
                })?;
            Some(GameplayAction::TimeScale(scale))
        }
        (CommandKind::Trigger, ["show"]) => Some(GameplayAction::TriggerLabels(None)),
        (CommandKind::Trigger, ["show", value]) => {
            on_off(value).map(|enabled| GameplayAction::TriggerLabels(Some(enabled)))
        }
        (CommandKind::Trigger, [id]) if is_valid_command_id(id) => {
            Some(GameplayAction::Trigger((*id).to_string()))
        }
        (CommandKind::Become, ["place"]) => {
            Some(GameplayAction::Become(RepresentationAction::Place))
        }
        (CommandKind::Become, ["reset"]) => {
            Some(GameplayAction::Become(RepresentationAction::Reset))
        }
        (CommandKind::Become, ["phys", value]) => on_off(value)
            .map(|enabled| GameplayAction::Become(RepresentationAction::PlacementPhysics(enabled))),
        (CommandKind::Become, [id]) if is_valid_command_id(id) => Some(GameplayAction::Become(
            RepresentationAction::Select((*id).to_string()),
        )),
        (
            CommandKind::Help
            | CommandKind::Clear
            | CommandKind::Noclip
            | CommandKind::Give
            | CommandKind::Respawn
            | CommandKind::Position
            | CommandKind::Remove
            | CommandKind::Physics
            | CommandKind::Time
            | CommandKind::Trigger
            | CommandKind::Become,
            _,
        ) => None,
    };
    action.map(Command::Action).ok_or_else(|| {
        format!(
            "Usage: {}. /{} help explains options.",
            spec.usage, spec.name
        )
    })
}
