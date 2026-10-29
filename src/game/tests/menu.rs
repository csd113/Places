//! Essential regression coverage for full error retention, modal input ownership,
//! and four-card paging. Tiny strings/rows only; no assets, GPU or map bake.
use super::*;

#[test]
fn full_error_pages_retain_long_unicode_paths_and_own_input() {
    let message = format!(
        "Cannot load:\n/{}\nTry importing the package again.",
        "long_path_é".repeat(120)
    );
    let lines = wrap(&message, 56);
    assert!(lines.iter().all(|line| line.chars().count() <= 56));
    assert_eq!(
        lines
            .concat()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>(),
        message
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    );
    let mut ui = UiState::new();
    ui.set_status(message, true);
    for _ in 0_u8..100 {
        ui.scroll_details(1);
    }
    assert_eq!(ui.status_page, lines.len().div_ceil(12).saturating_sub(1));
    let layout = menu_layout(
        AppState::MainMenu,
        &ui,
        &Settings::default(),
        &DisplayStatus::default(),
    );
    assert_eq!(
        layout.hit_test([40.0, 240.0]).map(|hit| hit.intent),
        Some(MenuIntent::Dismiss)
    );
    assert_eq!(
        layout.hit_test([40.0, 216.0]).map(|hit| hit.intent),
        Some(MenuIntent::Page(-1))
    );
    assert_eq!(
        layout.hit_test([300.0, 216.0]).map(|hit| hit.intent),
        Some(MenuIntent::Page(1))
    );
}
#[test]
fn large_explore_collections_keep_actions_and_selected_card_reachable() {
    let mut ui = UiState {
        level_entries: (0_u16..500).map(|n| format!("Place {n}")).collect(),
        level_select_idx: 499,
        ..UiState::new()
    };
    ui.keep_selection_visible(AppState::LevelSelect);
    let layout = menu_layout(
        AppState::LevelSelect,
        &ui,
        &Settings::default(),
        &DisplayStatus::default(),
    );
    assert_eq!(layout.rows.iter().filter(|row| row.index < 500).count(), 4);
    assert!(layout.rows.iter().any(|row| row.index == 499));
    assert!(layout.rows.iter().all(|row| row.bounds.left >= 20.0
        && row.bounds.right <= 460.0
        && row.bounds.bottom <= 236.0));
    assert_eq!(
        layout.rows.last().map(|row| row.label.as_str()),
        Some("Back")
    );
    ui.level_entries.clear();
    ui.level_select_idx = 0;
    let empty = menu_layout(
        AppState::LevelSelect,
        &ui,
        &Settings::default(),
        &DisplayStatus::default(),
    );
    assert_eq!(
        empty.rows.len(),
        3,
        "empty collections retain Import, Folder and Back"
    );
}

// Protects player-visible diagnostics without altering rollback or logging.
// Two in-memory UI states only; unique coverage of the developer setting seam.
#[test]
fn recoverable_prop_diagnostics_follow_console_enabled_state() {
    let mut ui = UiState::new();
    ui.developer_diagnostic("Ragdoll refused: fixture".to_string(), false);
    assert!(ui.status_message.is_none() && !ui.status_is_error);
    ui.developer_diagnostic("Ragdoll refused: fixture".to_string(), true);
    assert_eq!(
        ui.status_message.as_deref(),
        Some("Ragdoll refused: fixture")
    );
    assert!(ui.status_is_error);
    ui.clear_status();
    ui.set_status("Map cannot load", true);
    assert!(
        ui.status_is_error,
        "real blocking failures remain unconditional"
    );
}

// The retry button must own its click and cannot fall through to pause actions.
// Tiny layout fixture; no renderer, assets or native window.
#[test]
fn budget_recovery_buttons_keep_destructive_choice_explicit() {
    let mut ui = UiState::new();
    ui.set_status("Keeping both worlds exceeds the budget", true);
    ui.load_retry = Some(());
    let layout = menu_layout(
        AppState::Paused,
        &ui,
        &Settings::default(),
        &DisplayStatus::default(),
    );
    assert_eq!(
        layout.hit_test([90.0, 240.0]).map(|hit| hit.intent),
        Some(MenuIntent::RetryLoad)
    );
    assert_eq!(
        layout.hit_test([380.0, 240.0]).map(|hit| hit.intent),
        Some(MenuIntent::Dismiss)
    );
    ui.clear_status();
    assert!(ui.load_retry.is_none());
}

// Essential regression for overflowing card titles and the full-name view.
// Four tiny rows verify glyph containment and exclusive details hit ownership.
#[test]
fn card_titles_and_previews_fit_and_details_retain_the_full_name() {
    let name = "A very long place name ".repeat(20);
    let ui = UiState {
        level_entries: vec![name.clone(); 4],
        ..UiState::new()
    };
    let layout = menu_layout(
        AppState::LevelSelect,
        &ui,
        &Settings::default(),
        &DisplayStatus::default(),
    );
    let mut vertices = Vec::new();
    draw_rows(&mut vertices, &layout, AppState::LevelSelect, &ui);
    for card in layout.rows.iter().filter(|row| row.index < 4) {
        let mut isolated_card = menu_layout(
            AppState::LevelSelect,
            &ui,
            &Settings::default(),
            &DisplayStatus::default(),
        );
        isolated_card
            .rows
            .retain(|candidate| candidate.index == card.index);
        isolated_card.scroll = None;
        let mut card_vertices = Vec::new();
        draw_rows(
            &mut card_vertices,
            &isolated_card,
            AppState::LevelSelect,
            &ui,
        );
        assert!(
            card_vertices
                .iter()
                .all(|vertex| vertex.pos[0] >= card.bounds.left
                    && vertex.pos[0] <= card.bounds.right
                    && vertex.pos[1] >= card.bounds.top
                    && vertex.pos[1] <= card.bounds.bottom)
        );
        let [x, y, w, h] = level_icon_bounds(card);
        assert!(
            x >= card.bounds.left
                && x + w <= card.bounds.right
                && y >= card.bounds.top
                && y + h <= card.bounds.bottom
        );
    }
    assert!(
        vertices.iter().all(|v| v.pos[0] >= 30.0
            && v.pos[0] <= 458.0
            && v.pos[1] >= 50.0
            && v.pos[1] <= 236.0)
    );
    assert!(ui.place_details().starts_with(&name));
    let details_ui = UiState {
        level_description: ui.place_details(),
        details_open: Some(()),
        ..ui
    };
    assert_eq!(details_ui.place_details(), details_ui.level_description);
    let details_layout = menu_layout(
        AppState::LevelSelect,
        &details_ui,
        &Settings::default(),
        &DisplayStatus::default(),
    );
    assert_eq!(
        details_layout.hit_test([45.0, 240.0]).map(|hit| hit.intent),
        Some(MenuIntent::Details)
    );
    assert_eq!(
        details_layout.hit_test([80.0, 90.0]).map(|hit| hit.intent),
        Some(MenuIntent::Page(-1))
    );
}

// A five-map grid previously skipped the first import action on Down.
// One tiny row-state fixture protects odd/empty grids without a native window.
#[test]
fn odd_grid_rows_reach_pinned_actions_and_scroll_the_selected_card() {
    let mut ui = UiState {
        level_entries: vec!["Place".into(); 5],
        level_select_idx: 4,
        ..UiState::new()
    };
    ui.move_level_row(false);
    assert_eq!(ui.level_select_idx, 5);
    ui.move_level_row(true);
    assert_eq!(ui.level_select_idx, 4);
    assert!(ui.level_scroll > 0);
    ui.level_select_idx = 1;
    ui.move_level_row(true);
    assert_eq!(ui.level_select_idx, 7);
    ui.level_entries.clear();
    ui.level_select_idx = 0;
    ui.move_level_row(true);
    assert_eq!(ui.level_select_idx, 2);
}
