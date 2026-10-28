//! On-demand analysis and source-guarded, individually previewed corrections.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use super::actions::Popup;
use super::ui::Action;
use super::{App, safety};
use crate::engine::level::LevelDef;

impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This handler owns only its contextual commands; other commands return to the exhaustive application dispatcher."
    )]
    pub(super) fn safety_action(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::Safety => {
                self.navigation.visible = false;
                self.navigation.pick_goal = None;
                self.viewport.navigation_guides(Vec::new(), &[], None, None);
                self.start_safety()?;
                self.popup = Some(Popup::Safety {
                    index: self.safety_index,
                    preview: false,
                });
                self.highlight_finding(self.safety_index);
            }
            Action::Finding(forward) => self.navigate_finding(forward)?,
            Action::IgnoreFinding | Action::ClearFindingIgnore => {
                self.change_placement_ignore(action == Action::IgnoreFinding)?;
            }
            Action::IgnoredFindings => self.review_placement_ignores()?,
            Action::CancelSafety => {
                self.cancel_safety();
                self.status =
                    "Cancelling Safety at the next safe analysis boundary; source unchanged."
                        .into();
            }
            Action::ChooseFix => {
                let index = self.safety_index;
                let finding = self
                    .safety_report
                    .as_ref()
                    .and_then(|r| r.findings.get(index))
                    .ok_or("Run Safety first")?;
                if finding.fix.is_none() || finding.placement_ignore.is_some() {
                    return Err("This issue has no eligible correction".into());
                }
                if !self.safety_choices.remove(&index) {
                    let _inserted = self.safety_choices.insert(index);
                }
                self.status = format!(
                    "{} corrections chosen. Preview group checks all conflicts before changing anything.",
                    self.safety_choices.len()
                );
            }
            Action::PreviewFix | Action::PreviewGroup => {
                self.preview_safety(action == Action::PreviewGroup)?;
            }
            Action::ApplyFix => {
                let Some(Popup::Safety {
                    index: _,
                    preview: true,
                }) = self.popup
                else {
                    return Err("Preview this correction before applying it".into());
                };
                let (expected, candidate, _) = self
                    .safety_preview
                    .as_ref()
                    .ok_or("Preview this correction first")?
                    .clone();
                self.check_focused_edit(&candidate)?;
                let document = self.document.as_mut().ok_or("Open a project first")?;
                document.apply_checked_edit(&expected, candidate)?;
                self.popup = None;
                self.selected = None;
                self.changed();
                self.status =
                    "Applied the reviewed correction group atomically. Cmd/Ctrl Z restores the exact source.".into();
                self.start_safety()?;
            }
            Action::CancelFix => {
                let Some(Popup::Safety { index, preview: _ }) = self.popup else {
                    return Err("Open Safety first".into());
                };
                self.restore_preview();
                self.popup = Some(Popup::Safety {
                    index,
                    preview: false,
                });
            }
            Action::InspectFinding => {
                let Some(Popup::Safety { index, preview: _ }) = self.popup else {
                    return Err("Open Safety first".into());
                };
                self.restore_preview();
                self.highlight_finding(index);
                self.popup = None;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn navigate_finding(&mut self, forward: bool) -> Result<(), String> {
        let Some(Popup::Safety { index, preview: _ }) = self.popup else {
            return Err("Open Safety first".into());
        };
        self.restore_preview();
        let count = self
            .safety_report
            .as_ref()
            .map_or(0, |report| report.findings.len());
        let active = |candidate: &usize| {
            self.safety_report
                .as_ref()
                .and_then(|report| report.findings.get(*candidate))
                .is_some_and(|finding| finding.placement_ignore.is_none())
        };
        let next = if forward {
            (index.saturating_add(1)..count).find(active)
        } else {
            (0..index).rev().find(active)
        }
        .unwrap_or(index);
        self.safety_index = next;
        self.popup = Some(Popup::Safety {
            index: next,
            preview: false,
        });
        self.highlight_finding(next);
        Ok(())
    }
    fn preview_safety(&mut self, group: bool) -> Result<(), String> {
        let Some(Popup::Safety { index, preview: _ }) = self.popup else {
            return Err("Open Safety first".into());
        };
        let document = self.document.as_ref().ok_or("Open a project first")?;
        let report = self.safety_report.as_ref().ok_or("Run Safety first")?;
        let (candidate, explanation) = if group {
            let findings = self
                .safety_choices
                .iter()
                .filter_map(|i| report.findings.get(*i))
                .collect::<Vec<_>>();
            (
                safety::preview_group(document.raw(), &findings)?,
                format!(
                    "Previewing {} chosen compatible corrections. Apply commits them together in ONE undo step. Cancel preserves the exact original source. Reanalysis follows acceptance.",
                    findings.len()
                ),
            )
        } else {
            (
                self.fix_candidate(index)?,
                report
                    .findings
                    .get(index)
                    .and_then(|f| f.fix.as_ref())
                    .map_or_else(String::new, |f| f.explanation.clone()),
            )
        };
        self.check_focused_edit(&candidate)?;
        let expected = document.raw().clone();
        let level: LevelDef =
            serde_json::from_value(candidate.clone()).map_err(|e| e.to_string())?;
        crate::engine::loader::validate_level(&level)?;
        self.safety_preview = Some((expected, candidate, explanation));
        self.viewport.rebuild(&level);
        self.popup = Some(Popup::Safety {
            index,
            preview: true,
        });
        self.status =
            "Correction preview only. Source and history unchanged; Apply creates one undo step."
                .into();
        Ok(())
    }
    fn change_placement_ignore(&mut self, ignore: bool) -> Result<(), String> {
        let Some(Popup::Safety { index, preview: _ }) = self.popup else {
            return Err("Open Safety first".into());
        };
        self.restore_preview();
        let finding = self
            .safety_report
            .as_ref()
            .and_then(|report| report.findings.get(index))
            .cloned()
            .ok_or("Run Safety again to review the current placement")?;
        let document = self.document.as_mut().ok_or("Open a project first")?;
        if ignore {
            document.ignore_safety_finding(&finding)?;
        } else {
            let key = finding
                .placement_ignore
                .as_ref()
                .ok_or("No saved ignore for this finding")?;
            document.clear_safety_ignore(&key.key)?;
        }
        self.safety_report = None;
        self.safety_index = 0;
        self.start_safety()?;
        self.popup = Some(Popup::Safety {
            index: 0,
            preview: false,
        });
        self.status = if ignore {
                    "Ignored only this warning and placement context. Save persists it; Undo restores it."
                } else {
                    "Cleared this placement ignore. Current warning and correction eligibility will be reviewed again."
                }.into();
        Ok(())
    }

    fn review_placement_ignores(&mut self) -> Result<(), String> {
        let report = self.safety_report.as_ref().ok_or("Run Safety first")?;
        let index = report
            .findings
            .iter()
            .enumerate()
            .skip(self.safety_index.saturating_add(1))
            .chain(
                report
                    .findings
                    .iter()
                    .enumerate()
                    .take(self.safety_index.saturating_add(1)),
            )
            .find_map(|(index, finding)| finding.placement_ignore.as_ref().map(|_| index))
            .ok_or("No saved placement ignores in this review")?;
        self.restore_preview();
        self.safety_index = index;
        self.popup = Some(Popup::Safety {
            index,
            preview: false,
        });
        self.highlight_finding(index);
        Ok(())
    }

    pub(super) fn start_safety(&mut self) -> Result<(), String> {
        if self.safety_job.is_none() && self.safety_report.is_none() {
            let document = self.document.as_ref().ok_or("Open a project first")?;
            let level = document.level().clone();
            let raw = document.raw().clone();
            let (sender, receiver) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            self.safety_cancel = Some(Arc::clone(&cancel));
            let _worker = std::thread::Builder::new()
                .name("places-editor-safety".into())
                .spawn(move || {
                    let report = safety::analyze_cancellable(&level, &raw, &cancel);
                    let _sent = sender.send((raw, report));
                })
                .map_err(|error| format!("Cannot start safety analysis: {error}"))?;
            self.safety_job = Some(receiver);
        }
        Ok(())
    }

    fn fix_candidate(&self, index: usize) -> Result<serde_json::Value, String> {
        let document = self.document.as_ref().ok_or("Open a project first")?;
        let finding = self
            .safety_report
            .as_ref()
            .and_then(|report| report.findings.get(index))
            .ok_or("Run Safety first")?;
        safety::preview_corrections(document.raw(), std::slice::from_ref(finding))
            .into_iter()
            .next()
            .ok_or("No conservative correction for this finding")?
    }

    pub(super) fn poll_safety(&mut self) {
        let Some(receiver) = &self.safety_job else {
            return;
        };
        match receiver.try_recv() {
            Ok((raw, report)) => {
                self.safety_job = None;
                if self
                    .safety_cancel
                    .take()
                    .is_some_and(|token| token.load(Ordering::Relaxed))
                {
                    self.status = "Safety cancelled; source and history unchanged. Open Safety to run it again.".into();
                    return;
                }
                if self
                    .document
                    .as_ref()
                    .is_some_and(|document| document.raw() == &raw)
                {
                    let ignored = report
                        .findings
                        .iter()
                        .filter(|finding| finding.placement_ignore.is_some())
                        .count();
                    self.status = format!(
                        "Safety review: {} active entries, {ignored} saved placement ignores. Checks are bounded; review in context.",
                        report.findings.len().saturating_sub(ignored)
                    );
                    self.safety_choices.clear();
                    self.safety_report = Some(report);
                    if matches!(
                        self.popup,
                        Some(Popup::Safety {
                            index: _,
                            preview: _
                        })
                    ) {
                        self.highlight_finding(0);
                    }
                } else {
                    self.status = "Scene changed during safety analysis. Open Safety to analyze the current revision.".into();
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.safety_job = None;
                self.status =
                    "Safety analysis stopped; source unchanged. Reopen Safety to retry.".into();
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    fn highlight_finding(&mut self, index: usize) {
        if let Some(finding) = self
            .safety_report
            .as_ref()
            .and_then(|report| report.findings.get(index))
        {
            if finding.category == safety::SafetyCategory::Review {
                self.viewport.highlight(None, &[]);
                return;
            }
            self.selected = finding.objects.first().copied();
            self.viewport.select(self.selected);
            self.viewport
                .highlight(Some(finding.bounds), &finding.surfaces);
            self.viewport.frame_bounds(finding.bounds);
        }
    }

    pub(super) fn cancel_safety(&self) {
        if let Some(token) = &self.safety_cancel {
            token.store(true, Ordering::Relaxed);
        }
    }

    fn restore_preview(&mut self) {
        self.safety_preview = None;
        if matches!(
            self.popup,
            Some(Popup::Safety {
                index: _,
                preview: true
            })
        ) && let Some(document) = &self.document
        {
            self.viewport.rebuild(document.level());
        }
    }

    pub(super) fn close_popup(&mut self) {
        if self.lighting.painting || (self.popup.is_none() && self.lighting.preview_off) {
            self.lighting.painting = false;
            self.lighting.preview_off = false;
            if let Some(document) = &self.document {
                self.viewport.rebuild(document.level());
            }
        }
        if matches!(
            self.popup,
            Some(Popup::Safety {
                index: _,
                preview: _
            })
        ) {
            self.cancel_safety();
        }
        self.preview_audio.clear_world();
        self.sound_preview_until = None;
        if let Some(Popup::AuthoringForm(draft)) = &self.popup {
            self.viewport
                .set_environment_preview(draft.environment_before);
        }
        self.restore_preview();
        if matches!(
            self.popup,
            Some(Popup::Transform(_) | Popup::AuthoringForm(_) | Popup::Stairs(_))
        ) && let Some(document) = &self.document
        {
            self.viewport.rebuild(document.level());
        }
        if matches!(self.popup, Some(Popup::AuthoringForm(_))) {
            self.viewport.navigation_guides(Vec::new(), &[], None, None);
        }
        self.popup = None;
    }
}
