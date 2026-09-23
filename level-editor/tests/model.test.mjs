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

test('ceiling light enabled round-trips and is validated', () => {
  // Omitted means enabled, and a legacy light keeps its terse shape on save.
  const standard = new model.CeilingLight({ fixture: 'core:fluorescent_panel_01', x: 1, z: 2 });
  assert.equal(standard.enabled, true);
  assert.equal('enabled' in standard.toJSON(), false);

  // A disabled fixture survives clone, duplicate and a JSON round trip.
  const disabled = new model.CeilingLight({ fixture: 'core:fluorescent_panel_01', x: 1, z: 2, enabled: false });
  assert.equal(disabled.enabled, false);
  assert.equal(disabled.toJSON().enabled, false);
  assert.equal(disabled.clone().toJSON().enabled, false);
  assert.equal(disabled.duplicate().toJSON().enabled, false);
  const restored = new model.CeilingLight(JSON.parse(JSON.stringify(disabled.toJSON())));
  assert.equal(restored.enabled, false);
  assert.equal(restored.toJSON().enabled, false);

  // An explicit `true` is authored data too, so it is written back unchanged.
  const explicit = new model.CeilingLight({ fixture: 'core:fluorescent_panel_01', x: 1, z: 2, enabled: true });
  assert.equal(explicit.toJSON().enabled, true);

  // A whole level round trip keeps the switch.
  const level = new Level({
    id: 'enabled', name: 'Enabled', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    ceiling_lights: [
      { fixture: 'core:fluorescent_panel_01', x: 1, z: 1, enabled: false },
      { fixture: 'core:fluorescent_panel_01', x: 4, z: 4, enabled: true }
    ]
  });
  const json = JSON.parse(JSON.stringify(level.toJSON()));
  assert.equal(json.ceiling_lights[0].enabled, false);
  assert.equal(json.ceiling_lights[1].enabled, true);
  assert.deepEqual(new Level(json).toJSON().ceiling_lights, json.ceiling_lights);

  // Non-boolean values are malformed, exactly like the loader.
  const build = (enabled) => new Level({
    id: 'bad_enabled', name: 'Bad Enabled', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    ceiling_lights: [{ fixture: 'core:fluorescent_panel_01', x: 1, z: 1, enabled }]
  });
  const malformed = model.validateLevel(build('off'));
  assert.equal(malformed.valid, false);
  assert.ok(malformed.errors.some(e => e.includes('enabled must be a boolean')), malformed.errors.join());
  assert.deepEqual(model.validateLevel(build(false)).errors, []);
});

test('prop light sources round-trip unchanged', () => {
  const lights = [
    {
      shape: 'rect', half_width: 0.3, half_depth: 0.05, offset: [0, 0.9, 0.25],
      rotation_degrees: 0, color: [0.53, 0.73, 1.0], intensity: 0.4,
      range: 3.0, falloff: 'smooth', enabled: true
    },
    { shape: 'point', brightness: 0.5 },
    { shape: 'line', length: 1.2, falloff: 'linear', enabled: false },
    { color: [1.0, 0.9, 0.8] } // no shape and no dimensions: the default point
  ];
  const prop = new Prop({ model: 'core:desk', x: 1, z: 2, lights });
  assert.deepEqual(prop.toJSON().lights, lights);
  assert.deepEqual(prop.clone().toJSON().lights, lights);
  assert.deepEqual(prop.duplicate().toJSON().lights, lights);
  assert.deepEqual(new Prop(JSON.parse(JSON.stringify(prop.toJSON()))).toJSON().lights, lights);

  const level = new Level({
    id: 'prop_lights', name: 'Prop Lights', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    props: [{ model: 'core:desk', x: 1, z: 2, lights }]
  });
  const json = JSON.parse(JSON.stringify(level.toJSON()));
  assert.deepEqual(json.props[0].lights, lights);
  assert.deepEqual(new Level(json).toJSON().props[0].lights, lights);
});

test('malformed prop lights are rejected and over-bright ones only warn', () => {
  const build = (lights, enabled) => new Level({
    id: 'bad_lights', name: 'Bad Lights', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    ceiling_lights: enabled === undefined
      ? []
      : [{ fixture: 'core:fluorescent_panel_01', x: 1, z: 1, enabled }],
    props: [{ model: 'core:desk', x: 1, z: 2, lights }]
  });
  const errorsFor = (lights, enabled) => model.validateLevel(build(lights, enabled)).errors.join('\n');

  assert.match(errorsFor([{ shape: 'sphere' }]), /shape must be one of point, rect, line/);
  assert.match(errorsFor([{ shape: 'rect', half_width: 0.3 }]), /rect lights need a half_depth/);
  assert.match(errorsFor([{ shape: 'rect', half_width: 0.0, half_depth: 0.05 }]), /half_width must be a finite number > 0/);
  assert.match(errorsFor([{ shape: 'line' }]), /line lights need a length/);
  assert.match(errorsFor([{ shape: 'line', length: -1 }]), /length must be a finite number > 0/);
  assert.match(errorsFor([{ shape: 'point', falloff: 'quadratic' }]), /falloff/);
  assert.match(errorsFor([{ shape: 'point', offset: [0, 1] }]), /offset must be exactly three finite numbers/);
  assert.match(errorsFor([{ shape: 'point', offset: [0, Number.POSITIVE_INFINITY, 0] }]), /offset must be exactly three finite numbers/);
  assert.match(errorsFor([{ shape: 'point', color: [2, 0, 0] }]), /colour/);
  assert.match(errorsFor([{ shape: 'point', color: [0, 1] }]), /colour/);
  assert.match(errorsFor([{ shape: 'point', intensity: -1 }]), /cannot be negative/);
  assert.match(errorsFor([{ shape: 'point', brightness: 'bright' }]), /finite number/);
  assert.match(errorsFor([{ shape: 'point', offset: [0, 'up', 0] }]), /offset must be exactly three finite numbers/);
  assert.match(errorsFor([{ shape: 'point', range: 0 }]), /range must be a finite number > 0/);
  assert.match(errorsFor([{ shape: 'point', enabled: 'on' }]), /enabled must be a boolean/);
  assert.match(errorsFor(['bright']), /must be an object/);
  assert.match(errorsFor({ shape: 'point' }), /lights must be an array/);
  assert.match(errorsFor([{ shape: 'point' }], 1), /enabled must be a boolean/);

  // The documented minimum and everything in range is accepted.
  assert.deepEqual(model.validateLevel(build([
    { shape: 'point' },
    { shape: 'rect', half_width: 0.3, half_depth: 0.05 },
    { shape: 'line', length: 1.2 },
    { color: [0, 1, 0], brightness: 0.5, range: 3.0, falloff: 'constant', enabled: false }
  ])).errors, []);

  // Above the game clamp the level still loads, so the editor warns only.
  const high = model.validateLevel(build([{ shape: 'point', intensity: 20 }]));
  assert.equal(high.valid, true);
  assert.ok(high.warnings.some(w => w.includes('clamped')), high.warnings.join());
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

test('fixture pool and emission fields round-trip unchanged', () => {
  const fields = { range: 4.0, falloff: 'linear', emission: 1.5, enabled: false };
  const lit = new model.CeilingLight({
    fixture: 'core:fluorescent_panel_01', x: 1, z: 2, brightness: 0.2, ...fields
  });
  const emitted = lit.toJSON();
  assert.equal(emitted.range, 4.0);
  assert.equal(emitted.falloff, 'linear');
  assert.equal(emitted.emission, 1.5);
  assert.equal(emitted.enabled, false);
  assert.deepEqual(lit.clone().toJSON(), emitted);
  const duplicated = lit.duplicate().toJSON();
  delete duplicated.id;
  const original = { ...emitted };
  delete original.id;
  assert.deepEqual(duplicated, original);

  // A fixture that authors none of them keeps its terse shape.
  const plain = new model.CeilingLight({ fixture: 'core:fluorescent_panel_01', x: 1, z: 2 });
  const terse = plain.toJSON();
  for (const key of Object.keys(fields)) {
    assert.equal(key in terse, false, `${key} must stay omitted when unauthored`);
  }

  // A whole level round trip keeps them, and validation mirrors the engine.
  const level = new Level({
    id: 'emission', name: 'Emission', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    ceiling_lights: [
      { fixture: 'core:fluorescent_panel_01', x: 1, z: 1, brightness: 0.18, emission: 1.0 },
      { fixture: 'core:fluorescent_panel_01', x: 4, z: 4, range: 3.0, falloff: 'constant' }
    ]
  });
  const json = JSON.parse(JSON.stringify(level.toJSON()));
  assert.equal(json.ceiling_lights[0].emission, 1.0);
  assert.equal(json.ceiling_lights[1].falloff, 'constant');
  assert.deepEqual(new Level(json).toJSON().ceiling_lights, json.ceiling_lights);

  const bad = (fields) => model.validateLevel(new Level({
    id: 'bad_emission', name: 'Bad Emission', spawn: { x: 0, z: 0 },
    rooms: [{ x: 0, z: 0, width: 6, depth: 6, height: 3.5 }],
    ceiling_lights: [{ fixture: 'core:fluorescent_panel_01', x: 1, z: 1, ...fields }]
  }));
  assert.ok(bad({ range: 0 }).errors.some(e => e.includes('range must be a positive')));
  assert.ok(bad({ falloff: 'quadratic' }).errors.some(e => e.includes('falloff must be one of')));
  assert.ok(bad({ emission: -1 }).errors.some(e => e.includes('emission must be a finite number')));
  assert.deepEqual(bad({ emission: 2.0 }).errors, []);
});
