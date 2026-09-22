// ops.test.mjs - Editing operations, selection, undo/redo and history behaviour.
import test from 'node:test';
import assert from 'node:assert/strict';
import model from '../js/model.js';
import ops from '../js/ops.js';
import geometry from '../js/geometry.js';
import props from '../js/props.js';
import history from '../js/history.js';

const { Level, validateLevel } = model;
const { HistoryManager } = history;
const catalog = props.PropCatalog.builtin();

function baseLevel() {
  return new Level({
    id: 'ops_level',
    name: 'Ops Level',
    spawn: { x: 0, z: 0, yaw_degrees: 0 },
    rooms: [{ x: -5, z: -5, width: 10, depth: 10, height: 3.5 }],
    walls: [{ x: -5, z: -5, width: 10, depth: 0.35 }]
  });
}

test('rooms can be created and resized with handles', () => {
  const level = baseLevel();
  const room = ops.createRoom(level, { x: 0, z: 0, width: 6, depth: 4, height: 3.5 });
  assert.equal(level.rooms.length, 2);
  assert.equal(room.width, 6);

  const before = ops.objectBounds2D(level, room.id);
  ops.resizeObject(level, room.id, 'e', { x: before.x + 8, z: before.z }, before);
  assert.equal(room.width, 8);
  ops.resizeObject(level, room.id, 's', { x: before.x, z: before.z - 2 }, { x: room.x, z: room.z, width: room.width, depth: room.depth });
  assert.ok(room.depth >= 0.05);
});

test('resizing a room never produces a degenerate box', () => {
  const level = baseLevel();
  const room = level.rooms[0];
  const bounds = ops.objectBounds2D(level, room.id);
  ops.resizeObject(level, room.id, 'se', { x: bounds.x - 3, z: bounds.z - 3 }, bounds);
  assert.ok(room.width >= ops.MIN_SIZE);
  assert.ok(room.depth >= ops.MIN_SIZE);
});

test('objects move together and keep their own axes', () => {
  const level = baseLevel();
  ops.addLight(level, { x: 1, z: 1 });
  ops.addProp(level, { model: 'core:crate', x: 2, z: 2 }, catalog);
  const ids = [level.walls[0].id, level.ceiling_lights[0].id, level.props[0].id, 'spawn'];
  const moved = ops.moveObjects(level, ids, 1.5, -0.5);
  assert.equal(moved, 4);
  assert.equal(level.walls[0].x, -3.5);
  assert.equal(level.ceiling_lights[0].z, 0.5);
  assert.equal(level.props[0].x, 3.5);
  assert.equal(level.spawn.x, 1.5);
});

test('a doorway centre aligns with the clicked point and clamps to the wall', () => {
  const level = baseLevel();
  const wall = level.walls[0];
  const opening = ops.addOpeningAtPoint(level, wall, { x: 0, z: -5.1 }, 'door', {});
  assert.ok(opening);
  assert.equal(opening.kind, 'door');
  assert.equal(opening.sill, 0);
  assert.equal(opening.height, 2.1);
  // Centred on the click, so the opening spans x -0.5..0.5.
  assert.equal(opening.offset, 4.5);
  assert.equal(opening.width, 1.0);

  const closeToEnd = ops.addOpeningAtPoint(level, wall, { x: 4.9, z: -5.1 }, 'door', { width: 2 });
  assert.ok(closeToEnd.offset + closeToEnd.width <= geometry.wallLength(wall) + 1e-6);
});

test('a dragged doorway span sets position and width at once', () => {
  const level = baseLevel();
  const wall = level.walls[0];
  const opening = ops.addOpening(wall, { kind: 'door', offset: 1.0, width: 2.4, height: 2.1 });
  assert.equal(opening.offset, 1.0);
  assert.equal(opening.width, 2.4);
  assert.ok(opening.offset + opening.width <= geometry.wallLength(wall));
});

test('windows use a sill and doors do not', () => {
  const level = baseLevel();
  const wall = level.walls[0];
  const windowOpening = ops.addOpeningAtPoint(level, wall, { x: 2, z: -5.1 }, 'window', {});
  assert.equal(windowOpening.kind, 'window');
  assert.equal(windowOpening.sill, 1.0);
  assert.equal(windowOpening.height, 1.2);

  const door = ops.addOpeningAtPoint(level, wall, { x: -2, z: -5.1 }, 'door', {});
  assert.equal(door.sill, 0);
});

test('an opening wider than its wall is clamped instead of invalid', () => {
  const level = baseLevel();
  const wall = level.walls[0];
  const opening = ops.addOpening(wall, { kind: 'passage', offset: 9, width: 40, height: 3 });
  assert.equal(opening.offset, 0);
  assert.equal(opening.width, 10);
  assert.deepEqual(validateLevel(level).errors, []);
});

test('moving a wall moves its openings with it', () => {
  const level = baseLevel();
  const wall = level.walls[0];
  const opening = ops.addOpening(wall, { kind: 'door', offset: 2, width: 1, height: 2.1 });
  ops.moveObjects(level, [wall.id], 3, 0);
  const rect = ops.openingBounds2D(wall, opening);
  assert.equal(rect.x, -5 + 3 + 2);
});

test('openings can be resized along their wall', () => {
  const level = baseLevel();
  const wall = level.walls[0];
  const opening = ops.addOpening(wall, { kind: 'door', offset: 2, width: 1, height: 2.1 });
  const initial = { offset: opening.offset, width: opening.width };

  // Drag the east jamb outwards from x=-2 to x=-0.5 (wall starts at x=-5).
  ops.resizeObject(level, opening.id, 'e', { x: -5 + 4.5, z: -5.1 }, initial);
  assert.equal(opening.offset, 2);
  assert.equal(opening.width, 2.5);

  // Drag the west jamb back outwards to offset 1.5, keeping the east jamb at 4.5.
  ops.resizeObject(level, opening.id, 'w', { x: -5 + 1.5, z: -5.1 }, initial);
  assert.equal(opening.offset, 1.5);
  assert.equal(opening.width, 1.5);

  // The opening can never leave its wall.
  ops.resizeObject(level, opening.id, 'e', { x: 100, z: -5.1 }, initial);
  assert.ok(opening.offset + opening.width <= geometry.wallLength(wall) + 1e-6);
  assert.ok(opening.width >= 0.2);
});

test('hit testing prefers openings, props and lights over the room behind them', () => {
  const level = baseLevel();
  const wall = level.walls[0];
  const opening = ops.addOpening(wall, { kind: 'door', offset: 4, width: 1.4, height: 2.1 });
  ops.addProp(level, { model: 'core:crate', x: 0, z: -3 }, catalog);
  ops.addLight(level, { x: 2, z: 2 });

  assert.equal(ops.hitTest2D(level, { x: -5 + 4.7, z: -5.1 }, 0.2, catalog).id, opening.id);
  assert.equal(ops.hitTest2D(level, { x: 0, z: -3 }, 0.2, catalog).kind, 'prop');
  assert.equal(ops.hitTest2D(level, { x: 2, z: 2 }, 0.2, catalog).kind, 'light');
  assert.equal(ops.hitTest2D(level, { x: -5 + 2.5, z: -5.1 }, 0.2, catalog).id, wall.id);
  assert.equal(ops.hitTest2D(level, { x: 100, z: 100 }, 0.2, catalog), null);
});

test('wall picking for doors/windows finds the nearest wall within tolerance', () => {
  const level = baseLevel();
  const wall = ops.wallAtPoint(level, { x: 0, z: -5.2 }, 0.4);
  assert.equal(wall.id, level.walls[0].id);
  assert.equal(ops.wallAtPoint(level, { x: 0, z: 0 }, 0.4), null);
});

test('marquee selection collects overlapping objects', () => {
  const level = baseLevel();
  // Place the prop outside the room so the marquee can isolate it.
  const prop = ops.addProp(level, { model: 'core:chair', x: 20, z: 20 }, catalog);
  const ids = ops.objectsInRect(level, { x: -1, z: -1, width: 4, depth: 4 }, catalog);
  // The room spans the whole marquee and the spawn is inside it too.
  assert.deepEqual(ids.sort(), [level.rooms[0].id, 'spawn'].sort());

  const onlyProp = ops.objectsInRect(level, { x: 19.5, z: 19.5, width: 1, depth: 1 }, catalog);
  assert.deepEqual(onlyProp, [prop.id]);
});

test('deleting removes openings, props and walls but keeps the level valid', () => {
  const level = baseLevel();
  const wall = level.walls[0];
  const opening = ops.addOpening(wall, { kind: 'door', offset: 2, width: 1, height: 2.1 });
  const prop = ops.addProp(level, { model: 'core:crate', x: 1, z: 1 }, catalog);

  const result = ops.deleteObjects(level, [opening.id, prop.id]);
  assert.equal(result.deleted, 2);
  assert.equal(wall.openings.length, 0);
  assert.equal(level.props.length, 0);
  assert.equal(result.spawnReset, false);

  const spawnResult = ops.deleteObjects(level, ['spawn', level.walls[0].id]);
  assert.equal(spawnResult.spawnReset, true);
  assert.equal(level.spawn.x, 0);
});

test('the last room cannot be deleted and the spawn cannot be removed', () => {
  const level = baseLevel();
  const result = ops.deleteObjects(level, [level.rooms[0].id, 'spawn']);
  assert.equal(level.rooms.length, 1);
  assert.ok(level.spawn);
  assert.equal(result.spawnReset, true);
  assert.deepEqual(validateLevel(level).errors, []);
});

test('duplicating a room offsets the copy and selects it', () => {
  const level = baseLevel();
  const ids = ops.duplicateObjects(level, [level.rooms[0].id], { dx: 2, dz: 0 });
  assert.equal(level.rooms.length, 2);
  assert.equal(ids.length, 1);
  assert.equal(level.rooms[1].x, level.rooms[0].x + 2);
});

test('duplicating an opening keeps it inside its wall', () => {
  const level = baseLevel();
  const wall = level.walls[0];
  const opening = ops.addOpening(wall, { kind: 'door', offset: 1, width: 1, height: 2.1 });
  const ids = ops.duplicateObjects(level, [opening.id], { dx: 2, dz: 2 });
  assert.equal(wall.openings.length, 2);
  const copy = wall.openings.find(o => o.id === ids[0]);
  assert.equal(copy.offset, 3);
  assert.ok(copy.offset + copy.width <= geometry.wallLength(wall));
  assert.deepEqual(validateLevel(level).errors, []);
});

test('undo/redo restores creates, moves, resizes and opening edits', () => {
  const historyManager = new HistoryManager();
  let level = baseLevel();
  historyManager.pushState(level, 'Initial');

  const room = ops.createRoom(level, { x: 4, z: 4, width: 6, depth: 6, height: 3.5 });
  historyManager.pushState(level, 'Add Room');

  const opening = ops.addOpening(level.walls[0], { kind: 'door', offset: 2, width: 1.2, height: 2.1 });
  historyManager.pushState(level, 'Add Door');

  ops.moveObjects(level, [room.id], 2, 0);
  historyManager.pushState(level, 'Move Room');

  level = historyManager.undo(level);
  assert.equal(level.rooms[1].x, 4, 'move undone');
  level = historyManager.undo(level);
  assert.equal(level.walls[0].openings.length, 0, 'door undone');
  level = historyManager.undo(level);
  assert.equal(level.rooms.length, 1, 'room undone');
  level = historyManager.redo(level);
  assert.equal(level.rooms.length, 2, 'room redone');
  level = historyManager.redo(level);
  assert.equal(level.walls[0].openings.length, 1, 'door redone');
  level = historyManager.redo(level);
  assert.equal(level.rooms[1].x, 6, 'move redone');
  void opening;
});

test('a drag is committed as a single history entry', () => {
  const historyManager = new HistoryManager();
  const level = baseLevel();
  historyManager.pushState(level, 'Initial');
  const wall = level.walls[0];

  // Simulate a drag: many incremental moves, one commit at the end.
  for (let i = 0; i < 20; i++) ops.moveObjects(level, [wall.id], 0.1, 0);
  historyManager.pushState(level, 'Move Wall');
  assert.equal(historyManager.depth(), 2);

  const undone = historyManager.undo(level);
  assert.equal(undone.walls[0].x, -5);
  assert.equal(historyManager.depth(), 1);
  assert.equal(historyManager.canUndo(), false);
  assert.equal(historyManager.canRedo(), true);
});

test('shrinking a wall keeps its openings inside it', () => {
  const level = baseLevel();
  const wall = level.walls[0];
  ops.addOpening(wall, { kind: 'door', offset: 7, width: 2, height: 2.1 });
  assert.deepEqual(validateLevel(level).errors, []);

  const bounds = ops.objectBounds2D(level, wall.id);
  ops.resizeObject(level, wall.id, 'e', { x: bounds.x + 4, z: bounds.z }, bounds);
  assert.equal(geometry.wallLength(wall), 4);
  assert.deepEqual(validateLevel(level).errors, [], 'openings were clamped into the shorter wall');
  for (const opening of wall.openings) {
    assert.ok(opening.offset + opening.width <= geometry.wallLength(wall) + 1e-6);
    assert.ok(opening.width >= 0.2);
  }
});

test('advanced properties survive a serialize/deserialize round trip', () => {
  const level = baseLevel();
  const wall = level.walls[0];
  wall.material = 'core:wallpaper_stained_01';
  wall.faces = { north: 'pack:tile', south: 'core:wallpaper_yellow_01' };
  wall.height = 2.4;
  wall.y = 0.2;
  const opening = ops.addOpening(wall, { kind: 'vent', offset: 3, width: 0.6, height: 0.4, sill: 2.2 });
  const prop = ops.addProp(level, { model: 'core:stove', x: -3, y: -0.35, z: 1, rotation_degrees: 45, scale: 1.2, solid: true }, catalog);

  const json = ops.serializeLevel(level);
  const restored = ops.deserializeLevel(json);
  const restoredWall = restored.walls[0];
  // The simple "wall material" control round-trips through the format's faces map.
  assert.equal(restoredWall.material, 'core:wallpaper_stained_01');
  assert.deepEqual(restoredWall.faces, { north: 'pack:tile', south: 'core:wallpaper_yellow_01', east: 'core:wallpaper_stained_01', west: 'core:wallpaper_stained_01' });
  assert.equal(restoredWall.height, 2.4);
  assert.equal(restoredWall.y, 0.2);
  assert.equal(restoredWall.openings[0].kind, 'vent');
  assert.equal(restoredWall.openings[0].sill, 2.2);
  const restoredProp = restored.props[0];
  assert.equal(restoredProp.model, 'core:stove');
  assert.equal(restoredProp.y, -0.35);
  assert.equal(restoredProp.rotation_degrees, 45);
  assert.equal(restoredProp.scale, 1.2);
  assert.equal(restoredProp.solid, true);
  void opening;
  void prop;
});

test('a level built through ops passes validation', () => {
  const level = new Level({ id: 'workflow', name: 'Workflow', spawn: { x: 0, z: 0 } });
  level.rooms = [];
  ops.createRoom(level, { x: -6, z: -6, width: 12, depth: 12, height: 3.5 });
  const north = ops.createWall(level, { x: -6, z: -6, width: 12, depth: 0.35 });
  ops.createWall(level, { x: -6, z: 5.65, width: 12, depth: 0.35 });
  ops.addOpeningAtPoint(level, north, { x: 0, z: -6 }, 'door', {});
  ops.addOpeningAtPoint(level, north, { x: 3, z: -6 }, 'window', {});
  ops.addLight(level, { x: 0, z: 0 });
  ops.addProp(level, { model: 'core:sink', x: -5, z: -5 }, catalog);
  ops.setSpawn(level, { x: 0, z: 0, yaw_degrees: 90 });
  const result = validateLevel(level);
  assert.deepEqual(result.errors, []);
});
