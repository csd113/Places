//! Typed runtime dispatch. Catalog physics profiles opt small rigid props into
//! gameplay; other supported models remain explicitly labelled visual previews.
use super::{
    Console,
    command::{Command, REGISTRY},
};
use crate::engine::{
    physics::{ItemWorld, PlayerContext},
    render::Renderer,
};
use crate::game::Game;
use glam::Vec3;

const MAX_GIVEN_PROPS: usize = 16;
struct GivenProp {
    key: u64,
    model: String,
    min: Vec3,
    max: Vec3,
    physical: bool,
}

#[derive(Default)]
pub struct Runtime {
    given: Vec<GivenProp>,
}

impl Runtime {
    pub fn execute(
        &mut self,
        command: Result<Command, String>,
        console: &mut Console,
        game: &mut Game,
        renderer: &mut Renderer,
        physics: &mut ItemWorld,
    ) {
        dispatch(
            command,
            console,
            &mut LiveTarget {
                runtime: self,
                game,
                renderer,
                physics,
            },
        );
    }

    fn give(
        &mut self,
        id: &str,
        game: &Game,
        renderer: &mut Renderer,
        physics: &mut ItemWorld,
    ) -> Result<String, String> {
        if self.given.len() >= MAX_GIVEN_PROPS {
            return Err(
                "Given prop limit reached (16). Disable console or reload the map to remove them."
                    .to_string(),
            );
        }
        let (low, high) = renderer.rigid_runtime_model_bounds(id)?;
        let min = Vec3::from_array(low);
        let max = Vec3::from_array(high);
        let extent = Vec3::new(max.x - min.x, max.y - min.y, max.z - min.z);
        if extent.max_element() > 1.5 {
            return Err("Given props must be no larger than 1.5 m per axis.".to_string());
        }
        let floor_position = game.developer_prop_position(min, max).ok_or_else(|| "No clear flat floor ahead. Walk to open ground; /give is unavailable during noclip.".to_string())?;
        let profile = renderer.rigid_runtime_model_physics(id);
        // A centimetre of placement clearance keeps exact floor contact out
        // of the overlap query; gravity closes this gap on the first steps.
        let position = Vec3::new(
            floor_position.x,
            floor_position.y + if profile.is_some() { 0.01 } else { 0.0 },
            floor_position.z,
        );
        let world_min = Vec3::new(min.x + position.x, min.y + position.y, min.z + position.z);
        let world_max = Vec3::new(max.x + position.x, max.y + position.y, max.z + position.z);
        if self.given.iter().any(|preview| {
            !preview.physical
                && world_min.cmplt(preview.max).all()
                && world_max.cmpgt(preview.min).all()
        }) {
            return Err(
                "A preview prop already occupies that spot. Move before giving another."
                    .to_string(),
            );
        }
        // The high key range is reserved for developer previews. Normal world
        // keys start at one and are bounded by the authored entity lifecycle.
        let key = u64::MAX.saturating_sub(u64::try_from(self.given.len()).unwrap_or(u64::MAX));
        if let Some(def) = profile.as_ref() {
            physics.sync_context(
                PlayerContext {
                    eye: game.player_position,
                    feet: Vec3::new(
                        game.player_position.x,
                        game.feet_y(),
                        game.player_position.z,
                    ),
                    height: game.body_height(),
                    view: game.view_direction(),
                    yaw: game.player_yaw,
                    active: !game.noclip_enabled(),
                },
                game.door_colliders(),
            );
            physics.spawn_if_clear(key, def, (min, max), position, 0.0, 1.0)?;
        }
        if !renderer.spawn_runtime_model(key, id, position.to_array(), 0.0, 1.0) {
            let _removed = physics.remove(key);
            return Err(
                "Renderer refused the given prop (model/resource budget). No item was given."
                    .to_string(),
            );
        }
        self.given.push(GivenProp {
            key,
            model: id.to_string(),
            min: world_min,
            max: world_max,
            physical: profile.is_some(),
        });
        Ok(if profile.is_some() {
            if profile.is_some_and(|def| def.pickup) {
                format!("Placed `{id}` physical item.")
            } else {
                format!("Placed `{id}` physical prop without pickup.")
            }
        } else {
            format!(
                "Placed `{id}` visual preview. This catalog type has no physics profile or pickup."
            )
        })
    }

    pub fn clear_given_props(&mut self, renderer: &mut Renderer, physics: &mut ItemWorld) {
        for preview in self.given.drain(..) {
            let _physical_removed = physics.remove(preview.key);
            let _visual_removed = renderer.despawn_runtime_model(preview.key);
        }
    }

    #[must_use]
    pub fn given_key(&self, id: &str) -> Option<u64> {
        self.given
            .iter()
            .rev()
            .find_map(|prop| (prop.model == id).then_some(prop.key))
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
            console.write("Give: exact rigid catalog IDs <=1.5 m; max 16. Catalog physics profiles support pickup/drop; other types are visual previews. Disable console or reload to remove given props.");
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
    physics: &'a mut ItemWorld,
}
impl CommandTarget for LiveTarget<'_> {
    fn set_noclip(&mut self, request: Option<bool>) -> Result<String, String> {
        let enabled = request.unwrap_or_else(|| !self.game.noclip_enabled());
        if enabled && self.physics.held().is_some() {
            return Err("Drop the held item before enabling noclip.".to_string());
        }
        self.game.set_noclip(enabled).map(str::to_string)
    }
    fn give(&mut self, id: &str) -> Result<String, String> {
        self.runtime
            .give(id, self.game, self.renderer, self.physics)
    }
}
