//! Authored-source evidence emitted only for explicit native diagnostic events.

use serde_json::{Map, Value, json};

impl super::FrameLoop<'_> {
    pub(super) fn append_authoring_trace(&self, fields: &mut Map<String, Value>) {
        drop(fields.insert(
            "trigger_query".into(),
            json!(self.game.entities().trigger_query_stats()),
        ));
        let Some(loaded) = &self.current_level else {
            return;
        };
        let surfaces = crate::engine::level::LevelSurfaces::new(&loaded.level);
        let placements: Vec<_> = loaded.level.props.iter().zip(loaded.level.prop_instance_ids()).map(|(prop,id)| {
            let base_y = surfaces.floor_y_at(prop.x,prop.z).unwrap_or(0.0);
            json!({"id":id,"model":prop.model,"position":[prop.x,base_y+prop.y,prop.z],"rotation":[prop.rotation_degrees,prop.pitch_degrees,prop.roll_degrees],"orientation":prop.orientation().to_array(),"physics":prop.physics})
        }).collect();
        drop(fields.insert("authored_placements".into(), json!(placements)));
        drop(fields.insert(
            "authored_environment".into(),
            json!({
                "fog_regions":loaded.level.fog_regions,"ground_fog":loaded.level.ground_fog,
                "sound_emitters":loaded.level.sound_emitters,
            }),
        ));
    }
}
