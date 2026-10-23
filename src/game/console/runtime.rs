//! Console presentation over the shared typed gameplay runtime.
use super::{
    Console,
    command::{Command, REGISTRY},
};
pub use crate::game::gameplay_runtime::{GameplayContext, Runtime};
use crate::game::pickup_catalog;

impl Runtime {
    pub fn execute(
        &mut self,
        command: Result<Command, String>,
        console: &mut Console,
        context: &mut GameplayContext<'_>,
    ) {
        if !console.is_open() {
            return;
        }
        let result = match command {
            Err(error) => Err(error),
            Ok(Command::Help(request)) => {
                for spec in REGISTRY
                    .iter()
                    .filter(|spec| request.is_none_or(|kind| kind == spec.kind))
                {
                    console.write(format!("{} - {}", spec.usage, spec.description));
                    if request.is_some() {
                        for line in spec.help {
                            console.write(line);
                        }
                    }
                }
                if request.is_none() {
                    console.write("/<command> help explains options. Up/Down history; PageUp/PageDown or wheel scrolls.");
                }
                Ok(())
            }
            Ok(Command::Crosshair(enabled)) => {
                self.crosshair_override = Some(enabled);
                console.write(format!(
                    "Crosshair {} (until next map load).",
                    if enabled { "ON" } else { "OFF" }
                ));
                Ok(())
            }
            Ok(Command::Clear) => {
                console.clear();
                Ok(())
            }
            Ok(Command::GiveList) => {
                console.write(format!(
                    "{} curated pickup items (physics enabled). Scroll for all IDs:",
                    pickup_catalog::ITEMS.len()
                ));
                for item in pickup_catalog::ITEMS {
                    console.write(format!("{} - {}", item.id, item.name));
                }
                Ok(())
            }
            Ok(Command::BecomeList) => {
                console
                    .write("Eligible representations: all /give list IDs, plus these characters:");
                for id in crate::game::representation::CHARACTER_MODELS
                    .iter()
                    .chain(crate::game::representation::PROP_FORM_MODELS)
                {
                    console.write(id);
                }
                console.write(
                    "Aliases: party_cup = arcade:party_cup_confetti; spooner_man = spooner-man.",
                );
                Ok(())
            }
            Ok(Command::Action(action)) => self
                .apply_action(&action, context)
                .map(|message| console.write(message)),
        };
        if let Err(error) = result {
            console.write(format!("Error: {error}"));
        }
    }
}
