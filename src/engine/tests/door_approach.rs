//! Small baked portal cases exercise active-leg ownership and physical
//! steering with a stalled one-way leaf. Neither creates a map package.

use super::*;
use crate::engine::ai::{AiDef, AiWorld, local_move_step};
use crate::engine::collision::WallAabb;
use crate::engine::nav::NavAgentProfile;
use crate::engine::tests::navigation_ai::door_retreat_fixture_with_detour;

fn actor_at(world: &mut AiWorld, position: Vec3) -> Result<&mut AiAgent, String> {
    world.register(
        EntityHandle::from_parts(0, 0),
        "actor",
        AiDef::default(),
        NavAgentProfile::reference(),
        position,
        0.0,
        false,
    );
    world
        .agents
        .first_mut()
        .ok_or_else(|| "missing actor".into())
}

fn waiting_plan(
    agent: &mut AiAgent,
    ctx: &AiTickContext<'_>,
    nav: &NavMesh,
    goal: Vec3,
) -> Result<(usize, Path), String> {
    let door = ctx.doors.get(0).ok_or("missing swing door")?;
    let overlay = OpeningDoor {
        ctx,
        door,
        usage: PortalUse::Waiting,
    };
    let query = PathQuery {
        class: 0,
        start: agent.position,
        goal,
        can_open_doors: true,
        max_expansions: 2048,
        doors: &overlay,
    };
    let mut path = Path::new();
    nav.path_into_bounded(&query, &mut agent.scratch, &mut path, None)
        .map_err(|error| format!("{error:?}"))?;
    let index = path
        .waypoints
        .iter()
        .position(|&point| current_door(agent, ctx, &path, point) == Some(0))
        .ok_or("waiting plan must use the swing door")?;
    Ok((index, path))
}

#[test]
fn waiting_replan_keeps_only_the_equivalent_door_leg() -> Result<(), String> {
    let mut fixture = door_retreat_fixture_with_detour(true)?;
    let from = Vec3::new(2.7, 0.0, 1.1);
    let goal = Vec3::new(2.7, 0.0, 2.9);
    let new_goal = Vec3::new(2.7, 0.0, 3.55);
    assert!(fixture.doors.request_open_from("swing", Some(from)));
    let _steps = fixture.doors.advance(0.05, |_, _| true);
    assert!(fixture.doors.door_obstructed("swing"));

    fixture
        .obstacles
        .push(WallAabb::with_y(0.3, 0.0, 1.9, 1.2, 2.1, 0.3));
    let mut world = AiWorld::new();
    let agent = actor_at(&mut world, from)?;
    let ctx = fixture.context(0.05, &[]);
    let (index, path) = waiting_plan(agent, &ctx, &fixture.nav, goal)?;
    let waypoint = *path
        .waypoints
        .get(index)
        .ok_or("missing original waypoint")?;
    agent.path = path;
    agent.current_waypoint = index;
    agent.clock = 1.0;
    agent.opening_door = Some(Approach {
        door: 0,
        waypoint,
        stand_off: None,
        stalled_since: Some(0.25),
    });
    let query = PathQuery {
        class: 0,
        start: from,
        goal,
        can_open_doors: true,
        max_expansions: 2048,
        doors: &ctx,
    };
    let mut out = AiOutcome::default();
    let mut trial = Path::new();
    let start = plan(
        agent,
        &fixture.nav,
        &query,
        &mut trial,
        None,
        &ctx,
        &mut out,
    )
    .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        start.and_then(|slot| trial.waypoints.get(slot)).copied(),
        Some(waypoint)
    );
    assert_eq!(
        agent
            .opening_door
            .as_ref()
            .and_then(|approach| approach.stalled_since),
        Some(0.25)
    );
    assert_eq!(
        out.path_queries, 0,
        "an equivalent leg needs no speculative fallback"
    );

    let (changed_index, changed_path) = waiting_plan(agent, &ctx, &fixture.nav, new_goal)?;
    let changed_waypoint = *changed_path
        .waypoints
        .get(changed_index)
        .ok_or("missing replacement waypoint")?;
    assert!(
        !changed_waypoint.abs_diff_eq(waypoint, movement::ARRIVE_RADIUS_M),
        "the same door must have a genuinely different crossing waypoint"
    );
    let changed_query = PathQuery {
        goal: new_goal,
        ..query
    };
    let replacement = plan(
        agent,
        &fixture.nav,
        &changed_query,
        &mut trial,
        None,
        &ctx,
        &mut out,
    );
    assert!(
        matches!(replacement, Ok(None) | Err(PathResult::Unreachable))
            && agent.opening_door.is_none(),
        "a changed leg must use live-door planning, even when no detour is available: {replacement:?}"
    );
    assert!(
        trial.door_requests.is_empty(),
        "the fallback must respect the blocked swing"
    );
    assert_eq!(
        out.path_queries, 1,
        "the fallback spends a separate bounded query"
    );

    Ok(())
}

#[test]
fn changed_waypoint_through_the_same_door_starts_a_new_timeout() -> Result<(), String> {
    let mut fixture = door_retreat_fixture_with_detour(false)?;
    let from = Vec3::new(2.7, 0.0, 1.1);
    let waypoint = Vec3::new(2.7, 0.0, 2.9);
    let changed_waypoint = Vec3::new(2.7, 0.0, 3.55);
    assert!(fixture.doors.request_open_from("swing", Some(from)));
    let _steps = fixture.doors.advance(0.05, |_, _| true);
    assert!(fixture.doors.door_obstructed("swing"));
    // Real solid sides keep the replacement approach waiting.
    fixture.obstacles.extend([
        WallAabb::with_y(1.90, 0.0, 0.0, 0.10, 2.1, 1.00),
        WallAabb::with_y(3.35, 0.0, 0.0, 0.10, 2.1, 1.00),
        WallAabb::with_y(2.00, 0.0, 0.40, 1.35, 2.1, 0.10),
    ]);
    let mut world = AiWorld::new();
    let agent = actor_at(&mut world, from)?;
    agent.path.waypoints.push(changed_waypoint);
    agent.path.door_requests.push("swing".into());
    agent.clock = 1.0;
    agent.opening_door = Some(Approach {
        door: 0,
        waypoint,
        stand_off: None,
        stalled_since: Some(0.25),
    });
    let leaves = fixture.doors.colliders();
    let mut blocked_ctx = fixture.context(0.05, &[]);
    blocked_ctx.leaves = &leaves;
    assert!(advance(
        agent,
        &blocked_ctx,
        &HashMap::new(),
        &fixture.nav,
        changed_waypoint,
        1.5,
        &mut AiOutcome::default()
    ));
    let approach = agent
        .opening_door
        .as_ref()
        .ok_or("old timer discarded the new leg")?;
    assert!(approach.matches_leg(0, changed_waypoint));
    assert!(
        agent.stuck_seconds < 0.10,
        "the old timer must not be inherited"
    );
    Ok(())
}

#[test]
fn portal_exit_rejects_a_steer_across_the_closed_throat() -> Result<(), String> {
    let mut fixture = door_retreat_fixture_with_detour(false)?;
    let from = Vec3::new(2.95, 0.0, 1.55);
    let away = Vec3::new(3.9, 0.0, 1.55);
    let angled = Vec3::new(2.95, 0.0, 2.5);
    assert!(fixture.doors.request_open_from("swing", Some(from)));
    let _clear_steps = fixture.doors.advance(0.65, |_, _| false);
    let _blocked_steps = fixture.doors.advance(0.05, |_, _| true);
    let door = fixture.doors.get(0).ok_or("missing swing door")?;
    assert_eq!(door.phase(), DoorPhase::Opening);
    assert!(door.obstructed, "the live leaf must be stalled");
    let mut world = AiWorld::new();
    let agent = actor_at(&mut world, from)?;
    let leaf = door.collider();
    assert!(
        !crosses_leaf(agent, angled, &leaf),
        "the stalled leaf itself must clear the angled segment"
    );
    let leaves = fixture.doors.colliders();
    let mut ctx = fixture.context(0.05, &[]);
    ctx.leaves = &leaves;
    let exit =
        leaving_portal(agent, &ctx, &fixture.nav, away).ok_or("clear straight leg may leave")?;
    assert!(exit.allows_exit_segment(agent, away));
    assert!(
        !exit.allows_exit_segment(agent, angled),
        "an angled exit probe may not cross the closed throat"
    );
    assert!(
        fixture.nav.segment_clear(0, from, angled, &exit, false),
        "the portal relaxation alone would admit the angled probe"
    );
    let clear_motion = local_move_step(agent, &ctx, &HashMap::new(), &fixture.nav, 0, away, 1.5);
    assert!(
        matches!(clear_motion, MoveStep::Moved { position, yaw_degrees: _, speed_mps: _ } if position.x > from.x)
    );

    fixture
        .obstacles
        .push(WallAabb::with_y(3.42, 0.0, 0.0, 0.12, 2.1, 2.05));
    let mut obstructed = fixture.context(0.10, &[]);
    obstructed.leaves = &leaves;
    let obstructed_exit = leaving_portal(agent, &obstructed, &fixture.nav, away)
        .ok_or("straight waypoint still avoids throat")?;
    assert!(
        !fixture
            .nav
            .segment_clear(0, from, away, &obstructed_exit, false),
        "a genuine solid must still block a portal exit"
    );
    let physical_only = step(agent, &obstructed, angled, 8.0, obstructed.delta);
    assert!(
        matches!(physical_only, MoveStep::Moved { position, yaw_degrees: _, speed_mps: _ }
            if crosses_leaf(agent, position, &door.closed_collider())),
        "ordinary physical clearance alone admits a step into the closed throat"
    );
    let response = local_move_step(
        agent,
        &obstructed,
        &HashMap::new(),
        &fixture.nav,
        0,
        away,
        8.0,
    );
    assert!(
        !matches!(response, MoveStep::Moved { position, yaw_degrees: _, speed_mps: _ } if crosses_leaf(agent, position, &door.closed_collider())),
        "no accepted steer may enter the closed throat"
    );
    Ok(())
}
