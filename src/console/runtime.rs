//! Typed runtime dispatch and a deliberately limited give integration seam.
//! Preview props have no pickup, inventory, collision or physical simulation.
use super::{
    Console,
    command::{Command, REGISTRY},
};
use crate::{game::Game, render::Renderer};
use glam::Vec3;

const MAX_PREVIEWS: usize = 16;
struct Preview {
    key: u64,
    min: Vec3,
    max: Vec3,
}

#[derive(Default)]
pub struct Runtime {
    previews: Vec<Preview>,
}

impl Runtime {
    pub fn execute(
        &mut self,
        command: Result<Command, String>,
        console: &mut Console,
        game: &mut Game,
        renderer: &mut Renderer,
    ) {
        dispatch(
            command,
            console,
            &mut LiveTarget {
                runtime: self,
                game,
                renderer,
            },
        );
    }

    fn give(&mut self, id: &str, game: &Game, renderer: &mut Renderer) -> Result<String, String> {
        if self.previews.len() >= MAX_PREVIEWS {
            return Err(
                "Preview limit reached (16). Disable console or reload the map to remove them."
                    .to_string(),
            );
        }
        let (low, high) = renderer.rigid_runtime_model_bounds(id)?;
        let min = Vec3::from_array(low);
        let max = Vec3::from_array(high);
        let position = game.developer_prop_position(min, max).ok_or_else(|| "No clear flat floor ahead. Walk to open ground; /give is unavailable during noclip.".to_string())?;
        let world_min = Vec3::new(min.x + position.x, min.y + position.y, min.z + position.z);
        let world_max = Vec3::new(max.x + position.x, max.y + position.y, max.z + position.z);
        if self
            .previews
            .iter()
            .any(|preview| world_min.cmplt(preview.max).all() && world_max.cmpgt(preview.min).all())
        {
            return Err(
                "A preview prop already occupies that spot. Move before giving another."
                    .to_string(),
            );
        }
        // The high key range is reserved for developer previews. Normal world
        // keys start at one and are bounded by the authored entity lifecycle.
        let key = u64::MAX.saturating_sub(u64::try_from(self.previews.len()).unwrap_or(u64::MAX));
        if !renderer.spawn_runtime_model(key, id, position.to_array(), 0.0, 1.0) {
            return Err(
                "Renderer refused the preview (model/resource budget). No item was given."
                    .to_string(),
            );
        }
        self.previews.push(Preview {
            key,
            min: world_min,
            max: world_max,
        });
        Ok(format!(
            "Placed `{id}` visual prop. No inventory, pickup, collision or physics."
        ))
    }

    pub fn clear_previews(&mut self, renderer: &mut Renderer) {
        for preview in self.previews.drain(..) {
            let _removed = renderer.despawn_runtime_model(preview.key);
        }
    }
}

/// Focused integration seam for future gameplay commands. Parsing and UI never
/// acquire renderer, inventory or physics ownership.
pub trait CommandTarget {
    /// # Errors
    /// Refuses a movement mode change without a safe return pose.
    fn set_noclip(&mut self, request: Option<bool>) -> Result<String, String>;
    /// # Errors
    /// Refuses unknown, unsupported or unplaceable items without claiming success.
    fn give(&mut self, id: &str) -> Result<String, String>;
}

pub fn dispatch(
    command: Result<Command, String>,
    console: &mut Console,
    target: &mut impl CommandTarget,
) {
    // UI ownership is the command authorization gate, also used by native
    // acceptance actions. Closed/disabled console cannot dispatch commands.
    if !console.is_open() {
        return;
    }
    let result = match command {
        Err(error) => Err(error),
        Ok(Command::Help) => {
            for spec in &REGISTRY {
                console.write(format!("{} - {}", spec.usage, spec.description));
            }
            console.write("Noclip: movement/look keys; hold Jump up / Crouch down.");
            console.write("Give: exact rigid catalog IDs <=1.5 m; max 16 previews. Disable console or reload map to remove previews.");
            Ok(())
        }
        Ok(Command::Clear) => {
            console.clear();
            Ok(())
        }
        Ok(Command::Noclip(request)) => target
            .set_noclip(request)
            .map(|message| console.write(message)),
        Ok(Command::Give(id)) => target.give(&id).map(|message| console.write(message)),
    };
    if let Err(error) = result {
        console.write(format!("Error: {error}"));
    }
}

struct LiveTarget<'a> {
    runtime: &'a mut Runtime,
    game: &'a mut Game,
    renderer: &'a mut Renderer,
}
impl CommandTarget for LiveTarget<'_> {
    fn set_noclip(&mut self, request: Option<bool>) -> Result<String, String> {
        self.game
            .set_noclip(request.unwrap_or_else(|| !self.game.noclip_enabled()))
            .map(str::to_string)
    }
    fn give(&mut self, id: &str) -> Result<String, String> {
        self.runtime.give(id, self.game, self.renderer)
    }
}
