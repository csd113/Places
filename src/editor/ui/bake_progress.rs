//! Current-pass feedback. A full track completes one counted pass, never the job.
use crate::engine::render::Vertex;

use super::{ACCENT, Action, Hit, INK, MUTED, PANEL, button, rect, text};
use crate::editor::App;

pub(super) fn draw(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let Some(job) = &app.compile else {
        return;
    };
    // Keep feedback and cancellation visible above popups without blocking editing.
    rect(vertices, [0.0, 242.0, 480.0, 30.0], PANEL);
    text(vertices, job.scope(), 5.0, 244.0, INK, 117);
    if app.popup.is_some() {
        button(
            vertices,
            hits,
            [421.0, 243.0, 53.0, 8.0],
            if job.cancelling() {
                "Stopping..."
            } else {
                "Cancel"
            },
            Action::Cancel,
            !job.cancelling(),
        );
    }
    let progress = job.progress();
    let stage = progress.map_or_else(
        || "Starting compiler / waiting for measured work".into(),
        |p| {
            format!(
                "{} / {} / {}",
                p.source,
                if p.variant.is_empty() {
                    "shared"
                } else {
                    &p.variant
                },
                p.stage
            )
        },
    );
    text(vertices, &stage, 5.0, 253.0, INK, 133);
    rect(vertices, [5.0, 263.0, 260.0, 5.0], [0.16, 0.22, 0.27, 1.0]);
    let measured_work = progress
        .and_then(|p| p.work.as_ref())
        .filter(|w| w.total > 0);
    let detail = if let Some(work) = measured_work {
        // Integer scaling keeps even maximum protocol counts bounded and avoids
        // lossy usize-to-float conversions. Only the final pixel width is f32.
        let completed = u128::try_from(work.completed.min(work.total)).unwrap_or(u128::MAX);
        let total = u128::try_from(work.total).unwrap_or(u128::MAX);
        let scaled = completed
            .saturating_mul(1000)
            .checked_div(total)
            .unwrap_or(0);
        let fraction = f32::from(u16::try_from(scaled).unwrap_or(1000)) / 1000.0;
        rect(vertices, [5.0, 263.0, 260.0 * fraction, 5.0], ACCENT);
        format!(
            "{}/{} {} / this pass",
            work.completed, work.total, work.unit
        )
    } else {
        // An animated segment explicitly conveys an unmeasured stage, not a
        // percentage, ETA, or a completed publication.
        let phase = (job.elapsed().as_secs_f32() * 0.7).fract();
        rect(
            vertices,
            [phase.mul_add(218.0, 5.0), 263.0, 42.0, 5.0],
            ACCENT,
        );
        "Indeterminate / uncounted stage".into()
    };
    text(
        vertices,
        if job.cancelling() {
            "Cancelling safely / awaiting compiler"
        } else {
            &detail
        },
        273.0,
        263.0,
        MUTED,
        57,
    );
}
