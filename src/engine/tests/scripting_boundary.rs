//! Tiny CPU fixtures protect native query/move work limits and real label
//! capabilities. No assets, wall-time assertions or map bake are used.

use super::{MAX_NATIVE_CALLS, Module, ScriptSource, api};
use crate::engine::entities::EntityWorld;
use crate::engine::level::{ActionDef, LevelDef};
use crate::engine::project::ProjectPath;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::sync::Mutex;

#[test]
fn read_only_host_queries_have_a_hard_count_budget() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"host_budget","name":"Host budget","spawn":{"x":0,"z":0}}"#,
    )
    .map_err(|problem| problem.to_string())?;
    let world = EntityWorld::from_level(&level);
    let module = Module::load(&ScriptSource {
        path: ProjectPath::new("scripts/probe.luau".into())?,
        source: "return {probe=function(api) for n=1,257 do api.exists('missing') end end}".into(),
    })
    .map_err(|problem| problem.to_string())?;
    let queue = Mutex::new(Vec::new());
    let notices = Mutex::new(Vec::new());
    let points = BTreeSet::new();
    let groups = BTreeSet::new();
    let calls = Cell::new(0usize);
    // Isolate the hard count boundary from cooperative wall deadlines: an
    // overloaded CI worker must still exercise the native guard itself.
    let result = module.lua.scope(|scope| {
        let host = api::host(
            &module.lua,
            scope,
            &world,
            &queue,
            &notices,
            &points,
            &groups,
            &calls,
            32,
        )?;
        let navigate: mlua::Function = host.raw_get("navigate")?;
        let coercion = navigate
            .call::<()>(("missing", "1", 0.0_f32, 0.0_f32))
            .err()
            .ok_or_else(|| super::error("numeric-string coordinate was admitted"))?;
        assert!(
            coercion
                .to_string()
                .contains("Coordinate requires a number"),
            "numeric strings are rejected before native entity work"
        );
        assert_eq!(
            calls.get(),
            0,
            "invalid argument never enters the host callback"
        );
        let probe: mlua::Function = module.exports.raw_get("probe")?;
        probe.call::<()>(host)
    });
    let problem = result.err().ok_or("native query burst was not refused")?;
    assert!(
        problem
            .to_string()
            .contains("Host native-call budget exhausted"),
        "query-only work reaches the native budget guard"
    );
    assert_eq!(calls.get(), MAX_NATIVE_CALLS, "only admitted calls run");
    assert!(
        queue
            .lock()
            .map_err(|lock_error| lock_error.to_string())?
            .is_empty(),
        "queries do not create world actions"
    );
    Ok(())
}

// A three-value table protects a native CPU escape that allocates no large
// heap: sparse negative destination ranges formerly ran outside VM interrupts.
// Refusal is immediate; the valid cases retain ordinary overlap/target behavior.
#[test]
fn sparse_table_moves_are_bounded_before_native_iteration() -> Result<(), String> {
    for (body, expected) in [
        ("table.move({},1,10000000,-10000000)", "range exceeds 8192"),
        ("table.move({},'1',2,1)", "integral 32-bit numbers"),
    ] {
        let problem = Module::load(&ScriptSource {
            path: ProjectPath::new("scripts/move.luau".into())?,
            source: format!("{body}; return {{}}"),
        })
        .err()
        .ok_or("unbounded/coerced native move was admitted")?;
        assert!(
            problem.to_string().contains(expected),
            "the explicit native-work guard must refuse: {problem}"
        );
    }
    let _valid = Module::load(&ScriptSource {
        path: ProjectPath::new("scripts/move.luau".into())?,
        source: "local a={10,20,30}; assert(table.move(a,1,2,2)==a and a[2]==10 and a[3]==20); local b={}; assert(table.move(a,1,3,-2,b)==b and b[-2]==10 and b[0]==20); assert(table.move({},2,1,0,b)==b); return {}".into(),
    })
    .map_err(|problem| problem.to_string())?;
    Ok(())
}

#[test]
fn label_actions_require_the_runtime_capability_but_allow_disabled_controls() -> Result<(), String>
{
    let mut level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"label_boundary","name":"Label boundary","spawn":{"x":3,"z":3},"rooms":[{"width":4,"depth":4}],
        "props":[{"id":"plate","model":"core:plant"}],
        "volumes":[{"id":"zone","x":0,"z":0,"width":1,"depth":1,"bindings":[{"on":"enter_volume","actions":[{"action":"toggle_label","target":"plate"}]}]}]}"#,
    )
    .map_err(|problem| problem.to_string())?;
    let problem = crate::engine::loader::validate_level(&level)
        .err()
        .ok_or("label action without runtime capability was admitted")?;
    assert!(
        problem.contains("interactable") && problem.contains("label capability"),
        "admission names the missing runtime capability: {problem}"
    );
    level
        .props
        .first_mut()
        .ok_or("missing plate")?
        .components
        .push(
            serde_json::from_str(
                r#"{"component":"interactable","enabled":false,"label":"Closed"}"#,
            )
            .map_err(|decode_error| decode_error.to_string())?,
        );
    crate::engine::loader::validate_level(&level)?;
    let mut world = EntityWorld::from_level(&level);
    let report = world.dispatch_on(
        "zone",
        &[ActionDef::ToggleLabel {
            target: Some("plate".into()),
        }],
    );
    assert_eq!(
        report.labels_shown, 1,
        "disabled plate still supports completion feedback"
    );
    assert!(
        world
            .handle_of("plate")
            .and_then(|handle| world.components().interactables.get(handle))
            .is_some_and(|component| component.label_visible && !component.enabled),
        "label visibility does not make the plate aimable"
    );
    Ok(())
}
