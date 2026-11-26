//! Essential regression coverage for full error retention, modal input ownership,
//! and four-card paging. Tiny strings/rows only; no assets, GPU or map bake.
use super::*;
use crate::game::ui::{SettingsAction, activate_settings_item};

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
    ui.set_status(message.clone(), true);
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
    ui.set_import_report(message);
    ui.scroll_details(1);
    assert_eq!(ui.status_page, 1);
    assert!(ui.status_is_modal() && !ui.status_is_error);
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
        4,
        "empty collections retain the picker, folder actions and Back"
    );
    assert_eq!(
        empty.hit_test([40.0, 225.0]).map(|hit| hit.index),
        Some(0),
        "the visible picker must activate the first trailing action"
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
    assert_eq!(ui.level_select_idx, 8);
    ui.level_entries.clear();
    ui.level_select_idx = 0;
    ui.move_level_row(true);
    assert_eq!(ui.level_select_idx, 3);
}

// The long warning must remain reachable and must not cover the Back button.
#[test]
fn advanced_self_shadows_have_a_complete_warning_and_live_toggle() -> Result<(), String> {
    let mut settings = Settings::default();
    let display = DisplayStatus::default();
    let mut ui = UiState::new();
    ui.settings_page = SettingsPage::Graphics;
    ui.advanced_expanded = true;
    let rows = settings_rows(ui.settings_page, &settings, &display, true);
    assert_eq!(rows.len(), ui.settings_page.item_count(true));
    let (index, preference) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| {
            row.kind == SettingsRowKind::Value(SettingsValue::RealCharacterSelfShadows)
        })
        .ok_or("missing advanced preference")?;
    assert_eq!(
        preference.label,
        "Real character self shadows (WARNING major performance hit)"
    );
    ui.settings_idx = index;
    for expected in [true, false] {
        let _action = activate_settings_item(
            SettingsPage::Graphics,
            index,
            &mut ui,
            &mut settings,
            &display,
            0,
        );
        assert_eq!(settings.real_character_self_shadows, expected);
        assert!(settings.take_pending_apply().graphics);
    }
    for selected in 0..rows.len() {
        ui.settings_idx = selected;
        let layout = menu_layout(AppState::Settings, &ui, &settings, &display);
        assert!(layout.rows.iter().any(|row| row.index == selected));
        assert!(
            layout.rows.windows(2).all(
                |pair| matches!(pair, [left, right] if left.bounds.bottom <= right.bounds.top)
            )
        );
    }
    Ok(())
}

// The reused Master track used to route every pointer edit to Master. Pin the
// shared layout and keyboard dispatch to the same independent setting IDs.
#[test]
fn audio_slider_tracks_and_keys_keep_their_own_setting() -> Result<(), String> {
    use crate::game::settings::AudioVolume;
    let settings = Settings::default();
    let display = DisplayStatus::default();
    let mut ui = UiState::new();
    ui.settings_page = SettingsPage::Audio;
    let rows = settings_rows(ui.settings_page, &settings, &display, false);
    assert_eq!(rows.len(), ui.settings_page.item_count(false));
    let layout = menu_layout(AppState::Settings, &ui, &settings, &display);
    let controls = [
        AudioVolume::Master,
        AudioVolume::Ambient,
        AudioVolume::PropSound,
        AudioVolume::Effects,
    ];
    for control in controls {
        let track = layout
            .rows
            .iter()
            .find(|row| {
                row.slider
                    .is_some_and(|(candidate, _)| candidate == control)
            })
            .ok_or("missing audio slider")?;
        assert_eq!(
            layout
                .hit_test([372.0, track.bounds.top + 8.0])
                .map(|hit| hit.intent),
            Some(MenuIntent::Volume(control))
        );
        let mut adjusted = settings.clone();
        let _action = activate_settings_item(
            SettingsPage::Audio,
            track.index,
            &mut ui,
            &mut adjusted,
            &display,
            -1,
        );
        for candidate in controls {
            let expected = if candidate == control { 0.95 } else { 1.0 };
            assert!((adjusted.audio_volume(candidate) - expected).abs() < f32::EPSILON);
        }
    }
    Ok(())
}

// Two tiny layouts protect the M/P Settings Back hit and confirm/cancel paths.
// Non-value directional dispatch previously triggered an immediate reset or rebind.
#[test]
fn settings_actions_require_activation_and_reset_requires_confirmation() -> Result<(), String> {
    let display = DisplayStatus::default();
    for state in [AppState::Settings, AppState::PauseSettings] {
        let mut ui = UiState::new();
        let mut settings = Settings::default();
        settings.muted = !settings.muted;
        let nondefault_mute = settings.muted;
        let layout = menu_layout(state, &ui, &settings, &display);
        let restore = layout
            .rows
            .iter()
            .find(|row| row.label == "Restore Defaults")
            .ok_or("missing Restore Defaults")?;
        let back = layout.rows.last().ok_or("missing Back")?;
        assert!(restore.bounds.bottom <= back.bounds.top, "targets overlap");
        for row in [restore, back] {
            for y in [row.bounds.top, row.bounds.bottom - 1.0] {
                assert_eq!(
                    layout.hit_test([40.0, y]).map(|hit| hit.index),
                    Some(row.index)
                );
            }
        }
        for (page, index) in [
            (SettingsPage::Root, 0),
            (SettingsPage::Root, restore.index),
            (SettingsPage::Root, back.index),
            (SettingsPage::Controls, 0),
        ] {
            for direction in [-1_i32, 1_i32] {
                assert_eq!(
                    activate_settings_item(
                        page,
                        index,
                        &mut ui,
                        &mut settings,
                        &display,
                        direction
                    ),
                    SettingsAction::None
                );
                assert!(
                    ui.restore_confirmation.is_none(),
                    "direction opened confirmation"
                );
                assert!(ui.rebinding_action.is_none(), "direction started a rebind");
                assert_eq!(settings.muted, nondefault_mute);
            }
        }
        for confirm in [false, true] {
            assert_eq!(
                activate_settings_item(
                    SettingsPage::Root,
                    restore.index,
                    &mut ui,
                    &mut settings,
                    &display,
                    0,
                ),
                SettingsAction::None
            );
            assert_eq!(settings.muted, nondefault_mute);
            assert_eq!(
                ui.restore_confirmation,
                Some(false),
                "Cancel starts selected"
            );
            let modal = menu_layout(state, &ui, &settings, &display);
            assert!(
                modal.hit_test([40.0, back.bounds.top]).is_none(),
                "underlying Back must not take clicks"
            );
            if confirm {
                let hit = modal
                    .hit_test([40.0, 135.0])
                    .ok_or("missing confirmation action")?;
                ui.select_menu_index(state, hit.index);
            }
            assert_eq!(ui.resolve_restore_defaults(&mut settings), confirm);
            assert_eq!(settings.muted, nondefault_mute != confirm);
        }
    }
    Ok(())
}

// Shared arrow/wheel dispatch must reach the final Graphics rows and keep the
// Static lighting restriction, without a GPU or real Settings files.
#[test]
fn expanded_graphics_pointer_scroll_reaches_lighting_and_respects_static_reflections()
-> Result<(), String> {
    use crate::engine::quality::LightmapQuality;
    let display = DisplayStatus::default();
    let mut settings = Settings::default();
    let _set_static = settings.set_lightmap_quality(LightmapQuality::Off);
    for state in [AppState::Settings, AppState::PauseSettings] {
        let mut ui = UiState {
            settings_page: SettingsPage::Graphics,
            settings_idx: 5,
            advanced_expanded: true,
            ..UiState::new()
        };
        let layout = menu_layout(state, &ui, &settings, &display);
        let down = layout
            .hit_test([450.0, 190.0])
            .ok_or("missing scroll arrow")?;
        assert_eq!(down.intent, MenuIntent::Scroll(1));
        ui.scroll_menu(state, 1);
        assert_eq!(ui.settings_scroll, 1);
        let scrolled = menu_layout(state, &ui, &settings, &display);
        let gap = [65.0, 86.0];
        assert!(
            scrolled.hit_test(gap).is_none(),
            "the point is in a row gap"
        );
        assert!(
            scrolled.contains_scroll_area(gap),
            "the wheel can scroll between rows"
        );
        for _ in 0_u8..2 {
            ui.scroll_menu(state, 1);
        }
        let tail = menu_layout(state, &ui, &settings, &display);
        let lighting = tail.rows.iter().find(|row| row.label == "Lighting");
        assert!(
            lighting.is_some_and(|row| row.enabled),
            "Lighting is reachable"
        );
        let reflections = tail
            .rows
            .iter()
            .find(|row| row.label == "Reflections")
            .ok_or("Reflections must be reachable")?;
        assert!(!reflections.enabled, "Static must disable Reflections");
        assert!(
            reflections.value.contains("Static lighting"),
            "disabled state is explained"
        );
        assert!(
            tail.hit_test([450.0, 190.0]).is_none(),
            "no scroll past end"
        );
        assert_eq!(
            tail.hit_test([450.0, 70.0]).map(|hit| hit.intent),
            Some(MenuIntent::Scroll(-1))
        );
        let mut full = settings.clone();
        let _set_full = full.set_lightmap_quality(LightmapQuality::Full);
        assert!(
            menu_layout(state, &ui, &full, &display)
                .rows
                .iter()
                .any(|row| row.label == "Reflections" && row.enabled),
            "Full lighting re-enables Reflections"
        );
    }
    Ok(())
}

// One in-memory gesture plus viewport geometry verifies live clamping, row
// ownership, and a single save request for a changed final value.
#[test]
fn audio_drag_clamps_the_captured_row_and_requests_one_save() -> Result<(), String> {
    use crate::game::settings::AudioVolume::{Ambient, Master};
    let mut settings = Settings::default();
    let mut ui = UiState::new();
    let drawable = DrawableSize::new(1000, 500);
    let window = (500, 250);
    let sample = |point| {
        drawable
            .window_to_ui_unclipped(window, point)
            .map(|[x, _]| x)
            .ok_or("captured pointer must map even outside the window")
    };
    let _initial_apply = settings.take_pending_apply();
    ui.start_volume_drag(Ambient, &settings);
    ui.update_volume_drag(&mut settings, 372.0);
    assert_eq!(settings.audio_volume(Ambient).to_bits(), 0.5_f32.to_bits());
    assert!(
        settings.take_pending_apply().audio,
        "live mixer update is due before release"
    );
    ui.update_volume_drag(&mut settings, sample([525.0, -30.0])?);
    assert_eq!(settings.audio_volume(Ambient).to_bits(), 1.0_f32.to_bits());
    assert!(
        !ui.finish_volume_drag(&settings),
        "returning to the original value needs no write"
    );
    ui.start_volume_drag(Ambient, &settings);
    ui.update_volume_drag(&mut settings, sample([-15.0, 300.0])?);
    assert_eq!(settings.audio_volume(Ambient).to_bits(), 0.0_f32.to_bits());
    assert_eq!(settings.audio_volume(Master).to_bits(), 1.0_f32.to_bits());
    assert!(
        ui.finish_volume_drag(&settings),
        "only finishing requests a write"
    );
    assert!(
        !ui.finish_volume_drag(&settings),
        "late release cannot request another write"
    );
    Ok(())
}

// Real SDL motion events exercise the native router's lost button-up path.
// No window, assets, or on-disk settings are needed.
#[test]
fn captured_audio_motion_without_left_button_commits_last_held_sample_once() {
    use crate::game::settings::AudioVolume::Ambient;
    use sdl3::mouse::MouseState;

    let mut settings = Settings::default();
    let mut ui = UiState::new();
    let hit = MenuHit {
        index: 1,
        intent: MenuIntent::Volume(Ambient),
    };
    assert_eq!(ui.pointer.button(true, Some(hit)), Some(hit));
    ui.start_volume_drag(Ambient, &settings);
    let motion = |x, xrel, left_held| Event::MouseMotion {
        timestamp: 0,
        window_id: 1,
        which: 0,
        mousestate: MouseState::from_sdl_state(u32::from(left_held)),
        x,
        y: 100.0,
        xrel,
        yrel: 0.0,
    };
    let track_x = |[x, _]: [f32; 2]| Some(x);
    assert_eq!(
        ui.handle_captured_volume_motion(&mut settings, &motion(372.0, 48.0, true), track_x),
        CapturedVolumeMotion::Held,
    );
    assert_eq!(settings.audio_volume(Ambient).to_bits(), 0.5_f32.to_bits());
    assert_eq!(
        ui.handle_captured_volume_motion(&mut settings, &motion(500.0, 0.0, false), track_x),
        CapturedVolumeMotion::Recovered { changed: true },
        "even stationary unpressed motion must recover before sampling its position"
    );
    assert!(
        !ui.volume_drag_active(),
        "keyboard input must not stay blocked"
    );
    assert_eq!(
        ui.handle_captured_volume_motion(&mut settings, &motion(0.0, -500.0, false), track_x),
        CapturedVolumeMotion::Unhandled,
        "later motion must not revive the drag or request another write"
    );
    assert_eq!(settings.audio_volume(Ambient).to_bits(), 0.5_f32.to_bits());
    assert_eq!(ui.recover_volume_drag(&settings, false), None);
    assert_eq!(
        ui.pointer.button(true, Some(hit)),
        Some(hit),
        "the next click is fresh"
    );
}
