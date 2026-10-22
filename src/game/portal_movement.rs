//! Independent behavioral model. Numbers are Source units converted uniformly
//! at 0.025 metres/unit; evidence and unresolved retail parity are in the report.
//! No Valve movement implementation is incorporated here.
use glam::Vec2;

pub const UNIT: f32 = 0.025;
pub const TICK: f32 = 0.015;
pub const WALK_SPEED: f32 = 150.0 * UNIT;
pub const GRAVITY: f32 = 600.0 * UNIT;
pub const JUMP_SPEED: f32 = 160.0 * UNIT;
pub const DUCK_SECONDS: f32 = 0.4;
pub const UNDUCK_SECONDS: f32 = 0.2;

/// Provisional Portal predictor profile; high-speed retail air-movement restrictions
/// still require executable traces and are not inferred from HL2 or Portal 2.
/// Acceleration limits only velocity projected onto the requested direction.
/// The air cap limits that projection, while the uncapped wish speed controls
/// the acceleration budget. Existing perpendicular momentum survives.
pub fn accelerate(velocity: Vec2, wish: Vec2, speed: f32, air: bool, traction: f32) -> Vec2 {
    let direction = wish.normalize_or_zero();
    let cap = if air { speed.min(60.0 * UNIT) } else { speed };
    let remaining = (cap - velocity.dot(direction)).max(0.0);
    let rate = if air { 15.0 } else { 10.0 };
    Vec2::new(
        direction
            .x
            .mul_add(remaining.min(rate * speed * traction * TICK), velocity.x),
        direction
            .y
            .mul_add(remaining.min(rate * speed * traction * TICK), velocity.y),
    )
}

pub fn friction(velocity: Vec2, traction: f32) -> Vec2 {
    let speed = velocity.length();
    if speed < 0.1 * UNIT {
        return Vec2::ZERO;
    }
    let loss = speed.max(100.0 * UNIT) * 4.0 * traction * TICK;
    Vec2::new(
        velocity.x * ((speed - loss).max(0.0) / speed),
        velocity.y * ((speed - loss).max(0.0) / speed),
    )
}

/// The archived Portal fixture observes a 75-unit forward jump increment.
/// Express the documented facing-relative boost as a signed speed budget,
/// including its backwards-facing overspeed behavior, without a total-speed
/// clamp. Crouched launches use the smaller boost budget.
pub fn jump_boost(velocity: Vec2, facing: Vec2, forward: f32, speed: f32, ducked: bool) -> Vec2 {
    let gain = if ducked { 0.1 } else { 0.5 };
    let budget = speed
        .mul_add(1.0 + gain, -velocity.length())
        .min(forward.abs() * speed * gain);
    let sign = if forward < 0.0 { -1.0 } else { 1.0 };
    Vec2::new(
        facing.x.mul_add(budget * sign, velocity.x),
        facing.y.mul_add(budget * sign, velocity.y),
    )
}
