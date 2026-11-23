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
}

impl TriggerTrace {
    pub(super) fn begin_frame(&mut self, feet: Vec3) {
        self.points.clear();
        self.times.clear();
        self.start = 0.0_f64;
        self.end = 0.0_f64;
        self.recording = false;
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
            self.push(feet, (self.end - self.start).mul_add(fraction, self.start));
        }
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
    ) {
        self.points.truncate(self.command_start);
        self.times.truncate(self.command_start);
        if let Some((position, fraction)) = contact {
            self.point(position, fraction);
        }
        self.point(feet, arrival);
        self.point(feet, 1.0);
    }

    pub(super) fn vertical(
        &mut self,
        start: Vec3,
        peak: Option<(f32, f32)>,
        end_y: f32,
        delta: f32,
    ) {
        if !self.recording
            || delta <= 0.0
            || (peak.is_none() && start.y.to_bits() == end_y.to_bits())
        {
            return;
        }
        // Vertical integration uses the final accepted horizontal footprint.
        // Keep that solver order and the ballistic elapsed time without
        // inventing intermediate diagonal positions the solver never checked.
        if let Some(planar) = self.times.get_mut(self.command_start..) {
            planar.fill(self.start);
        }
        self.point(start, 0.0);
        if let Some((elapsed, y)) = peak {
            self.point(Vec3::new(start.x, y, start.z), f64::from(elapsed / delta));
        }
    }

    fn push(&mut self, feet: Vec3, time: f64) {
        if self.points.last() != Some(&feet) || self.times.last() != Some(&time) {
            self.points.push(feet);
            self.times.push(time);
        }
    }
}
