//! Frame-relative positions accepted by the player controller.

use glam::Vec3;

#[derive(Default)]
pub(super) struct TriggerTrace {
    pub(super) points: Vec<Vec3>,
    pub(super) times: Vec<f64>,
    command_start: usize,
    start: f64,
    end: f64,
    recording: bool,
    vertical: Option<VerticalMotion>,
    merged: Vec<(Vec3, f64)>,
}

impl TriggerTrace {
    pub(super) fn begin_frame(&mut self, feet: Vec3) {
        self.points.clear();
        self.times.clear();
        self.start = 0.0_f64;
        self.end = 0.0_f64;
        self.recording = false;
        self.vertical = None;
        self.push(feet, 0.0);
        self.command_start = self.points.len();
    }

    pub(super) const fn begin_command(&mut self, end: f64) {
        self.command_start = self.points.len();
        self.start = self.end;
        self.end = end;
        self.recording = true;
    }

    pub(super) fn point(&mut self, feet: Vec3, fraction: f64) {
        if self.recording {
            self.push(feet, self.command_time(fraction));
        }
    }

    fn command_time(&self, fraction: f64) -> f64 {
        (self.end - self.start).mul_add(fraction, self.start)
    }

    pub(super) fn finish_frame(&mut self, feet: Vec3) {
        self.push(feet, 1.0);
        self.recording = false;
    }

    pub(super) fn replace_command(
        &mut self,
        contact: Option<(Vec3, f64)>,
        feet: Vec3,
        arrival: f64,
        keep_vertical: bool,
    ) {
        self.points.truncate(self.command_start);
        self.times.truncate(self.command_start);
        if !keep_vertical {
            self.vertical = None;
        }
        let base_y = self.vertical.as_mut().map(|vertical| {
            vertical.origin.x = feet.x;
            vertical.origin.z = feet.z;
            vertical.origin.y
        });
        let planar = |point: Vec3| Vec3::new(point.x, base_y.unwrap_or(point.y), point.z);
        if let Some((position, fraction)) = contact {
            self.point(planar(position), fraction);
        }
        self.point(planar(feet), arrival);
        self.point(planar(feet), 1.0);
    }

    pub(super) fn vertical(
        &mut self,
        origin: Vec3,
        peak: Option<(f32, f32)>,
        end_y: f32,
        delta: f32,
    ) {
        if self.recording
            && delta > 0.0
            && (peak.is_some() || origin.y.to_bits() != end_y.to_bits())
        {
            self.vertical = Some(VerticalMotion {
                origin,
                peak: peak.map(|(elapsed, y)| (self.command_time(f64::from(elapsed / delta)), y)),
                end_y,
            });
        }
    }

    pub(super) fn vertical_range(&self, end_y: f32) -> Option<(f32, f32)> {
        let vertical = self
            .vertical
            .filter(|motion| motion.end_y.to_bits() == end_y.to_bits())?;
        let peak = vertical.peak.map_or(vertical.origin.y, |(_, y)| y);
        Some((
            vertical.origin.y.min(vertical.end_y).min(peak),
            vertical.origin.y.max(vertical.end_y).max(peak),
        ))
    }

    pub(super) fn finish_command(&mut self, feet: Vec3, mut clear: impl FnMut(Vec3, Vec3) -> bool) {
        let Some(vertical) = self.vertical.take() else {
            self.point(feet, 1.0);
            return;
        };
        self.point(vertical.origin, 1.0);
        let first = self.command_start.saturating_sub(1);
        let mixed = self.points.get(first..).is_some_and(|points| {
            points
                .iter()
                .any(|point| (point.x, point.z) != (vertical.origin.x, vertical.origin.z))
        });
        if mixed && self.end > self.start && self.merge_command(first, vertical, &mut clear) {
            self.points.truncate(self.command_start);
            self.times.truncate(self.command_start);
            for &(point, time) in &self.merged {
                self.points.push(point);
                self.times.push(time);
            }
        } else {
            // Preserve the planar clock if raising the path would cut through
            // solid geometry. The vertical solve is valid at its own footprint.
            self.append_vertical(vertical);
        }
        self.point(feet, 1.0);
    }

    fn merge_command(
        &mut self,
        first: usize,
        vertical: VerticalMotion,
        clear: &mut impl FnMut(Vec3, Vec3) -> bool,
    ) -> bool {
        let Some(points) = self.points.get(first..) else {
            return false;
        };
        let Some(times) = self.times.get(first..) else {
            return false;
        };
        let mut peak = vertical.peak;
        let mut previous = points.first().copied().unwrap_or(vertical.origin);
        self.merged.clear();
        for ((&from, &to), (&start, &end)) in points
            .iter()
            .zip(points.iter().skip(1))
            .zip(times.iter().zip(times.iter().skip(1)))
        {
            if let Some((time, _)) = peak
                && start < time
                && time < end
            {
                let sample = from.lerp(to, interpolation_fraction(start, end, time));
                let point = vertical.combine(sample, time, self.start, self.end);
                if !clear(previous, point) {
                    return false;
                }
                self.merged.push((point, time));
                previous = point;
                peak = None;
            }
            let point = vertical.combine(to, end, self.start, self.end);
            if !clear(previous, point) {
                return false;
            }
            self.merged.push((point, end));
            previous = point;
        }
        true
    }

    fn append_vertical(&mut self, vertical: VerticalMotion) {
        let origin_index = self.command_start.saturating_sub(1);
        let mut arrival_index = self.points.len().saturating_sub(1);
        while arrival_index > origin_index
            && self.points.get(arrival_index.saturating_sub(1)) == Some(&vertical.origin)
        {
            arrival_index = arrival_index.saturating_sub(1);
        }
        let arrival = self.times.get(arrival_index).copied().unwrap_or(self.start);
        let retain = arrival_index.saturating_add(1);
        self.points.truncate(retain);
        self.times.truncate(retain);
        if let Some((time, y)) = vertical.peak {
            self.push(
                Vec3::new(vertical.origin.x, y, vertical.origin.z),
                time.max(arrival),
            );
        }
    }

    fn push(&mut self, feet: Vec3, time: f64) {
        if self.points.last() != Some(&feet) || self.times.last() != Some(&time) {
            self.points.push(feet);
            self.times.push(time);
        }
    }
}

#[derive(Clone, Copy)]
struct VerticalMotion {
    origin: Vec3,
    peak: Option<(f64, f32)>,
    end_y: f32,
}

impl VerticalMotion {
    fn combine(self, planar: Vec3, time: f64, start: f64, end: f64) -> Vec3 {
        let (low, high) = self
            .peak
            .map_or(((start, self.origin.y), (end, self.end_y)), |peak| {
                if time <= peak.0 {
                    ((start, self.origin.y), peak)
                } else {
                    (peak, (end, self.end_y))
                }
            });
        let y = if time <= start {
            self.origin.y
        } else if time >= end {
            self.end_y
        } else {
            (high.1 - low.1).mul_add(interpolation_fraction(low.0, high.0, time), low.1)
        };
        Vec3::new(planar.x, planar.y + (y - self.origin.y), planar.z)
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "the interpolation factor is clamped to the finite unit interval"
)]
fn interpolation_fraction(start: f64, end: f64, time: f64) -> f32 {
    if end > start {
        ((time - start) / (end - start)).clamp(0.0, 1.0) as f32
    } else {
        1.0
    }
}
