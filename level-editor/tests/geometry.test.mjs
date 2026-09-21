// geometry.test.mjs - Wall opening geometry, collision boxes, mesh spec and picking.
// These mirror the assertions in src/level.rs and src/render.rs so the editor and the
// game agree on what a level looks like.
import test from 'node:test';
import assert from 'node:assert/strict';
import geometry from '../js/geometry.js';

function wall(overrides) {
  return Object.assign({ x: 0, y: 0, z: 0, width: 4, depth: 0.35, height: null, openings: [] }, overrides);
}

test('wall axis follows the longer dimension', () => {
  assert.equal(geometry.wallAxis(wall({})), 'x');
  assert.equal(geometry.wallAxis(wall({ width: 0.35, depth: 4 })), 'z');
  assert.equal(geometry.wallLength(wall({})), 4);
  assert.equal(geometry.wallThickness(wall({})), 0.35);
  assert.equal(geometry.wallLength(wall({ width: 0.35, depth: 4 })), 4);
  assert.equal(geometry.wallThickness(wall({ width: 0.35, depth: 4 })), 0.35);
});

test('wall without openings is a single full-height slice', () => {
  const slices = geometry.wallSolidSlices(wall({}), 3.5);
  assert.equal(slices.length, 1);
  assert.deepEqual(slices[0], { start: 0, end: 4, bottom: 0, top: 3.5 });
});

test('doorway splits a wall into left, header and right slices', () => {
  const w = wall({ openings: [{ kind: 'door', offset: 1.5, width: 1.0, height: 2.1 }] });
  const slices = geometry.wallSolidSlices(w, 3.5);
  assert.equal(slices.length, 3);
  assert.deepEqual(slices[0], { start: 0, end: 1.5, bottom: 0, top: 3.5 });
  assert.deepEqual(slices[1], { start: 1.5, end: 2.5, bottom: 2.1, top: 3.5 });
  assert.deepEqual(slices[2], { start: 2.5, end: 4, bottom: 0, top: 3.5 });
});

test('window keeps a sill and a header', () => {
  const w = wall({ openings: [{ kind: 'window', offset: 1.0, width: 1.5, height: 1.2, sill: 1.0 }] });
  const slices = geometry.wallSolidSlices(w, 3.5);
  assert.equal(slices.length, 4);
  assert.deepEqual(slices[1], { start: 1.0, end: 2.5, bottom: 0, top: 1.0 });
  assert.deepEqual(slices[2], { start: 1.0, end: 2.5, bottom: 2.2, top: 3.5 });
});

test('multiple openings decompose without losing wall material', () => {
  const w = wall({
    openings: [
      { kind: 'door', offset: 0.5, width: 1.0, height: 2.1 },
      { kind: 'window', offset: 2.5, width: 1.0, height: 1.0, sill: 1.2 }
    ]
  });
  const slices = geometry.wallSolidSlices(w, 3.5);
  // Horizontal coverage is complete: every metre of the wall length has a slice.
  const spans = Array.from(new Set(slices.flatMap(s => [s.start, s.end]))).sort((a, b) => a - b);
  let covered = 0;
  for (let i = 0; i < spans.length - 1; i++) covered += spans[i + 1] - spans[i];
  assert.equal(covered, 4);
  // Every slice stays inside the wall.
  for (const s of slices) {
    assert.ok(s.start >= 0 && s.end <= 4 && s.top <= 3.5 && s.bottom >= 0);
    assert.ok(s.end > s.start && s.top > s.bottom);
  }
  // The doorway leaves no material at floor level, the window leaves a sill.
  const floorLevel = slices.filter(s => s.bottom === 0);
  assert.equal(floorLevel.some(s => s.start === 0.5 && s.end === 1.5), false, 'doorway is open at floor level');
  assert.ok(floorLevel.some(s => s.start === 2.5 && s.end === 3.5), 'window keeps its sill');
});

test('full-height opening leaves a gap with no slices', () => {
  const w = wall({ openings: [{ kind: 'passage', offset: 1.0, width: 2.0, height: 3.5 }] });
  const slices = geometry.wallSolidSlices(w, 3.5);
  assert.equal(slices.length, 2);
  assert.deepEqual(slices[0], { start: 0, end: 1, bottom: 0, top: 3.5 });
  assert.deepEqual(slices[1], { start: 3, end: 4, bottom: 0, top: 3.5 });
});

test('an opening flush with a wall end keeps only the header above it', () => {
  const w = wall({ openings: [{ kind: 'door', offset: 0, width: 1.2, height: 2.1 }] });
  const slices = geometry.wallSolidSlices(w, 3.5);
  assert.equal(slices.length, 2);
  assert.deepEqual(slices[0], { start: 0, end: 1.2, bottom: 2.1, top: 3.5 });
  assert.deepEqual(slices[1], { start: 1.2, end: 4, bottom: 0, top: 3.5 });
});

test('malformed openings are ignored or clamped instead of throwing', () => {
  const w = wall({
    openings: [
      { kind: 'door', offset: 99, width: 1, height: 2 },
      { kind: 'door', offset: -5, width: 0.001, height: 2 },
      { kind: 'window', offset: 1, width: 1, height: 1, sill: 99 },
      { kind: 'door', offset: Number.NaN, width: Number.NaN, height: Number.NaN }
    ]
  });
  const slices = geometry.wallSolidSlices(w, 3.5);
  assert.ok(slices.length >= 1);
  for (const s of slices) assert.ok(Number.isFinite(s.start) && Number.isFinite(s.top));
});

test('collision boxes leave a doorway walkable and a window solid', () => {
  const door = wall({ x: 0, z: -0.35, width: 4, depth: 0.35, openings: [{ kind: 'door', offset: 1.5, width: 1, height: 2.1 }] });
  const boxes = geometry.wallCollisionBoxes(door, 3.5);
  // Left wall, header above the doorway, right wall: three boxes, none of them
  // blocking the player at doorway height (the game ignores boxes above 1.8 m).
  assert.equal(boxes.length, 3);
  const blocksPlayer = (x) => boxes.some(b => x >= b.minX && x <= b.maxX && b.maxY > 0 && b.minY < 1.8);
  assert.equal(blocksPlayer(2.0), false, 'the doorway stays walkable');
  assert.equal(blocksPlayer(0.5), true);
  assert.equal(blocksPlayer(3.5), true);

  const windowWall = wall({ openings: [{ kind: 'window', offset: 1, width: 1.4, height: 1.2, sill: 1.0 }] });
  const windowBoxes = geometry.wallCollisionBoxes(windowWall, 3.5);
  const windowBlocksPlayer = (x) => windowBoxes.some(b => x >= b.minX && x <= b.maxX && b.maxY > 0 && b.minY < 1.8);
  assert.equal(windowBlocksPlayer(1.6), true, 'a window sill must still block the player');
});

test('level mesh contains the expected batches and owners', () => {
  const level = {
    rooms: [{ id: 'room_1', x: -5, z: -5, width: 10, depth: 10, height: 3.5 }],
    walls: [wall({ id: 'wall_1', x: -5, z: -5, width: 10, depth: 0.35, openings: [{ kind: 'door', offset: 4, width: 1.4, height: 2.1 }] })],
    ceiling_lights: [{ id: 'light_1', x: 0, z: 0, rotation_degrees: 0 }],
    props: [{ id: 'prop_1', model: 'core:crate', x: 2, z: 2, rotation_degrees: 0 }],
    getCeilingHeight: () => 3.5
  };
  const mesh = geometry.buildLevelMesh(level, {});
  const names = mesh.batches.map(b => b.name);
  assert.deepEqual(names, ['floor', 'ceiling', 'walls', 'lights', 'props']);
  assert.ok(mesh.vertexCount > 0);
  assert.equal(mesh.positions.length, mesh.vertexCount * 3);
  assert.equal(mesh.colors.length, mesh.vertexCount * 4);
  assert.equal(mesh.owners.length, mesh.vertexCount);
  for (let i = 0; i < mesh.positions.length; i++) assert.ok(Number.isFinite(mesh.positions[i]));
  // Six quads per prop (36 vertices).
  const propBatch = mesh.batches.find(b => b.name === 'props');
  assert.equal(propBatch.count, 36);
  // Owners identify the source object for click-to-select.
  assert.ok(mesh.owners.includes('wall_1'));
  assert.ok(mesh.owners.includes('prop_1'));
  assert.ok(mesh.owners.includes('room_1'));
});

test('a doorway removes wall faces compared with a solid wall', () => {
  const solid = { id: 'w', x: 0, z: 0, width: 4, depth: 0.35, height: null, openings: [] };
  const withDoor = { ...solid, openings: [{ kind: 'door', offset: 1.5, width: 1, height: 2.1 }] };
  const levelA = { rooms: [], walls: [solid], ceiling_lights: [], props: [], getCeilingHeight: () => 3.5 };
  const levelB = { rooms: [], walls: [withDoor], ceiling_lights: [], props: [], getCeilingHeight: () => 3.5 };
  const batchA = geometry.buildLevelMesh(levelA, {}).batches.find(b => b.name === 'walls').count;
  const batchB = geometry.buildLevelMesh(levelB, {}).batches.find(b => b.name === 'walls').count;
  assert.notEqual(batchA, batchB);
});

test('mesh geometry follows a rotated prop', () => {
  const level = {
    rooms: [], walls: [], ceiling_lights: [],
    props: [{ id: 'p', model: 'core:couch', x: 0, z: 0, rotation_degrees: 90 }],
    getCeilingHeight: () => 3.5
  };
  const mesh = geometry.buildLevelMesh(level, { catalog: { get: () => ({ size: [2, 0.9, 0.9], color: [1, 1, 1] }) } });
  const batch = mesh.batches.find(b => b.name === 'props');
  assert.equal(batch.count, 36);
  let maxX = 0;
  let maxZ = 0;
  for (let i = 0; i < mesh.positions.length; i += 3) {
    maxX = Math.max(maxX, Math.abs(mesh.positions[i]));
    maxZ = Math.max(maxZ, Math.abs(mesh.positions[i + 2]));
  }
  // Rotated 90°: the 2 m length now runs along Z.
  assert.ok(maxX < 0.6, `expected the 0.9 m depth on X, got ${maxX}`);
  assert.ok(maxZ > 0.9, 'expected the 2 m length on Z');
});

test('picking sees through a doorway and selects the nearest object', () => {
  const level = {
    rooms: [],
    walls: [wall({ id: 'w', x: -2, y: 0, z: -0.35, width: 4, depth: 0.35, openings: [{ kind: 'door', offset: 1.5, width: 1, height: 2.1 }] })],
    ceiling_lights: [],
    props: [],
    getCeilingHeight: () => 3.5
  };
  const throughDoor = geometry.pickObject(level, { origin: [0, 1.0, -3], direction: [0, 0, 1] }, { skipRooms: true });
  assert.equal(throughDoor, null, 'a ray through the doorway should not hit the wall');
  const throughWall = geometry.pickObject(level, { origin: [-1.5, 1.0, -3], direction: [0, 0, 1] }, { skipRooms: true });
  assert.ok(throughWall);
  assert.equal(throughWall.id, 'w');
});

test('picking respects prop rotation', () => {
  const level = {
    rooms: [], walls: [], ceiling_lights: [],
    props: [{ id: 'p', model: 'core:table', x: 0, y: 0, z: 0, rotation_degrees: 90, size: [2, 0.75, 0.4] }],
    getCeilingHeight: () => 3.5
  };
  const hitLongAxis = geometry.pickObject(level, { origin: [0, 0.4, 3], direction: [0, 0, -1] }, {});
  assert.ok(hitLongAxis, 'the rotated length should be hit along Z');
  const miss = geometry.pickObject(level, { origin: [1.5, 0.4, 3], direction: [0, 0, -1] }, {});
  assert.equal(miss, null);
});

test('picking agrees with the drawn mesh at odd rotation angles', () => {
  // Regression: the pick box used to rotate the opposite way from the drawn box,
  // so a prop rotated 45 degrees could be picked where it is not drawn.
  const level = {
    rooms: [], walls: [], ceiling_lights: [],
    props: [{ id: 'p', model: 'core:table', x: 0, y: 0, z: 0, rotation_degrees: 45, size: [2, 0.4, 0.4] }],
    getCeilingHeight: () => 3.5
  };
  const mesh = geometry.buildLevelMesh(level, {});

  // Corner of the *drawn* box that is furthest from the prop centre.
  let corner = { x: 0, z: 0, distance: 0 };
  for (let i = 0; i < mesh.positions.length; i += 3) {
    const x = mesh.positions[i];
    const z = mesh.positions[i + 2];
    const distance = Math.hypot(x, z);
    if (distance > corner.distance) corner = { x, z, distance };
  }
  assert.ok(corner.distance > 0.6, `expected a corner away from the centre, got ${corner.distance}`);

  const down = (x, z) => geometry.rayBoxIntersection([x, 5, z], [0, -1, 0], [0, 0.2, 0], [1, 0.2, 0.2], 45);
  assert.ok(down(corner.x * 0.97, corner.z * 0.97) !== null, 'the drawn corner is pickable');
  // Mirrored across X: a wrongly signed pick box would hit this and miss the drawn corner.
  assert.equal(down(corner.x * 0.97, -corner.z * 0.97), null, 'the mirrored corner is not pickable');
});

test('level stats summarise rooms, openings, lights and props', () => {
  const level = {
    rooms: [{ id: 'r' }],
    walls: [{ id: 'w', width: 4, depth: 0.35, x: 0, y: 0, z: 0, height: null, openings: [{ kind: 'door' }, { kind: 'window' }] }],
    ceiling_lights: [{ id: 'l' }],
    props: [{ id: 'p' }],
    floor_patches: []
  };
  assert.deepEqual(geometry.levelStats(level), { rooms: 1, walls: 1, openings: 2, lights: 1, props: 1, patches: 0 });
});
