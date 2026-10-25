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
