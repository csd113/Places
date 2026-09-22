// lighting.test.mjs - The editor's static-lighting mirror (js/lighting.js).
//
// These tests cover the externally meaningful behaviour the game's
// src/lighting.rs tests also cover, so an author can trust the preview's
// relative brightness and colour even though it is an approximation.
import test from 'node:test';
import assert from 'node:assert/strict';
import lighting from '../js/lighting.js';
import geometry from '../js/geometry.js';
import model from '../js/model.js';

const {
  TUNING, bakeLevelLighting, sanitizeIntensity, sanitizeColor, emittedColor,
  roomBaseline, smoothFalloff, luminance
} = lighting;

function levelWith(rooms, lights, walls) {
  return {
    rooms,
    walls: walls || [],
    ceiling_lights: lights || [],
    props: [],
    getCeilingHeight: () => 3.5
  };
}

function room(x, z, width, depth, height) {
  return { id: `room_${x}_${z}`, x, z, width, depth, height };
}

function light(x, z, extra) {
  return Object.assign({ id: `light_${x}_${z}`, fixture: 'core:fluorescent_panel_01', x, z }, extra || {});
}

test('more fixtures raise a room baseline and larger rooms lower it', () => {
  const one = bakeLevelLighting(levelWith([room(0, 0, 20, 20, 3.5)], [light(10, 10)]));
  const four = bakeLevelLighting(levelWith(
    [room(0, 0, 20, 20, 3.5)],
    [light(5, 5), light(15, 5), light(5, 15), light(15, 15)]
  ));
  assert.ok(luminance(one.rooms[0].baseline) < luminance(four.rooms[0].baseline));

  const large = bakeLevelLighting(levelWith([room(0, 0, 60, 60, 3.5)], [light(30, 30)]));
  assert.ok(luminance(large.rooms[0].baseline) < luminance(one.rooms[0].baseline));
  assert.ok(luminance(large.rooms[0].baseline) >= TUNING.AMBIENT_LEVEL);
});

test('fixture intensity scales the baseline and defaults to one', () => {
  const weak = bakeLevelLighting(levelWith([room(0, 0, 16, 16, 3.5)], [light(8, 8, { brightness: 0.5 })]));
  const standard = bakeLevelLighting(levelWith([room(0, 0, 16, 16, 3.5)], [light(8, 8)]));
  const strong = bakeLevelLighting(levelWith([room(0, 0, 16, 16, 3.5)], [light(8, 8, { brightness: 2.0 })]));
  assert.ok(luminance(weak.rooms[0].baseline) < luminance(standard.rooms[0].baseline));
  assert.ok(luminance(standard.rooms[0].baseline) < luminance(strong.rooms[0].baseline));

  // The `intensity` alias is accepted too.
  const alias = bakeLevelLighting(levelWith([room(0, 0, 16, 16, 3.5)], [light(8, 8, { intensity: 2.0 })]));
  assert.deepEqual(alias.rooms[0].baseline, strong.rooms[0].baseline);
  assert.equal(alias.lights[0].intensity, 2.0);
});

test('higher ceilings make the same fixtures less effective', () => {
  const low = bakeLevelLighting(levelWith([room(0, 0, 16, 16, 2.6)], [light(8, 8)]));
  const normal = bakeLevelLighting(levelWith([room(0, 0, 16, 16, 3.5)], [light(8, 8)]));
  const tall = bakeLevelLighting(levelWith([room(0, 0, 16, 16, 5.0)], [light(8, 8)]));
  assert.ok(luminance(low.rooms[0].baseline) > luminance(normal.rooms[0].baseline));
  assert.ok(luminance(normal.rooms[0].baseline) > luminance(tall.rooms[0].baseline));
});

test('an unlit room stays at the small ambient fill, never black and never bright', () => {
  const empty = bakeLevelLighting(levelWith([room(0, 0, 12, 12, 3.0)], []));
  assert.deepEqual(empty.rooms[0].baseline, [TUNING.AMBIENT_LEVEL, TUNING.AMBIENT_LEVEL, TUNING.AMBIENT_LEVEL]);
  assert.deepEqual(empty.sample(6, 0, 6), [TUNING.AMBIENT_LEVEL, TUNING.AMBIENT_LEVEL, TUNING.AMBIENT_LEVEL]);
  assert.ok(TUNING.AMBIENT_LEVEL > 0);
  // Regression guard against the historical 0.55 floor.
  assert.ok(TUNING.AMBIENT_LEVEL < 0.2);
});

test('malformed intensities are sanitized to finite, non-negative values', () => {
  assert.equal(sanitizeIntensity(NaN), 1.0);
  assert.equal(sanitizeIntensity(-4), 0.0);
  assert.equal(sanitizeIntensity('nonsense'), 1.0);
  assert.equal(sanitizeIntensity(1e30), TUNING.MAX_LIGHT_INTENSITY);
  assert.ok(sanitizeIntensity(Infinity) > 0);
  const level = levelWith([room(0, 0, 10, 10, 3.0)], [light(5, 5, { brightness: -2 })]);
  const baked = bakeLevelLighting(level);
  assert.ok(Number.isFinite(luminance(baked.sample(5, 0, 5))));
  assert.ok(luminance(baked.sample(5, 0, 5)) >= TUNING.AMBIENT_LEVEL);
});

test('malformed colours are sanitized and omitted colours use the warm default', () => {
  assert.deepEqual(sanitizeColor(undefined), TUNING.DEFAULT_LIGHT_COLOR);
  assert.deepEqual(sanitizeColor(null), TUNING.DEFAULT_LIGHT_COLOR);
  assert.deepEqual(sanitizeColor([1.5, -1, 0.5]), [1, 0, 0.5]);
  assert.deepEqual(sanitizeColor([NaN, Infinity, 0.25]), [0, 1, 0.25]);
  assert.deepEqual(emittedColor({}), TUNING.DEFAULT_LIGHT_COLOR);
  assert.deepEqual(emittedColor({ color: [0, 0, 1] }), [0, 0, 1]);
  const baked = bakeLevelLighting(levelWith([room(0, 0, 10, 10, 3)], [light(5, 5)]));
  assert.deepEqual(baked.lights[0].color, TUNING.DEFAULT_LIGHT_COLOR);
});

test('coloured fixtures tint the baked samples in their own channels', () => {
  const red = bakeLevelLighting(levelWith([room(0, 0, 16, 16, 3)], [light(8, 8, { color: [1, 0, 0] })]));
  const blue = bakeLevelLighting(levelWith([room(0, 0, 16, 16, 3)], [light(8, 8, { color: [0, 0, 1] })]));
  const redSample = red.sample(8, 0, 8);
  const blueSample = blue.sample(8, 0, 8);
  assert.ok(redSample[0] > redSample[1] + 0.1, `red fixture must dominate red: ${redSample}`);
  assert.ok(blueSample[2] > blueSample[0] + 0.1, `blue fixture must dominate blue: ${blueSample}`);
  assert.equal(redSample[1], TUNING.AMBIENT_LEVEL);
  assert.equal(redSample[2], TUNING.AMBIENT_LEVEL);

  // Two colours in one room accumulate instead of one replacing the other.
  const mixed = bakeLevelLighting(levelWith(
    [room(0, 0, 16, 8, 3)],
    [light(4, 4, { color: [1, 0, 0] }), light(12, 4, { color: [0, 0, 1] })]
  ));
  const middle = mixed.sample(8, 0, 4);
  assert.ok(middle[0] > TUNING.AMBIENT_LEVEL + 0.05, `red must survive: ${middle}`);
  assert.ok(middle[2] > TUNING.AMBIENT_LEVEL + 0.05, `blue must survive: ${middle}`);
});

test('saturation keeps extreme fixture counts inside the allowed range', () => {
  const lights = [];
  for (let i = 0; i < 200; i++) lights.push(light(2 + (i % 10) * 0.2, 2 + Math.floor(i / 10) * 0.2, { brightness: 2 }));
  const baked = bakeLevelLighting(levelWith([room(0, 0, 4, 4, 3.5)], lights));
  assert.ok(luminance(baked.rooms[0].baseline) <= TUNING.MAX_BRIGHTNESS);
  assert.ok(luminance(baked.rooms[0].baseline) > 0.85);
  assert.ok(Number.isFinite(luminance(baked.sample(2, 0, 2))));
  assert.ok(luminance(baked.sample(2, 0, 2)) <= TUNING.MAX_BRIGHTNESS);

  // The pure curve never exceeds its bounds either.
  assert.deepEqual(roomBaseline(0, [0, 0, 0]), [TUNING.AMBIENT_LEVEL, TUNING.AMBIENT_LEVEL, TUNING.AMBIENT_LEVEL]);
  assert.deepEqual(
    roomBaseline(0, [Number.MAX_VALUE, Number.MAX_VALUE, Number.MAX_VALUE]),
    [1, 1, 1]
  );
  // A finite extreme saturates just below the maximum, exactly like the game.
  assert.ok(luminance(roomBaseline(0, [1e30, 1e30, 1e30])) > 0.98);
  assert.equal(smoothFalloff(0), 1);
  assert.equal(smoothFalloff(1), 0);
  assert.equal(smoothFalloff(NaN), 0);
});

test('fixture pools brighten the floor beneath a fixture', () => {
  const baked = bakeLevelLighting(levelWith([room(0, 0, 24, 8, 3.0)], [light(4, 4)]));
  const beneath = luminance(baked.sample(4, 0, 4));
  const near = luminance(baked.sample(6.5, 0, 4));
  const far = luminance(baked.sample(20, 0, 4));
  assert.ok(beneath > near, `${beneath} should beat ${near}`);
  assert.ok(near > far, `${near} should beat ${far}`);
  assert.ok(Math.abs(far - luminance(baked.rooms[0].baseline)) < 1e-6);
});

test('doorways blend between differently lit rooms instead of stepping', () => {
  const rooms = [room(0, 0, 10, 10, 3.0), room(10.4, 0, 30, 20, 3.0)];
  const lights = [
    light(2, 2), light(5, 2), light(8, 2), light(2, 8), light(5, 8), light(8, 8),
    light(30, 2)
  ];
  const wall = {
    id: 'w1', x: 10, z: 0, width: 0.4, depth: 10, height: 3,
    openings: [{ kind: 'door', offset: 4.5, width: 1, height: 2.1, sill: 0 }]
  };
  const open = bakeLevelLighting(levelWith(rooms, lights, [wall]));
  const closed = bakeLevelLighting(levelWith(rooms, lights, [Object.assign({}, wall, { openings: [] })]));
  assert.ok(
    luminance(open.rooms[0].baseline) > luminance(open.rooms[1].baseline) + 0.1,
    'test setup needs contrast'
  );

  const brightNearDoor = luminance(open.sampleInRoom(0, 9.9, 0, 5));
  const brightSolid = luminance(closed.sampleInRoom(0, 9.9, 0, 5));
  const dimNearDoor = luminance(open.sampleInRoom(1, 10.5, 0, 5));
  const dimSolid = luminance(closed.sampleInRoom(1, 10.5, 0, 5));
  assert.ok(brightNearDoor < brightSolid, 'the bright side loses light to the dim room');
  assert.ok(dimNearDoor > dimSolid, 'the dim side gains light from the bright room');

  // The exchange itself cancels across the threshold: the two sides of the
  // 0.4 m wall are the same distance from the opening. Local fixture pools are
  // deliberately not part of it — a fixture behind the wall no longer lights
  // the other side — so the exchange is measured on its own.
  const brightBlend = luminance(open.openingBlend(0, 9.95, 0, 5));
  const dimBlend = luminance(open.openingBlend(1, 10.45, 0, 5));
  assert.ok(brightBlend < -0.001 && dimBlend > 0.001, 'the doorway exchanges light');
  assert.ok(Math.abs(brightBlend + dimBlend) < 0.01, 'no hard seam at the threshold');

  // A fixture behind the wall contributes nothing on the far side: the raw
  // threshold step is the local pool difference, which is real geometry.
  const leaked = closed.sampleInRoom(1, 10.5, 0, 5);
  const throughDoor = open.sampleInRoom(1, 10.5, 0, 5);
  assert.ok(
    Math.abs(leaked[0] - open.rooms[1].baseline[0]) < 0.35,
    'a solid wall keeps the dim room near its own baseline'
  );
  assert.ok(throughDoor[0] >= leaked[0], 'the doorway only adds light on the dim side');

  // Bounded: far from the opening both rooms keep their baselines.
  assert.ok(
    Math.abs(luminance(open.sampleInRoom(1, 35, 0, 15)) - luminance(closed.sampleInRoom(1, 35, 0, 15))) < 1e-6
  );
});

test('opaque walls stop a fixture pool from crossing them', () => {
  // A red light on one side of a solid wall, a blue one on the other: the
  // editor preview must not show either colour contaminating the far room.
  const rooms = [room(0, 0, 10, 10, 3.0), room(10.4, 0, 10, 10, 3.0)];
  const lights = [
    Object.assign(light(5, 5), { color: [1, 0, 0] }),
    Object.assign(light(15.4, 5), { color: [0, 0, 1] })
  ];
  const solid = { id: 'w1', x: 10, z: 0, width: 0.4, depth: 10, height: 3, openings: [] };
  const closed = bakeLevelLighting(levelWith(rooms, lights, [solid]));
  const nearRedWall = closed.sampleInRoom(1, 10.5, 0, 5);
  const nearBlueWall = closed.sampleInRoom(0, 9.9, 0, 5);
  assert.ok(
    nearRedWall[2] > nearRedWall[0],
    `the blue room must stay blue next to the wall: ${nearRedWall}`
  );
  assert.ok(
    nearBlueWall[0] > nearBlueWall[2],
    `the red room must stay red next to the wall: ${nearBlueWall}`
  );

  // Opening the wall admits the neighbour's pool through the hole, at the hole.
  const open = bakeLevelLighting(levelWith(rooms, lights, [
    Object.assign({}, solid, {
      openings: [{ kind: 'door', offset: 4.0, width: 2.0, height: 2.1, sill: 0 }]
    })
  ]));
  const throughDoor = open.sampleInRoom(1, 10.5, 0, 5);
  assert.ok(
    throughDoor[0] > closed.sampleInRoom(1, 10.5, 0, 5)[0],
    'the doorway must admit the neighbouring fixture through the hole'
  );
});

test('a wall that straddles a room boundary does not shadow its own floor', () => {
  // The showcase levels author walls across the room edge, so the outermost
  // floor sample sits inside the wall. It must read as the room, not as a dark
  // strip along the wall base.
  const level = levelWith(
    [room(0, 0, 10, 10, 3.0)],
    [light(5, 5)],
    [{ id: 'w1', x: 0, z: 0, width: 10, depth: 0.4, height: 3 }]
  );
  const baked = bakeLevelLighting(level);
  const edge = luminance(baked.sampleInRoom(0, 5, 0, 0));
  const interior = luminance(baked.sampleInRoom(0, 5, 0, 2.5));
  assert.ok(
    edge > interior - 0.35,
    `the floor edge under the wall must not go dark: ${edge} vs ${interior}`
  );
});

test('the 3D preview mesh bakes room colour into its vertex colours', () => {
  // A small, well-lit room next to a large, dim one: the same fixture count
  // must produce visibly different pre-grey levels in the preview.
  const rooms = [room(0, 0, 6, 6, 3.0), room(24, 0, 60, 60, 3.0)];
  const level = levelWith(
    rooms,
    [light(2, 2), light(4, 4), light(54, 30)],
    [{ id: 'w1', x: 0, z: 6, width: 6, depth: 0.4, height: 3, openings: [] }]
  );
  const mesh = geometry.buildLevelMesh(level);
  const baked = bakeLevelLighting(level);
  assert.ok(
    luminance(baked.rooms[0].baseline) > luminance(baked.rooms[1].baseline) + 0.1,
    'test setup needs contrast'
  );

  const floor = mesh.batches.find((batch) => batch.name === 'floor');
  assert.ok(floor && floor.count > 0);
  const red = (vertex) => mesh.colors[vertex * 4];
  const small = red(floor.start);
  const large = red(floor.start + 6);
  assert.ok(small > large + 0.05, `a well-lit room must read brighter: ${small} vs ${large}`);

  // Every vertex stays inside the renderer's valid colour range.
  for (let i = 0; i < mesh.colors.length; i++) {
    assert.ok(Number.isFinite(mesh.colors[i]));
    assert.ok(mesh.colors[i] >= 0 && mesh.colors[i] <= 1);
  }

  // The wall batch is baked too (walls carry the per-vertex gradients).
  const wall = mesh.batches.find((batch) => batch.name === 'walls');
  assert.ok(wall && wall.count > 0);
  assert.ok(red(wall.start) <= geometry.COLOR.wall[0] + 1e-6);

  // The emissive fixture panels are left alone by the bake: their brightest
  // corner is the un-dimmed fixture glow (the authored colour at full output).
  const lights = mesh.batches.find((batch) => batch.name === 'lights');
  assert.ok(lights && lights.count > 0);
  let lightMax = 0;
  for (let v = lights.start; v < lights.start + lights.count; v++) {
    lightMax = Math.max(lightMax, red(v));
  }
  assert.equal(lightMax, TUNING.DEFAULT_LIGHT_COLOR[0]);

  // A coloured fixture shows that colour on its panel.
  const coloured = geometry.buildLevelMesh(levelWith(
    [room(0, 0, 6, 6, 3.0)],
    [light(3, 3, { color: [0, 0.2, 1] })],
    []
  ));
  const colouredLights = coloured.batches.find((batch) => batch.name === 'lights');
  let maxBlue = 0;
  for (let v = colouredLights.start; v < colouredLights.start + colouredLights.count; v++) {
    maxBlue = Math.max(maxBlue, coloured.colors[v * 4 + 2]);
  }
  assert.equal(maxBlue, 1);

  // Disabling the lighting option restores the flat preview colours.
  const flat = geometry.buildLevelMesh(level, { lighting: false });
  const flatFloor = flat.batches.find((batch) => batch.name === 'floor');
  assert.ok(Math.abs(flat.colors[flatFloor.start * 4] - geometry.COLOR.floor[0]) < 1e-6);
  assert.equal(baked.summary().rooms, 2);
});

test('the preview respects the model-level intensity limit', () => {
  assert.equal(TUNING.MAX_LIGHT_INTENSITY, model.LIGHT_INTENSITY_MAX);
  // Real editor levels keep their intensity when the preview bakes them.
  const parsed = new model.Level({
    format_version: 1,
    id: 'preview',
    name: 'Preview',
    spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 10, depth: 10, height: 3 }],
    ceiling_lights: [{ fixture: 'core:fluorescent_panel_01', x: 5, z: 5, brightness: 1.4 }]
  });
  const baked = bakeLevelLighting(parsed);
  assert.equal(baked.lights[0].intensity, 1.4);
  // The editor's CeilingLight model carries the authored colour through.
  const coloured = new model.Level({
    format_version: 1,
    id: 'preview_colour',
    name: 'Preview Colour',
    spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 10, depth: 10, height: 3 }],
    ceiling_lights: [{ fixture: 'core:fluorescent_panel_01', x: 5, z: 5, color: [0.1, 0.2, 0.9] }]
  });
  const bakedColoured = bakeLevelLighting(coloured);
  assert.deepEqual(bakedColoured.lights[0].color, [0.1, 0.2, 0.9]);
});

test('fixture orientation is one shared rule for the bake and the preview mesh', () => {
  const { fixtureIsTurned, fixtureHalfExtents } = lighting;
  const cases = [
    [0, false], [45, true], [89.6, true], [90, true], [135, true],
    [179.0, true], [179.6, false], [180, false], [180.4, false], [270, true],
    [360, false], [-90, true], [-0.6, true]
  ];
  for (const [rotation, turned] of cases) {
    assert.equal(fixtureIsTurned(rotation), turned, `rotation ${rotation}`);
    const [halfW, halfD] = fixtureHalfExtents(rotation);
    assert.equal(halfW, turned ? 0.3 : 0.6, `rotation ${rotation} width`);
    assert.equal(halfD, turned ? 0.6 : 0.3, `rotation ${rotation} depth`);
  }

  // The preview's drawn panel footprint must match the pool footprint for the
  // same rotation (this used to drift for 135-degree fixtures).
  for (const rotation of [0, 45, 90, 135, 180, 270, 315]) {
    const level = {
      rooms: [{ id: 'r', x: 0, z: 0, width: 10, depth: 10, height: 3 }],
      walls: [],
      props: [],
      ceiling_lights: [{ id: 'l', fixture: 'core:fluorescent_panel_01', x: 5, z: 5, rotation_degrees: rotation }],
      getCeilingHeight: () => 3
    };
    const mesh = geometry.buildLevelMesh(level, { lighting: false });
    const batch = mesh.batches.find((entry) => entry.name === 'lights');
    let minX = Infinity, maxX = -Infinity, minZ = Infinity, maxZ = -Infinity;
    for (let vertex = batch.start; vertex < batch.start + batch.count; vertex++) {
      const p = vertex * 3;
      minX = Math.min(minX, mesh.positions[p]);
      maxX = Math.max(maxX, mesh.positions[p]);
      minZ = Math.min(minZ, mesh.positions[p + 2]);
      maxZ = Math.max(maxZ, mesh.positions[p + 2]);
    }
    const [halfW, halfD] = fixtureHalfExtents(rotation);
    assert.ok(Math.abs((maxX - minX) - halfW * 2) < 1e-6, `rotation ${rotation} x extent`);
    assert.ok(Math.abs((maxZ - minZ) - halfD * 2) < 1e-6, `rotation ${rotation} z extent`);
  }
});
