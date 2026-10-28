//! Construction corrections enrich existing findings; no duplicate review panel.
use super::{Correction, SafetyCategory, SafetyFinding, SafetyFix, SafetyReport, SafetySeverity};
use crate::editor::document::ObjectId;
use crate::engine::level::{LevelDef, WallAxis};
use serde_json::Value;
use std::sync::Arc;

fn fix(
    expected: &Arc<Value>,
    title: &str,
    explanation: String,
    correction: Correction,
) -> SafetyFix {
    SafetyFix {
        title: title.into(),
        explanation,
        expected: Arc::clone(expected),
        correction,
    }
}
pub(super) fn corrections(
    level: &LevelDef,
    raw: &Value,
    report: &mut SafetyReport,
    cancel: &std::sync::atomic::AtomicBool,
) {
    let expected = Arc::new(raw.clone());
    for issue in crate::map_compiler::geometry_check::construction::endpoints(level)
        .into_iter()
        .filter(|issue| issue.auto_repairable)
    {
        if let Some(finding) = report.findings.iter_mut().find(|f| {
            f.check == issue.check
                && f.objects.contains(&ObjectId::Wall(issue.wall))
                && f.objects.contains(&ObjectId::Wall(issue.other))
        }) {
            finding.fix = Some(fix(
                &expected,
                "Weld / trim this endpoint",
                issue.explanation.clone(),
                Correction::Endpoint(issue),
            ));
        }
    }
    for joint in crate::map_compiler::geometry_check::wall_joints(level)
        .into_iter()
        .filter(|joint| joint.auto_repairable)
        .take(16)
    {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if let Some(finding) = report.findings.iter_mut().find(|f| {
            f.check == "wall-joint-step"
                && f.objects.contains(&ObjectId::Wall(joint.first))
                && f.objects.contains(&ObjectId::Wall(joint.second))
        }) {
            match crate::map_compiler::geometry_check::preview_alignment(raw,joint.first,joint.second){
                Ok(fields)=>finding.fix=Some(fix(&expected,"Align the shared wall planes","Aligns these shared wall planes and their supported trim/floor contacts. Ambiguous nearby attachments or a new geometry error refuse the correction. Apply is one undo step; reanalysis follows.".into(),Correction::Fields(fields))),
                Err(reason)=>{finding.risk.push_str(" Correction refused: ");finding.risk.push_str(&reason);}
            }
        }
    }
    for finding in &mut report.findings {
        if finding.check == "prop-no-collision"
            && let Some(ObjectId::Prop(index)) = finding.objects.first()
        {
            let supported = level
                .props
                .get(*index)
                .is_some_and(|p| p.physics.is_none() && p.float.is_none() && p.span.is_none());
            if supported {
                finding.fix=Some(fix(&expected,"Restore supported prop collision","Enables this catalog prop's fitted fixed shape when available, otherwise its ordinary solid envelope. Pose, model, materials and attachments stay fixed. This can obstruct a route: inspect the preview and Play-test it. Apply is one undo step.".into(),Correction::Collision(*index)));
            }
        }
    }
    joinable(level, raw, &expected, report);
    mounts(level, raw, &expected, report);
}
fn joinable(level: &LevelDef, raw: &Value, expected: &Arc<Value>, report: &mut SafetyReport) {
    let Some(records) = raw.get("walls").and_then(Value::as_array) else {
        return;
    };
    for (first, a) in level.walls.iter().enumerate() {
        for (second, b) in level.walls.iter().enumerate().skip(first.saturating_add(1)) {
            if a.axis() != b.axis()
                || a.width <= 0.0
                || a.depth <= 0.0
                || b.width <= 0.0
                || b.depth <= 0.0
            {
                continue;
            }
            let same = |x: f32, y: f32| (x - y).abs() < 0.0001;
            let overlap = match a.axis() {
                WallAxis::X => {
                    same(a.z, b.z)
                        && same(a.depth, b.depth)
                        && (a.x + a.width).min(b.x + b.width) - a.x.max(b.x) > 0.05
                }
                WallAxis::Z => {
                    same(a.x, b.x)
                        && same(a.width, b.width)
                        && (a.z + a.depth).min(b.z + b.depth) - a.z.max(b.z) > 0.05
                }
            };
            if !overlap || !same(a.y, b.y) || a.height != b.height {
                continue;
            }
            let (Some(left), Some(right)) = (records.get(first), records.get(second)) else {
                continue;
            };
            let comparable = |record: &Value| -> Option<Value> {
                let mut object = record.as_object()?.clone();
                if object.keys().any(|key| {
                    !matches!(
                        key.as_str(),
                        "x" | "y"
                            | "z"
                            | "width"
                            | "depth"
                            | "height"
                            | "material"
                            | "shine"
                            | "trim"
                    )
                }) {
                    return None;
                }
                for key in ["x", "z", "width", "depth"] {
                    drop(object.remove(key));
                }
                Some(Value::Object(object))
            };
            if comparable(left).is_none() || comparable(left) != comparable(right) {
                continue;
            }
            if left == right {
                continue;
            } // The existing exact-duplicate finding owns this.
            let objects = vec![ObjectId::Wall(first), ObjectId::Wall(second)];
            let Some(bounds) = super::reference_bounds(level, &objects) else {
                continue;
            };
            report.findings.push(SafetyFinding{check:"joinable-wall-solids",category:SafetyCategory::GeometryOverlap,severity:SafetySeverity::Info,
                title:format!("Walls {} / {} can share one solid",first.saturating_add(1),second.saturating_add(1)),
                risk:"These overlapping rectangular records have identical recognized surface/trim settings. Existing collinear union and covered-face clipping already render their contact correctly. Joining is optional source cleanup; different materials, openings, identities and unknown metadata require manual editing.".into(),
                position:super::bounds_center(bounds),bounds,objects,surfaces:Vec::new(),placement_ignore:None,
                fix:Some(fix(expected,"Join these compatible solids","Keeps the first wall and extends its existing span to the rectangular union, then removes the redundant second record. World-space materials and generated trim remain attached; every other source record stays exact. Preview first; Apply is one undo step.".into(),Correction::Join(first,second)))});
        }
    }
}
fn mounts(level: &LevelDef, raw: &Value, expected: &Arc<Value>, report: &mut SafetyReport) {
    for (index, prop) in level.props.iter().enumerate() {
        // Elevated/embedded scenery, physics, fitted spans, floating objects,
        // lights and water are not inferred to be misplaced floor props.
        if level
            .water
            .iter()
            .any(|water| water.contains(prop.x, prop.z))
        {
            continue;
        }
        if prop.y <= 0.004
            || prop.y > 0.05
            || prop.physics.is_some()
            || prop.float.is_some()
            || prop.span.is_some()
            || prop.pitch_degrees.abs() > 0.001
            || prop.roll_degrees.abs() > 0.001
        {
            continue;
        }
        let Ok(candidate) =
            crate::editor::document::mounting::candidate(level, raw, ObjectId::Prop(index))
        else {
            continue;
        };
        let Some(y) = candidate
            .get("props")
            .and_then(|a| a.get(index))
            .and_then(|p| p.get("y"))
            .and_then(Value::as_f64)
        else {
            continue;
        };
        let gap = f64::from(prop.y) - y;
        if !(0.003_f64..=0.05_f64).contains(&gap) {
            continue;
        }
        let objects = vec![ObjectId::Prop(index)];
        let Some(bounds) = super::reference_bounds(level, &objects) else {
            continue;
        };
        report.findings.push(SafetyFinding{check:"prop-floor-mount-gap",category:SafetyCategory::GeometryOverlap,severity:SafetySeverity::Warning,
            title:format!("{}: small floor mounting gap",prop.model),risk:format!("The measured model mounting back is {gap:.4} m above a continuous supporting surface. This can be intentional; the existing Mount to surface tool supplies the optional correction. Wall mounting and embedded scenery remain manual."),
            position:super::bounds_center(bounds),bounds,objects,surfaces:Vec::new(),placement_ignore:None,
            fix:Some(fix(expected,"Mount this prop to its surface","Reuses Mount to surface: measures real model bounds, preserves yaw/scale and refuses footprint steps or missing carriers. The preview shows any slope orientation change. Apply creates one undo step.".into(),Correction::Mount(index)))});
    }
}
