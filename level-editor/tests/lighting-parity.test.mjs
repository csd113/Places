// lighting-parity.test.mjs - Cross-implementation parity with the game.
//
// `src/lighting.rs` is authoritative; `js/lighting.js` mirrors it for the
// editor preview. Both test suites replay the exact same deterministic vector
// file (`tests/support/lighting_vectors.json`), generated from the Rust
// implementation. The Rust test asserts exact values; this one allows a
// tolerance because the editor preview is an approximation of the same model.
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

test('the shared parity vector file is present and complete', () => {
  assert.ok(Array.isArray(document.scenarios));
  assert.equal(document.scenarios.length, 8);
  for (const scenario of document.scenarios) {
    assert.equal(typeof scenario.name, 'string');
    assert.ok(scenario.level && Array.isArray(scenario.level.rooms));
    assert.ok(scenario.expect && Array.isArray(scenario.expect.rooms));
    assert.ok(Array.isArray(scenario.expect.samples));
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
      assert.ok(
        Math.abs(room.effectivePower - expected.effective_power) < TOLERANCE,
        `room ${index} effective power: ${room.effectivePower} vs ${expected.effective_power}`
      );
      assert.ok(
        Math.abs(room.baseline - expected.baseline) < TOLERANCE,
        `room ${index} baseline: ${room.baseline} vs ${expected.baseline}`
      );
    });

    for (const sample of scenario.expect.samples) {
      const value = baked.sampleInRoom(sample.room, sample.x, sample.y, sample.z);
      assert.ok(
        Number.isFinite(value),
        `sample (${sample.x}, ${sample.y}, ${sample.z}) must be finite`
      );
      assert.ok(
        Math.abs(value - sample.value) < TOLERANCE,
        `sample (${sample.x}, ${sample.y}, ${sample.z}): ${value} vs Rust ${sample.value}`
      );
    }
  });
}
