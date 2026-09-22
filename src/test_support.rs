//! Exact-value assertion helpers shared by the unit tests.
//!
//! These tests check the numbers the code produces, not tolerances: a baked
//! shade, a level's dimensions or a camera setting must come out exactly as
//! documented. IEEE equality is therefore written out as a `partial_cmp`
//! (`-0.0` equals `0.0`, NaN equals nothing) instead of being loosened to an
//! epsilon, and determinism checks can compare bit patterns so even a one-ULP
//! drift between two runs fails the test.

use std::cmp::Ordering;
use std::fmt::Display;

/// Asserts `actual` is exactly `expected` under IEEE equality.
#[track_caller]
pub fn assert_exact(actual: f32, expected: f32) {
    assert!(
        actual.partial_cmp(&expected) == Some(Ordering::Equal),
        "expected {expected}, got {actual}"
    );
}

/// Asserts `actual` is exactly `expected`, naming the case if it is not.
#[track_caller]
pub fn assert_exact_named(actual: f32, expected: f32, context: impl Display) {
    assert!(
        actual.partial_cmp(&expected) == Some(Ordering::Equal),
        "{context}: expected {expected}, got {actual}"
    );
}

/// Asserts every component of `actual` is exactly `expected`.
#[track_caller]
pub fn assert_exact_array<const N: usize>(actual: [f32; N], expected: [f32; N]) {
    for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
        assert!(
            actual.partial_cmp(expected) == Some(Ordering::Equal),
            "component {index}: expected {expected}, got {actual}"
        );
    }
}

/// Every `step` from `start` through `end`, both ends included.
///
/// The samples are produced by repeated addition, so a scan visits exactly the
/// values the accumulator-based loop it replaces did. The generator is used by
/// the continuity tests, which walk a surface in fixed increments and compare
/// each sample with the previous one.
pub fn scan(start: f32, step: f32, end: f32) -> impl Iterator<Item = f32> {
    scan_while(start, step, move |value| value <= end)
}

/// Every `step` from `start` up to, but not including, `end`.
pub fn scan_below(start: f32, step: f32, end: f32) -> impl Iterator<Item = f32> {
    scan_while(start, step, move |value| value < end)
}

fn scan_while(start: f32, step: f32, keep: impl Fn(f32) -> bool) -> impl Iterator<Item = f32> {
    let mut next = start;
    std::iter::from_fn(move || {
        let value = next;
        next += step;
        keep(value).then_some(value)
    })
}
