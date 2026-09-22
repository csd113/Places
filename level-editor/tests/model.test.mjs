// model.test.mjs - Level data model, serialization round trips and validation.
import test from 'node:test';
import assert from 'node:assert/strict';
import model from '../js/model.js';
import geometry from '../js/geometry.js';

const { Level, Wall, WallOpening, Prop, Room, Decal } = model;

test('a wall round trip preserves openings', () => {
  const wall = new Wall({ x: 1, z: 2, width: 6, depth: 0.35 });
  wall.openings.push(new WallOpening({ kind: 'window', offset: 1.5, width: 1.2, height: 1.0, sill: 1.1 }));
  const json = JSON.parse(JSON.stringify(wall.toJSON()));
  assert.equal(json.openings.length, 1);
  assert.deepEqual(json.openings[0], { kind: 'window', offset: 1.5, width: 1.2, height: 1.0, sill: 1.1 });

  const restored = new Wall(json);
  assert.equal(restored.openings.length, 1);
  assert.equal(restored.openings[0].kind, 'window');
  assert.equal(restored.openings[0].sill, 1.1);
});

test('a doorway with sill 0 omits the sill field', () => {
  const wall = new Wall({ width: 4, depth: 0.35 });
  wall.openings.push(new WallOpening({ kind: 'door', offset: 1, width: 1, height: 2.1 }));
  const json = wall.toJSON();
  assert.equal('sill' in json.openings[0], false);
  assert.equal(json.openings[0].offset, 1);
});

test('cloning a wall keeps ids for history, duplicating renumbers them', () => {
  const wall = new Wall({ width: 4, depth: 0.35 });
  wall.openings.push(new WallOpening({ kind: 'door', offset: 1, width: 1, height: 2.1 }));
  const copy = wall.clone();
  copy.openings[0].offset = 3;
  assert.equal(wall.openings[0].offset, 1, 'the copy is independent');
  assert.equal(copy.id, wall.id, 'clones keep their id so undo/redo leaves selection stable');
  assert.equal(copy.openings[0].id, wall.openings[0].id);

  const duplicate = wall.duplicate();
  assert.notEqual(duplicate.id, wall.id);
  assert.notEqual(duplicate.openings[0].id, wall.openings[0].id);
  assert.equal(duplicate.openings[0].offset, 1, 'duplicates copy the data');
});

test('props serialize only non-default fields and round trip', () => {
  const prop = new Prop({ model: 'core:fridge', x: 3, z: -2, rotation_degrees: 90, y: -0.2, solid: true });
  const json = prop.toJSON();
  assert.deepEqual(json, { model: 'core:fridge', x: 3, z: -2, rotation_degrees: 90, y: -0.2, solid: true });

  const plain = new Prop({ model: 'core:crate', x: 0, z: 0 });
  assert.deepEqual(plain.toJSON(), { model: 'core:crate', x: 0, z: 0, rotation_degrees: 0 });

  const restored = new Prop({ ...json, id: 'p2' });
  assert.equal(restored.y, -0.2);
  assert.equal(restored.solid, true);
});

test('a level round trips rooms, openings, props, lights and textures', () => {
  const level = new Level({
    id: 'round_trip',
    name: 'Round Trip',
    spawn: { x: 1, z: 2, yaw_degrees: 45 },
    rooms: [{ x: -5, z: -5, width: 10, depth: 10, height: 3.5 }, { x: 5, z: 0, width: 6, depth: 6, height: 3.0 }],
    walls: [{ x: -5, z: -5, width: 10, depth: 0.35, openings: [{ kind: 'door', offset: 4, width: 1.2, height: 2.1 }] }],
    ceiling_lights: [{ fixture: 'core:fluorescent_panel_01', x: 0, z: 0 }],
    props: [{ model: 'core:couch', x: 1, z: 1, rotation_degrees: 180 }],
    decals: [
      { x: 0, y: 1.5, z: -4.9, width: 1.6, height: 0.6, material: 'core:decal_test_01', surface: 'wall_south' },
      { x: 2, y: 0, z: 2, width: 1, height: 1, material: 'core:decal_arrow_01', surface: 'floor', rotation_degrees: 90 }
    ],
    custom_textures: { 'pack:tile': { filename: 'textures/tile.png', width: 32, height: 32, dataUrl: 'data:,' } }
  });
  const json = JSON.parse(JSON.stringify(level.toJSON()));
  const restored = new Level(json);

  assert.equal(restored.rooms.length, 2);
  assert.equal(restored.walls[0].openings.length, 1);
  assert.equal(restored.props.length, 1);
  assert.equal(restored.props[0].model, 'core:couch');
  assert.equal(restored.spawn.yaw_degrees, 45);
  assert.equal(restored.decals.length, 2);
  assert.equal(restored.decals[0].surface, 'wall_south');
  assert.equal(restored.decals[1].rotation_degrees, 90);
  // Custom texture metadata is not part of level.json (the ZIP pack carries the
  // texture files and materials.json), but cloning keeps it for the session.
  assert.ok(level.clone().custom_textures['pack:tile']);
  assert.deepEqual(restored.toJSON(), json);
});

test('a single room is exported as `room`, several as `rooms`', () => {
  const one = new Level({ id: 'a', name: 'A', rooms: [{ x: 0, z: 0, width: 4, depth: 4, height: 3.5 }] });
  assert.ok(one.toJSON().room);
  assert.equal(one.toJSON().rooms, undefined);

  const two = new Level({
    id: 'b', name: 'B',
    rooms: [{ x: 0, z: 0, width: 4, depth: 4, height: 3.5 }, { x: 4, z: 0, width: 4, depth: 4, height: 3.5 }]
  });
  assert.equal(two.toJSON().rooms.length, 2);
  assert.equal(two.toJSON().room, undefined);
});

test('cloning a level is independent of the original', () => {
  const level = new Level({ id: 'c', name: 'C', props: [{ model: 'core:chair', x: 0, z: 0 }] });
  const copy = level.clone();
  copy.props[0].x = 99;
  copy.walls.push(new Wall({ width: 2, depth: 0.3 }));
  assert.equal(level.props[0].x, 0);
  assert.equal(level.walls.length, 0);
});

test('validation accepts a valid level with doors, windows and a sunk prop', () => {
  const level = new Level({
    id: 'valid_level',
    name: 'Valid Level',
    spawn: { x: 0, z: 0 },
    rooms: [{ x: -5, z: -5, width: 10, depth: 10, height: 3.5 }],
    walls: [
      { x: -5, z: -5, width: 10, depth: 0.35, openings: [{ kind: 'door', offset: 4, width: 1.2, height: 2.1 }] },
      { x: 5, z: -5, width: 0.35, depth: 10, openings: [{ kind: 'window', offset: 3, width: 1.5, height: 1.2, sill: 1.0 }] }
    ],
    props: [
      { model: 'core:chair', x: 1, z: 1, y: -0.2 },
      { model: 'core:stove', x: -4.9, z: 0 }
    ]
  });
  const result = model.validateLevel(level);
  assert.deepEqual(result.errors, []);
});

test('validation rejects a door that extends beyond its wall in user terms', () => {
  const level = new Level({
    id: 'bad_door',
    name: 'Bad Door',
    spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    walls: [{ x: 0, z: 0, width: 4, depth: 0.35, openings: [{ kind: 'door', offset: 3.5, width: 1.2, height: 2.1 }] }]
  });
  const result = model.validateLevel(level);
  assert.equal(result.valid, false);
  assert.ok(result.errors[0].startsWith('Door opening extends beyond this wall'), result.errors[0]);
  assert.match(result.errors[0], /wall 0/);

  const window = new Level({
    id: 'bad_window', name: 'Bad Window', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    walls: [{ x: 0, z: 0, width: 0.35, depth: 3, openings: [{ kind: 'window', offset: 2.5, width: 1.0, height: 1.0, sill: 1.0 }] }]
  });
  assert.match(model.validateLevel(window).errors[0], /^Window opening extends beyond this wall/);
});

test('validation rejects malformed openings and props only', () => {
  const level = new Level({
    id: 'bad_data', name: 'Bad Data', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    walls: [{ x: 0, z: 0, width: 4, depth: 0.35, openings: [{ kind: 'window', offset: 1, width: 1, height: 1, sill: -1 }] }],
    props: [{ model: '', x: 0, z: 0 }]
  });
  const result = model.validateLevel(level);
  assert.equal(result.valid, false);
  assert.ok(result.errors.some(e => e.includes('negative sill height')));
  assert.ok(result.errors.some(e => e.includes('non-empty model id')));
});

test('decals serialize their surface and rotation and reject malformed data', () => {
  const build = (decals) => new Level({
    id: 'decals', name: 'Decals', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    decals
  });

  const valid = model.validateLevel(build([
    { x: 3, y: 1.5, z: 0, width: 1.2, height: 0.5, material: 'core:decal_test_01', surface: 'wall_south' },
    { x: 3, y: 0, z: 3, width: 1, height: 1, material: 'core:decal_arrow_01', surface: 'floor', rotation_degrees: 45 }
  ]));
  assert.deepEqual(valid.errors, []);

  const bad = model.validateLevel(build([
    { x: 3, y: 1.5, z: 0, width: -1, height: 0.5, material: 'core:decal_test_01', surface: 'wall_south' },
    { x: 3, y: 1.5, z: 0, width: 11, height: 0.5, material: 'core:decal_test_01', surface: 'floor' },
    { x: 3, y: 1.5, z: 0, width: 1, height: 0.5, material: '  ', surface: 'floor' },
    { x: 3, y: 1.5, z: 0, width: 1, height: 0.5, material: 'core:decal_test_01', surface: 'wall_up' }
  ]));
  assert.equal(bad.valid, false);
  assert.ok(bad.errors.some(e => e.includes('must be positive')));
  assert.ok(bad.errors.some(e => e.includes('larger than the 10 m limit')));
  assert.ok(bad.errors.some(e => e.includes('non-empty material id')));
  assert.ok(bad.errors.some(e => e.includes('is not one of floor')));

  // A decal round trips with its rotation and sheet untouched.
  const decal = new Decal({ x: 2, y: 1, z: 3, width: 1.5, height: 0.4, rotation_degrees: 90, material: 'core:decal_stripes_01', surface: 'wall_east' });
  const restored = new Decal(JSON.parse(JSON.stringify(decal.toJSON())));
  assert.equal(restored.surface, 'wall_east');
  assert.equal(restored.rotation_degrees, 90);
  assert.notEqual(decal.duplicate().id, decal.id);
});

test('ceiling light intensity is optional, aliased and validated', () => {
  // Omitted means the standard 1.0 fixture, and the JSON stays terse.
  const standard = new model.CeilingLight({ fixture: 'core:fluorescent_panel_01', x: 1, z: 2 });
  assert.equal(standard.brightness, 1.0);
  assert.equal('brightness' in standard.toJSON(), false);

  // `intensity` is accepted on import as an alias for `brightness`.
  const aliased = new model.CeilingLight({ fixture: 'core:fluorescent_panel_01', x: 1, z: 2, intensity: 1.4 });
  assert.equal(aliased.brightness, 1.4);
  assert.equal(aliased.toJSON().brightness, 1.4);

  // A non-default intensity survives a round trip.
  const strong = new model.CeilingLight({ fixture: 'core:fluorescent_panel_01', x: 1, z: 2, brightness: 0.8 });
  assert.equal(new model.CeilingLight(JSON.parse(JSON.stringify(strong.toJSON()))).brightness, 0.8);

  const build = (lights) => new Level({
    id: 'lights', name: 'Lights', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    ceiling_lights: lights
  });
  assert.deepEqual(model.validateLevel(build([
    { fixture: 'core:fluorescent_panel_01', x: 1, z: 1 },
    { fixture: 'core:fluorescent_panel_01', x: 4, z: 4, brightness: 2.0 }
  ])).errors, []);

  const negative = model.validateLevel(build([
    { fixture: 'core:fluorescent_panel_01', x: 1, z: 1, brightness: -1 }
  ]));
  assert.equal(negative.valid, false);
  assert.ok(negative.errors.some(e => e.includes('intensity cannot be negative')), negative.errors.join());

  const malformed = model.validateLevel(build([
    { fixture: 'core:fluorescent_panel_01', x: Number.NaN, z: 0 },
    { fixture: 'core:fluorescent_panel_01', x: 0, z: 0, rotation_degrees: Number.POSITIVE_INFINITY }
  ]));
  assert.equal(malformed.valid, false);
  assert.equal(malformed.errors.length, 2, malformed.errors.join());
  assert.ok(malformed.errors.every(e => e.includes('finite numbers')), malformed.errors.join());

  // Above the game's clamp the level still loads, so the editor warns only.
  const high = model.validateLevel(build([
    { fixture: 'core:fluorescent_panel_01', x: 1, z: 1, brightness: 20 }
  ]));
  assert.equal(high.valid, true);
  assert.ok(high.warnings.some(w => w.includes('clamped')), high.warnings.join());
});

test('ceiling light colour is optional, round-trips and is validated', () => {
  // Omitted colour stays omitted on save: legacy levels keep their shape.
  const legacy = new model.CeilingLight({ fixture: 'core:fluorescent_panel_01', x: 1, z: 2 });
  assert.equal(legacy.color, null);
  assert.equal('color' in legacy.toJSON(), false);

  // An authored colour survives a clone and a JSON round trip.
  const blue = new model.CeilingLight({
    fixture: 'core:fluorescent_panel_01', x: 1, z: 2, color: [0.1, 0.2, 0.9]
  });
  assert.deepEqual(blue.toJSON().color, [0.1, 0.2, 0.9]);
  assert.deepEqual(blue.clone().color, [0.1, 0.2, 0.9]);
  const restored = new model.CeilingLight(JSON.parse(JSON.stringify(blue.toJSON())));
  assert.deepEqual(restored.color, [0.1, 0.2, 0.9]);
  assert.deepEqual(blue.duplicate().color, [0.1, 0.2, 0.9]);

  const build = (lights) => new Level({
    id: 'colours', name: 'Colours', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    ceiling_lights: lights
  });
  // Boundary channels (0 and 1) are legal.
  assert.deepEqual(model.validateLevel(build([
    { fixture: 'core:fluorescent_panel_01', x: 1, z: 1, color: [0, 1, 0] }
  ])).errors, []);
  // Out-of-range colours are reported like a negative intensity.
  const bad = model.validateLevel(build([
    { fixture: 'core:fluorescent_panel_01', x: 1, z: 1, color: [1.5, 0, 0] },
    { fixture: 'core:fluorescent_panel_01', x: 2, z: 2, color: [-0.1, 0, 0] }
  ]));
  assert.equal(bad.valid, false);
  assert.equal(bad.errors.filter(e => e.includes('colour')).length, 2, bad.errors.join());
  // A malformed array is normalised back to "no colour" (the game default)
  // instead of being written out as invalid JSON.
  const malformed = new model.CeilingLight({
    fixture: 'core:fluorescent_panel_01', x: 1, z: 2, color: [0, 0]
  });
  assert.equal(malformed.color, null);
  assert.equal('color' in malformed.toJSON(), false);
});

test('unknown opening kinds stay forward compatible', () => {
  const level = new Level({
    id: 'future', name: 'Future', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    walls: [{ x: 0, z: 0, width: 4, depth: 0.35, openings: [{ kind: 'vent_v2', offset: 1, width: 0.5, height: 0.5, sill: 2.0 }] }]
  });
  assert.deepEqual(model.validateLevel(level).errors, []);
});

test('validation tolerates intentional overlap and clipping', () => {
  const level = new Level({
    id: 'clipping', name: 'Clipping', spawn: { x: 0, z: 0 },
    rooms: [
      { x: 0, z: 0, width: 6, depth: 6, height: 3.5 },
      { x: 3, z: 3, width: 6, depth: 6, height: 3.5 }
    ],
    walls: [
      { x: 0, z: 0, width: 4, depth: 0.35 },
      { x: 1, z: 0, width: 4, depth: 0.35 }
    ],
    props: [
      { model: 'core:couch', x: 0, z: 0 },
      { model: 'core:table', x: 0, y: -0.5, z: 0 }
    ]
  });
  assert.deepEqual(model.validateLevel(level).errors, []);
});

test('validation measures wall length the same way geometry does', () => {
  const build = (width, depth, offset, openingWidth) => new Level({
    id: 'axis_case', name: 'Axis Case', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 20, depth: 20, height: 3.5 }],
    walls: [{ x: 0, z: 0, width, depth, openings: [{ kind: 'door', offset, width: openingWidth, height: 2.1 }] }]
  });

  // A wall whose length runs along Z (thin in X) accepts an opening near its far end...
  assert.deepEqual(model.validateLevel(build(0.35, 8, 6.5, 1.2)).errors, []);
  // ...and rejects one past that end.
  assert.equal(model.validateLevel(build(0.35, 8, 7.5, 1.2)).errors.length, 1);

  for (const [w, d] of [[4, 0.35], [0.35, 4], [2, 2]]) {
    const wall = new Wall({ width: w, depth: d });
    assert.equal(geometry.wallLength(wall), Math.max(Math.abs(w), Math.abs(d)));
  }
});
