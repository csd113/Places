// lighting-parity.test.mjs - Cross-implementation parity with the game.
//
// `src/lighting.rs` is authoritative; `js/lighting.js` mirrors it for the
// editor preview. Both test suites replay the exact same deterministic vector
// file (`tests/support/lighting_vectors.json`), generated from the Rust
// implementation. The Rust test asserts exact values; this one allows a
// tolerance because the editor preview is an approximation of the same model.
//
// Baked illumination is three-channel RGB, so every vector is an `[r, g, b]`
// array and every channel is compared: a preview that silently dropped the
// colour would fail here.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import lighting from '../js/lighting.js';

const here = dirname(fileURLToPath(import.meta.url));
const vectorsPath = join(here, 'support', 'lighting_vectors.json');
const document = JSON.parse(readFileSync(vectorsPath, 'utf8'));

// The preview is flat-shaded and may round differently, so values must agree
// within a small tolerance rather than bit-for-bit. A drift of a few percent
// would mean the models have diverged; 0.02 is comfortably below anything a
// viewer would notice but far above float noise.
const TOLERANCE = 0.02;

function assertChannels(actual, expected, context) {
  assert.ok(Array.isArray(actual) && actual.length === 3, `${context}: expected an [r, g, b] triple`);
  assert.ok(Array.isArray(expected) && expected.length === 3, `${context}: vector is not a triple`);
  for (let channel = 0; channel < 3; channel++) {
    assert.ok(
      Number.isFinite(actual[channel]),
      `${context}[${channel}] must be finite, got ${actual[channel]}`
    );
    assert.ok(
      Math.abs(actual[channel] - expected[channel]) < TOLERANCE,
      `${context}[${channel}]: ${actual[channel]} vs Rust ${expected[channel]}`
    );
  }
}

test('the shared parity vector file is present and complete', () => {
  assert.ok(Array.isArray(document.scenarios));
  assert.equal(document.scenarios.length, 12);
  for (const scenario of document.scenarios) {
    assert.equal(typeof scenario.name, 'string');
    assert.ok(scenario.level && Array.isArray(scenario.level.rooms));
    assert.ok(scenario.expect && Array.isArray(scenario.expect.rooms));
    assert.ok(Array.isArray(scenario.expect.samples));
    for (const room of scenario.expect.rooms) {
      assert.ok(Array.isArray(room.baseline) && room.baseline.length === 3);
      assert.ok(Array.isArray(room.effective_power) && room.effective_power.length === 3);
    }
    for (const sample of scenario.expect.samples) {
      assert.ok(Array.isArray(sample.value) && sample.value.length === 3);
    }
  }
});

for (const scenario of document.scenarios) {
  test(`Rust/editor parity: ${scenario.name}`, () => {
    const baked = lighting.bakeLevelLighting(scenario.level);
    assert.equal(
      baked.rooms.length,
      scenario.expect.rooms.length,
      'room count must match'
    );

    scenario.expect.rooms.forEach((expected, index) => {
      const room = baked.rooms[index];
      assert.ok(
        Math.abs(room.area - expected.area) < 1e-4,
        `room ${index} area: ${room.area} vs ${expected.area}`
      );
      assert.equal(
        room.fixtureCount,
        expected.fixture_count,
        `room ${index} fixture ownership must match`
      );
      assertChannels(room.effectivePower, expected.effective_power, `room ${index} effective power`);
      assertChannels(room.baseline, expected.baseline, `room ${index} baseline`);
    });

    for (const sample of scenario.expect.samples) {
      const value = baked.sampleInRoom(sample.room, sample.x, sample.y, sample.z);
      assertChannels(value, sample.value, `sample (${sample.x}, ${sample.y}, ${sample.z})`);
    }
  });
}
