//! Focused review keeps diagnostics and individual fixes readable on demand.

use super::{ACCENT, Action, Hit, INK, MUTED, PANEL, button, rect, text};
use crate::editor::App;
use crate::editor::safety::{SafetyCategory, SafetyFinding};
use crate::engine::render::Vertex;

pub(in crate::editor) fn safety_popup(
    app: &App,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    index: usize,
    preview: bool,
) {
    if preview && let Some((_, _, explanation)) = &app.safety_preview {
        fix_preview(v, hits, explanation);
        return;
    }
    rect(v, [0.0, 0.0, 480.0, 272.0], [0.0, 0.0, 0.0, 0.6]);
    rect(v, [118.0, 44.0, 356.0, 194.0], PANEL);
    text(v, "CONSTRUCTION / SAFETY", 128.0, 54.0, ACCENT, 64);
    button(
        v,
        hits,
        [434.0, 50.0, 30.0, 11.0],
        "Close",
        Action::ClosePopup,
        true,
    );
    if app.safety_job.is_some() {
        text(
            v,
            "Checking geometry and placed model surfaces...",
            128.0,
            79.0,
            INK,
            94,
        );
        text(
            v,
            "The source scene is unchanged while checks run.",
            128.0,
            92.0,
            MUTED,
            94,
        );
        button(
            v,
            hits,
            [128.0, 112.0, 116.0, 13.0],
            "Cancel analysis",
            Action::CancelSafety,
            true,
        );
        return;
    }
    let Some(report) = &app.safety_report else {
        text(
            v,
            "Analysis stopped or changed. Run the checks again.",
            128.0,
            79.0,
            INK,
            94,
        );
        button(
            v,
            hits,
            [128.0, 100.0, 116.0, 13.0],
            "Run checks",
            Action::Safety,
            app.document.is_some(),
        );
        return;
    };
    let Some(finding) = report.findings.get(index) else {
        text(v, "No findings from these checks.", 128.0, 80.0, INK, 94);
        wrapped(v, &report.limitations, [128.0, 100.0], MUTED, 94, 12);
        return;
    };
    finding_details(
        v,
        hits,
        finding,
        index,
        report.findings.len(),
        report
            .findings
            .iter()
            .filter(|entry| entry.placement_ignore.is_some())
            .count(),
        [
            report
                .findings
                .iter()
                .take(index)
                .any(|entry| entry.placement_ignore.is_none()),
            report
                .findings
                .iter()
                .skip(index.saturating_add(1))
                .any(|entry| entry.placement_ignore.is_none()),
        ],
    );
    group_controls(app, v, hits, finding, index);
}

fn fix_preview(v: &mut Vec<Vertex>, hits: &mut Vec<Hit>, explanation: &str) {
    rect(v, [118.0, 178.0, 356.0, 60.0], PANEL);
    text(
        v,
        "FIX PREVIEW / source unchanged",
        128.0,
        185.0,
        ACCENT,
        84,
    );
    button(
        v,
        hits,
        [434.0, 182.0, 30.0, 11.0],
        "Close",
        Action::ClosePopup,
        true,
    );
    wrapped(v, explanation, [128.0, 198.0], INK, 94, 2);
    button(
        v,
        hits,
        [128.0, 220.0, 116.0, 13.0],
        "Back to finding",
        Action::CancelFix,
        true,
    );
    button(
        v,
        hits,
        [252.0, 220.0, 116.0, 13.0],
        "Apply preview",
        Action::ApplyFix,
        true,
    );
}

fn finding_details(
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    finding: &SafetyFinding,
    index: usize,
    total: usize,
    ignored: usize,
    [previous_active, next_active]: [bool; 2],
) {
    text(
        v,
        &format!(
            "{} of {} / {} / {}",
            index.saturating_add(1),
            total,
            finding.placement_ignore.as_ref().map_or_else(
                || finding.severity.label(),
                |state| {
                    if state.active {
                        "Ignored"
                    } else {
                        "Inactive ignore"
                    }
                }
            ),
            finding.category.label()
        ),
        128.0,
        68.0,
        MUTED,
        94,
    );
    text(v, &finding.title, 128.0, 81.0, INK, 94);
    wrapped(v, &finding.risk, [128.0, 94.0], INK, 94, 4);
    correction(v, hits, finding);
    button(
        v,
        hits,
        [128.0, 182.0, 116.0, 13.0],
        if finding.category == SafetyCategory::Review {
            "Return to viewport"
        } else {
            "View in viewport"
        },
        Action::InspectFinding,
        true,
    );
    if finding.placement_ignore.is_some() {
        button(
            v,
            hits,
            [252.0, 182.0, 150.0, 13.0],
            "Clear this placement ignore",
            Action::ClearFindingIgnore,
            true,
        );
    } else if crate::editor::document::intent::can_ignore(finding) {
        button(
            v,
            hits,
            [252.0, 182.0, 150.0, 13.0],
            "Ignore for this placement",
            Action::IgnoreFinding,
            true,
        );
    }
    button(
        v,
        hits,
        [252.0, 200.0, 150.0, 13.0],
        &format!("Review saved ignores ({ignored})"),
        Action::IgnoredFindings,
        ignored > 0,
    );
    button(
        v,
        hits,
        [376.0, 222.0, 42.0, 11.0],
        "< Prev",
        Action::Finding(false),
        previous_active,
    );
    button(
        v,
        hits,
        [422.0, 222.0, 42.0, 11.0],
        "Next >",
        Action::Finding(true),
        next_active,
    );
}

fn correction(v: &mut Vec<Vertex>, hits: &mut Vec<Hit>, finding: &SafetyFinding) {
    if let Some(state) = &finding.placement_ignore {
        text(v, "SAVED PLACEMENT INTENT", 128.0, 136.0, ACCENT, 94);
        wrapped(
            v,
            if state.active {
                "This specific warning is ignored. Automatic corrections protect its placement and support context. Clear to review correction eligibility."
            } else {
                "This saved decision is inactive or its warning is absent. New warnings remain visible. Clear the obsolete record here."
            },
            [128.0, 147.0],
            MUTED,
            94,
            3,
        );
    } else if let Some(fix) = &finding.fix {
        text(v, "AVAILABLE CORRECTION", 128.0, 134.0, ACCENT, 94);
        text(v, &fix.title, 128.0, 145.0, INK, 94);
        wrapped(v, &fix.explanation, [128.0, 155.0], MUTED, 94, 2);
        button(
            v,
            hits,
            [128.0, 200.0, 116.0, 13.0],
            "Preview fix",
            Action::PreviewFix,
            true,
        );
    } else if finding.category == SafetyCategory::Review {
        text(v, "REVIEW GUIDANCE", 128.0, 145.0, ACCENT, 94);
        text(
            v,
            "Review these limits and use Play to check the relevant areas.",
            128.0,
            158.0,
            MUTED,
            94,
        );
    } else {
        text(v, "MANUAL REVIEW", 128.0, 145.0, ACCENT, 94);
        text(
            v,
            "No automatic correction: review the highlighted geometry in context.",
            128.0,
            158.0,
            MUTED,
            94,
        );
    }
}

fn wrapped(
    v: &mut Vec<Vertex>,
    value: &str,
    [x, y]: [f32; 2],
    color: [f32; 4],
    width: usize,
    rows: u16,
) {
    let columns = width.saturating_mul(7) / 8;
    let mut line = String::new();
    let mut row = 0_u16;
    for word in value.split_whitespace() {
        if !line.is_empty()
            && line
                .chars()
                .count()
                .saturating_add(word.chars().count())
                .saturating_add(1)
                > columns
        {
            text(v, &line, x, f32::from(row).mul_add(9.0, y), color, width);
            line.clear();
            row = row.saturating_add(1);
            if row >= rows {
                return;
            }
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() && row < rows {
        text(v, &line, x, f32::from(row).mul_add(9.0, y), color, width);
    }
}

fn group_controls(
    app: &App,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    finding: &SafetyFinding,
    index: usize,
) {
    if finding.fix.is_some() && finding.placement_ignore.is_none() {
        button(
            v,
            hits,
            [128.0, 219.0, 116.0, 13.0],
            if app.safety_choices.contains(&index) {
                "Remove choice"
            } else {
                "Choose for group"
            },
            Action::ChooseFix,
            true,
        );
    }
    button(
        v,
        hits,
        [252.0, 219.0, 116.0, 13.0],
        &format!("Preview group ({})", app.safety_choices.len()),
        Action::PreviewGroup,
        !app.safety_choices.is_empty(),
    );
}
