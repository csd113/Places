//! Private quick-test preparation. This path writes no package, saved source,
//! profile or lighting cache; final builds keep the compiler admission policy.
use crate::engine::{
    assets::AssetCatalog, level::LevelDef, loader::LoadedLevel, props::PropAssets,
};
use crate::game::CollisionWorld;

pub(super) struct PreparedTest {
    pub level: LevelDef,
    pub world: CollisionWorld,
    pub physics: super::test_session::PreparedPhysics,
    pub warning: String,
}

/// Validate the same authored schema, built-in references and bounded modules
/// used for final source admission, without asking for final lighting output.
pub(super) fn validate(level: &LevelDef, catalog: &AssetCatalog) -> Result<(), String> {
    crate::engine::resources::checkpoint()?;
    crate::engine::loader::validate_level(level)?;
    crate::engine::project::validate_level_assets(level, catalog)?;
    crate::engine::scripting::check_level(level)?;
    crate::engine::resources::checkpoint()
}

impl PreparedTest {
    pub fn prepare(loaded: &LoadedLevel, assets: &mut PropAssets) -> Result<Self, String> {
        crate::engine::progress::stage("Preparing gameplay navigation / no lighting bake");
        crate::engine::resources::checkpoint()?;
        if let Some(issue) = crate::engine::structure::clearances(&loaded.level).first() {
            return Err(format!(
                "Structural clearance: room {} ceiling and room {} floor leave {:.3} m; need {:.2} m. Review Safety before testing.",
                issue.lower,
                issue.upper,
                issue.gap,
                crate::engine::level::MIN_STRUCTURE_M,
            ));
        }
        let mut warnings = Vec::new();
        let (bytes, _report) =
            crate::map_compiler::bake_navigation(&loaded.level, 1, &mut warnings)?;
        crate::engine::resources::checkpoint()?;
        let grid = crate::engine::package::navigation::read_navigation(&bytes)?;
        let navigation = crate::engine::nav::NavMesh::from_record(grid)?;
        let world = CollisionWorld::from_level_with_navigation(&loaded.level, navigation)
            .with_ground_materials(&loaded.level, &loaded.materials);
        crate::engine::progress::stage("Preparing gameplay physics / no lighting bake");
        let physics = super::test_session::PreparedPhysics::prepare(loaded, &world, assets)?;
        crate::engine::resources::checkpoint()?;
        let mut warning = if loaded.level.requires_full_lighting {
            "Full-required level / quick lighting is approximate. Final build checks shadows and circuits."
        } else {
            "Approximate lighting / Final build checks shadows, indirect light, reflections and circuits."
        }.to_string();
        for message in warnings.iter().take(4) {
            warning.push(' ');
            warning.push_str(message);
        }
        crate::engine::progress::stage("Gameplay ready / uploading scene");
        Ok(Self {
            level: loaded.level.clone(),
            world,
            physics,
            warning,
        })
    }
}
