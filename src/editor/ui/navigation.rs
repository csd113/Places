//! Navigation stays contextual and leaves the scene viewport unobstructed.

use super::{ACCENT, Action, Hit, INK, MUTED, button, text};
use crate::editor::App;
use crate::engine::render::Vertex;

pub(super) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let nav = &app.navigation;
    text(v, "NAVIGATION", 5.0, 43.0, ACCENT, 28);
    button(
        v,
        hits,
        [5.0, 52.0, 102.0, 11.0],
        if nav.is_baking() {
            "Baking current source..."
        } else {
            "Bake current source"
        },
        Action::NavBake,
        !nav.is_baking(),
    );
    text(
        v,
        if nav.is_current() {
            "CURRENT / authored doors"
        } else {
            "STALE / overlay hidden"
        },
        5.0,
        68.0,
        MUTED,
        29,
    );
    agent(app, v, hits);
    coverage(app, v, hits);
    if let Some(goal) = nav.pick_goal {
        text(
            v,
            if goal {
                "PICK GOAL / click a floor"
            } else {
                "PICK START / click a floor"
            },
            117.0,
            42.0,
            ACCENT,
            90,
        );
    }
    text(v, &nav.feedback, 117.0, 231.0, INK, 100);
}

fn agent(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let nav = &app.navigation;
    text(v, "AGENT / BODY", 5.0, 80.0, ACCENT, 28);
    text(
        v,
        nav.selected_agent()
            .map_or("Bake to list agents", |agent| agent.label.as_str()),
        5.0,
        90.0,
        INK,
        28,
    );
    text(v, &nav.body_label(), 5.0, 100.0, MUTED, 29);
    for (x, width, label, action, enabled) in [
        (
            5.0,
            18.0,
            "<",
            Action::NavAgent(false),
            nav.agent_count() > 1,
        ),
        (
            25.0,
            18.0,
            ">",
            Action::NavAgent(true),
            nav.agent_count() > 1,
        ),
        (
            45.0,
            62.0,
            "Start at agent",
            Action::NavUseAgent,
            nav.selected_agent()
                .is_some_and(|agent| agent.position.is_some()),
        ),
    ] {
        button(v, hits, [x, 109.0, width, 11.0], label, action, enabled);
    }
    text(v, &nav.movement_label(), 5.0, 124.0, MUTED, 29);
    for (x, width, label, action) in [
        (5.0, 49.0, "Pick start", Action::NavPick(false)),
        (57.0, 50.0, "Pick goal", Action::NavPick(true)),
    ] {
        button(
            v,
            hits,
            [x, 134.0, width, 11.0],
            label,
            action,
            nav.is_current(),
        );
    }
}

fn coverage(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let nav = &app.navigation;
    text(v, "COVERAGE / HEIGHT", 5.0, 151.0, ACCENT, 29);
    for (x, width, label, action) in [
        (
            5.0,
            49.0,
            if nav.coverage {
                "Coverage on"
            } else {
                "Coverage off"
            },
            Action::NavCoverage,
        ),
        (
            57.0,
            50.0,
            if nav.all_floors {
                "All floors"
            } else {
                "Slice only"
            },
            Action::NavAllFloors,
        ),
    ] {
        button(
            v,
            hits,
            [x, 160.0, width, 11.0],
            label,
            action,
            nav.is_current(),
        );
    }
    text(
        v,
        &format!("Y {:.1} +/- 0.75m", nav.slice_y),
        5.0,
        177.0,
        MUTED,
        23,
    );
    button(
        v,
        hits,
        [83.0, 173.0, 11.0, 11.0],
        "-",
        Action::NavSlice(false),
        nav.is_current(),
    );
    button(
        v,
        hits,
        [96.0, 173.0, 11.0, 11.0],
        "+",
        Action::NavSlice(true),
        nav.is_current(),
    );
    text(
        v,
        &format!("Shown {} / {}", nav.shown_cells, nav.total_cells),
        5.0,
        190.0,
        MUTED,
        29,
    );
    text(v, "Tint=region / red=blocked", 5.0, 201.0, MUTED, 29);
    text(v, "Moving props: verify in Play", 5.0, 211.0, MUTED, 29);
    button(
        v,
        hits,
        [5.0, 222.0, 102.0, 11.0],
        "Clear route",
        Action::NavClear,
        nav.is_current(),
    );
}
