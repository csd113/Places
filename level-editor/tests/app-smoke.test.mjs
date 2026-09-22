// app-smoke.test.mjs - Boots the real editor scripts in a stubbed DOM and walks the
// complete level-building workflow end to end:
//
//   create level → add room → resize → add wall → doorway → window → light → spawn
//   → prop → select from both "views" → move → undo/redo → advanced edit → save
//   → reload → validate
//
// It cannot check pixels, but it does exercise the actual app/editor/properties/io
// code paths, and it proves that every element id the code looks up exists in
// index.html.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createEnvironment, readIndexHtml } from './support/dom.mjs';

function boot() {
  const env = createEnvironment();
  env.loadAll();
  env.run('window.app = new App();');
  env.flushFrame();
  return env;
}

/** Simulates a press/move/release sequence on the 2D canvas. */
function canvasPoint(env, worldX, worldZ) {
  const point = env.run(`(() => { const r = app.renderer; const s = r.worldToScreen(${worldX}, ${worldZ}); return { x: s.x, y: s.y }; })()`);
  return point;
}

function mouse(env, type, worldX, worldZ, extra = {}) {
  const point = canvasPoint(env, worldX, worldZ);
  const event = {
    type,
    target: env.document.getElementById('canvas-2d'),
    button: 0,
    clientX: point.x,
    clientY: point.y,
    shiftKey: !!extra.shiftKey,
    preventDefault() {},
    ...extra
  };
  if (type === 'mousedown') env.run(`app.editor.onMouseDown(${JSON.stringify({ ...event, target: null })});`);
  else if (type === 'mousemove') env.run(`app.editor.onMouseMove(${JSON.stringify({ ...event, target: null })});`);
  else env.run(`app.editor.onMouseUp(${JSON.stringify({ ...event, target: null })});`);
}

test('the editor boots with a starter level and no missing element ids', () => {
  const env = boot();
  const state = env.run(`JSON.stringify({
    hasLevel: !!app.level,
    rooms: app.level.rooms.length,
    walls: app.level.walls.length,
    openings: app.level.walls.reduce((n, w) => n + w.openings.length, 0),
    lights: app.level.ceiling_lights.length,
    props: app.level.props.length,
    catalog: app.propCatalog.size,
    tool: app.editor.currentTool,
    viewMode: app.viewMode
  })`);
  const parsed = JSON.parse(state);
  assert.equal(parsed.rooms, 1);
  assert.equal(parsed.walls, 4, 'the starter level has an enclosed room');
  assert.equal(parsed.openings, 4, 'one doorway and three windows');
  assert.ok(parsed.lights >= 1);
  assert.ok(parsed.props >= 1);
  assert.ok(parsed.catalog >= 10);
  assert.equal(parsed.tool, 'select');
  assert.equal(parsed.viewMode, '2d');

  const missing = [...env.requestedIds].filter(id => !env.idsInHtml.has(id));
  assert.deepEqual(missing, [], `index.html is missing: ${missing.join(', ')}`);
});

test('the full build workflow works through the real input handlers', () => {
  const env = boot();

  // 1. Room tool: drag a 10 x 8 m room.
  env.run(`app.setTool('room');`);
  mouse(env, 'mousedown', -10, -10);
  mouse(env, 'mousemove', 0, -2);
  mouse(env, 'mouseup', 0, -2);
  let rooms = env.run('app.level.rooms.length');
  assert.equal(rooms, 2, 'a second room was created');
  const newRoomId = env.run('[...app.editor.selectedIds][0]');
  assert.equal(env.run(`app.editor.selectedIds.has(${JSON.stringify(newRoomId)})`), true);

  // 2. Resize it with the east handle.
  env.run(`app.setTool('select');`);
  const bounds = env.run(`JSON.stringify(LiminalOps.objectBounds2D(app.level, ${JSON.stringify(newRoomId)}, app.propCatalog))`);
  const rect = JSON.parse(bounds);
  const east = canvasPoint(env, rect.x + rect.width, rect.z + rect.depth / 2);
  env.run(`app.editor.onMouseDown(${JSON.stringify({ type: 'mousedown', button: 0, clientX: east.x, clientY: east.y, preventDefault() {} })});`);
  mouse(env, 'mousemove', rect.x + rect.width + 4, rect.z + rect.depth / 2);
  mouse(env, 'mouseup', rect.x + rect.width + 4, rect.z + rect.depth / 2);
  const resized = JSON.parse(env.run(`JSON.stringify(LiminalOps.objectBounds2D(app.level, ${JSON.stringify(newRoomId)}, app.propCatalog))`));
  assert.ok(resized.width > rect.width + 3, `room grew from ${rect.width} to ${resized.width}`);

  // 3. Wall tool: drag a divider inside the starter room.
  env.run(`app.setTool('wall');`);
  mouse(env, 'mousedown', -4, 2);
  mouse(env, 'mousemove', 4, 2.2);
  mouse(env, 'mouseup', 4, 2.2);
  const wallCount = env.run('app.level.walls.length');
  assert.equal(wallCount, 5, 'a wall was added');
  const wallId = env.run('[...app.editor.selectedIds][0]');

  // 4. Door tool: click on that wall, then drag a wider doorway.
  env.run(`app.setTool('door');`);
  mouse(env, 'mousedown', -1, 2.1);
  mouse(env, 'mousemove', 1.6, 2.2);
  mouse(env, 'mouseup', 1.6, 2.2);
  const doorInfo = JSON.parse(env.run(`(() => {
    const wall = app.level.walls.find(w => w.id === ${JSON.stringify(wallId)});
    const opening = wall.openings[0];
    return JSON.stringify({ count: wall.openings.length, kind: opening && opening.kind, width: opening && opening.width, sill: opening && opening.sill,
      wallLength: LiminalGeometry.wallLength(wall), end: opening && opening.offset + opening.width });
  })()`));
  assert.equal(doorInfo.count, 1);
  assert.equal(doorInfo.kind, 'door');
  assert.ok(doorInfo.width > 1.4, `dragged doorway width ${doorInfo.width}`);
  assert.equal(doorInfo.sill, 0);
  assert.ok(doorInfo.end <= doorInfo.wallLength + 1e-6);

  // 5. Window tool: click on a perimeter wall (the east wall of the starter room).
  env.run(`app.setTool('window');`);
  const eastWall = env.run(`(() => { const wall = app.level.walls.find(w => LiminalGeometry.wallAxis(w) === 'z'); return wall.id; })()`);
  const eastPos = env.run(`(() => { const wall = app.level.walls.find(w => w.id === ${JSON.stringify(eastWall)});
    return JSON.stringify({ x: wall.x + wall.width / 2, z: wall.z + wall.depth / 2 }); })()`);
  const eastPoint = JSON.parse(eastPos);
  mouse(env, 'mousedown', eastPoint.x, eastPoint.z);
  mouse(env, 'mouseup', eastPoint.x, eastPoint.z);
  const windows = env.run(`app.level.walls.find(w => w.id === ${JSON.stringify(eastWall)}).openings.filter(o => o.kind === 'window').length`);
  assert.ok(windows >= 2, 'a window was added to that wall');

  // 6. Light, prop and spawn placement.
  env.run(`app.setTool('light');`);
  mouse(env, 'mousedown', 2, 6);
  mouse(env, 'mouseup', 2, 6);
  const lightId = env.run('[...app.editor.selectedIds][0]');
  assert.ok(env.run(`app.level.ceiling_lights.some(l => l.id === ${JSON.stringify(lightId)})`));

  env.run(`app.setTool('prop'); app.activePropModel = 'core:stove';`);
  mouse(env, 'mousedown', -1, 5);
  mouse(env, 'mouseup', -1, 5);
  const propId = env.run('[...app.editor.selectedIds][0]');
  const placedProp = JSON.parse(env.run(`(() => { const p = app.level.props.find(p => p.id === ${JSON.stringify(propId)}); return JSON.stringify(p); })()`));
  assert.equal(placedProp.model, 'core:stove');
  assert.equal(placedProp.solid, true, 'catalog solid flag is applied');

  env.run(`app.setTool('spawn');`);
  mouse(env, 'mousedown', 3, 3);
  mouse(env, 'mouseup', 3, 3);
  assert.deepEqual(JSON.parse(env.run('JSON.stringify({x: app.level.spawn.x, z: app.level.spawn.z})')), { x: 3, z: 3 });
});

test('selection is shared between the views and drives the inspector', () => {
  const env = boot();
  const wallId = env.run('app.level.walls[0].id');

  env.run(`app.viewportSelect(${JSON.stringify(wallId)}, false);`);
  assert.equal(env.run('app.editor.selectedIds.size'), 1);
  let inspector = env.document.getElementById('inspector').innerHTML;
  assert.match(inspector, /Openings/);

  const openingId = env.run('app.level.walls[0].openings[0].id');
  env.run(`app.viewportSelect(${JSON.stringify(openingId)}, false);`);
  inspector = env.document.getElementById('inspector').innerHTML;
  assert.match(inspector, /Position along wall/);

  env.run('app.viewportSelect(null, false);');
  assert.equal(env.run('app.editor.selectedIds.size'), 0);
  inspector = env.document.getElementById('inspector').innerHTML;
  assert.match(inspector, /Level surfaces/, 'the inspector falls back to level info');

  // The status bar reflects the selection.
  assert.equal(env.document.getElementById('status-selection').textContent, 'Nothing selected');
});

test('undo and redo restore whole drags and placements', () => {
  const env = boot();
  const startWalls = env.run('app.level.walls.length');

  env.run(`app.setTool('wall');`);
  mouse(env, 'mousedown', -3, 6);
  mouse(env, 'mousemove', 3, 6.1);
  mouse(env, 'mouseup', 3, 6.1);
  assert.equal(env.run('app.level.walls.length'), startWalls + 1);

  env.run('app.undo();');
  assert.equal(env.run('app.level.walls.length'), startWalls, 'undo removed the wall');
  env.run('app.redo();');
  assert.equal(env.run('app.level.walls.length'), startWalls + 1, 'redo restored the wall');

  // A drag produces a single entry, so one undo returns to the pre-drag position.
  const wallId = env.run('app.level.walls[app.level.walls.length - 1].id');
  const before = env.run(`app.level.walls.find(w => w.id === ${JSON.stringify(wallId)}).x`);
  env.run(`app.setTool('select'); app.editor.select(${JSON.stringify(wallId)});`);
  mouse(env, 'mousedown', before + 1, 6.1);
  for (let step = 1; step <= 5; step++) mouse(env, 'mousemove', before + 1 + step * 0.25, 6.1);
  mouse(env, 'mouseup', before + 2.5, 6.1);
  const after = env.run(`app.level.walls.find(w => w.id === ${JSON.stringify(wallId)}).x`);
  assert.ok(Math.abs(after - (before + 1.5)) < 0.3, `wall moved from ${before} to ${after}`);
  env.run('app.undo();');
  const undone = env.run(`app.level.walls.find(w => w.id === ${JSON.stringify(wallId)}).x`);
  assert.ok(Math.abs(undone - before) < 0.01, 'one undo restored the pre-drag position');
});

test('advanced mode exposes exact fields and they persist through save/reload', () => {
  const env = boot();
  const wallId = env.run('app.level.walls[0].id');
  env.run(`app.editor.select(${JSON.stringify(wallId)}); app.setAdvanced(true);`);
  assert.equal(env.document.getElementById('chk-advanced').checked, true);

  const inspector = env.document.getElementById('inspector').innerHTML;
  assert.match(inspector, /Base elevation Y/);
  assert.match(inspector, /Exact width/);
  assert.match(inspector, /Per-face materials/);

  // Apply a few advanced edits through the panel's own field handler.
  env.run(`app.propertiesPanel.applyField('wall', 'y', 0.25, true, 'Base elevation Y');`);
  env.run(`app.propertiesPanel.applyField('wall', 'material', 'core:wallpaper_stained_01', true, 'Material');`);
  const level = JSON.parse(env.run('app.io.levelJSON(app.level)'));

  const wall = level.walls.find(w => w.width === level.walls[0].width && w.y === 0.25) || level.walls[0];
  const target = level.walls.find(w => w.y === 0.25);
  assert.ok(target, 'the raised wall serialized its y');
  void wall;

  // Reload the saved JSON into a fresh app instance and validate it.
  env.run(`window.__reloaded = new Level(${JSON.stringify(level)});`);
  const result = env.run(`JSON.stringify(validateLevel(window.__reloaded))`);
  const parsed = JSON.parse(result);
  assert.deepEqual(parsed.errors, [], `validation errors: ${parsed.errors.join('; ')}`);
  void wallId;
});

test('view switching, prop browser and tool options are wired', () => {
  const env = boot();

  env.run(`app.setViewMode('split');`);
  assert.equal(env.run('app.viewMode'), 'split');
  assert.equal(env.document.getElementById('pane-3d').hidden, false);
  // No WebGL in this environment: the viewport must fail gracefully and the 2D
  // editor must keep working.
  const fallbackShown = env.document.getElementById('pane-3d-fallback').hidden === false;
  const viewportReady = env.run('!!app.viewport3d');
  assert.ok(fallbackShown || viewportReady, 'either the 3D preview or its fallback message is shown');
  env.flushFrame();
  env.run(`app.setViewMode('2d');`);
  assert.equal(env.document.getElementById('pane-3d').hidden, true);

  env.run(`app.setTool('prop');`);
  assert.equal(env.document.getElementById('prop-browser').hidden, false);
  const grid = env.document.getElementById('prop-grid').innerHTML;
  assert.match(grid, /core:stove/);
  assert.match(grid, /core:couch/);

  env.run(`app.propCategory = 'Appliances'; app.renderPropBrowser();`);
  const filtered = env.document.getElementById('prop-grid').innerHTML;
  assert.match(filtered, /core:sink/);
  assert.doesNotMatch(filtered, /core:couch/);

  env.run(`app.setTool('window');`);
  const options = env.document.getElementById('tool-options').innerHTML;
  assert.match(options, /Sill/);

  env.run(`app.setTool('room');`);
  assert.match(env.document.getElementById('tool-options').innerHTML, /Add walls around each room/);
});

test('the prop browser shows thumbnails with the colour swatch as fallback', () => {
  const env = boot();
  const grid = env.document.getElementById('prop-grid').innerHTML;
  assert.match(grid, /src="assets\/thumbs\/washing_machine\.png"/);
  assert.match(grid, /src="assets\/thumbs\/couch\.png"/);
  assert.match(grid, /class="prop-thumb"/);
  assert.match(grid, /--prop-color: rgb\(/);
  assert.match(grid, /onerror=/, 'a missing thumbnail hides itself instead of breaking the grid');

  // A catalogue entry with no model has no thumbnail, but still renders.
  env.run(`app.propCatalog = LiminalProps.PropCatalog.fromJSON({ props: [
    { id: 'pack:no_thumb', name: 'No Thumb', category: 'Other', size: [0.5, 0.5, 0.5], color: '#123456' }
  ] }); app.renderPropBrowser();`);
  const custom = env.document.getElementById('prop-grid').innerHTML;
  assert.match(custom, /pack:no_thumb/);
  assert.match(custom, /class="prop-thumb"/);
  assert.match(custom, /--prop-color: rgb\(18,52,86\)/);
  assert.doesNotMatch(custom, /<img/);

  // A model whose thumbnail file is missing still renders the swatch behind it.
  env.run(`app.propCatalog = LiminalProps.PropCatalog.fromJSON({ props: [
    { id: 'core:mystery', name: 'Mystery', category: 'Other', size: [0.5, 0.5, 0.5], color: '#ffffff', model: 'models/mystery.glb' }
  ] }); app.renderPropBrowser();`);
  const missing = env.document.getElementById('prop-grid').innerHTML;
  assert.match(missing, /src="assets\/thumbs\/mystery\.png"/);
  assert.match(missing, /class="prop-thumb"/);
  assert.match(missing, /onerror=/);
});

test('loaded proxy geometry is handed to the viewport and marks it dirty', async () => {
  const env = boot();
  const model = env.run('app.level.props[0].model');
  const payload = { props: {} };
  payload.props[model] = {
    name: 'Proxy',
    model: 'models/proxy.glb',
    parts: [{ shape: 'box', center: [0, 0.45, 0], size: [0.46, 0.05, 0.46], rotation: [0, 0, 0], color: '#a08a6a' }]
  };
  env.run(`window.__proxyMarks = 0; app.viewport3d = { markDirty() { window.__proxyMarks++; } };`);
  env.run(`LiminalProps.loadPropProxies = async () => LiminalProps.PropProxies.fromJSON(${JSON.stringify(payload)});`);
  await env.run('app.loadPropProxies()');
  assert.equal(env.run('app.propProxies.size'), 1);
  assert.equal(env.run(`app.propProxies.has(${JSON.stringify(model)})`), true);
  assert.ok(env.run('window.__proxyMarks') >= 1, 'the 3D mesh is marked dirty after proxies load');
});

test('placing and deleting single objects is undoable', () => {
  const env = boot();
  const propsBefore = env.run('app.level.props.length');

  env.run(`app.setTool('prop'); app.activePropModel = 'core:sink';`);
  mouse(env, 'mousedown', 6, -2);
  mouse(env, 'mouseup', 6, -2);
  assert.equal(env.run('app.level.props.length'), propsBefore + 1, 'the prop was placed by a click');
  assert.equal(env.run('app.history.lastAction'), 'Add prop');

  env.run('app.undo();');
  assert.equal(env.run('app.level.props.length'), propsBefore, 'one undo removed the click-placed prop');
  env.run('app.redo();');
  assert.equal(env.run('app.level.props.length'), propsBefore + 1);

  // Same for lights.
  const lightsBefore = env.run('app.level.ceiling_lights.length');
  env.run(`app.setTool('light');`);
  mouse(env, 'mousedown', 6, 4);
  mouse(env, 'mouseup', 6, 4);
  assert.equal(env.run('app.level.ceiling_lights.length'), lightsBefore + 1);
  env.run('app.undo();');
  assert.equal(env.run('app.level.ceiling_lights.length'), lightsBefore);
});

test('a click places a doorway exactly where the hover ghost was shown', () => {
  const env = boot();
  const wallId = env.run('app.level.walls[0].id');
  env.run(`app.setTool('door'); app.editor.select(${JSON.stringify(wallId)});`);

  // Hover first: the ghost is centred on the pointer.
  const wallInfo = env.run(`(() => { const wall = app.level.walls.find(w => w.id === ${JSON.stringify(wallId)});
    const base = LiminalGeometry.wallMinCorner(wall);
    return JSON.stringify({ x: base.x + 6, z: base.z + 0.175 }); })()`);
  const point = JSON.parse(wallInfo);
  mouse(env, 'mousemove', point.x, point.z);
  const ghost = JSON.parse(env.run('JSON.stringify(app.editor.preview)'));
  assert.ok(ghost && ghost.type === 'opening', 'a ghost opening is previewed');

  mouse(env, 'mousedown', point.x, point.z);
  mouse(env, 'mouseup', point.x, point.z);

  const placed = JSON.parse(env.run(`(() => {
    const wall = app.level.walls.find(w => w.id === ${JSON.stringify(wallId)});
    const opening = wall.openings[wall.openings.length - 1];
    return JSON.stringify({ offset: opening.offset, width: opening.width, rect: LiminalOps.openingBounds2D(wall, opening) });
  })()`));
  assert.equal(placed.width, ghost.width, 'the placed doorway keeps the ghost width');
  assert.deepEqual(placed.rect, ghost.rect, 'the placed doorway keeps the ghost position');
  assert.deepEqual(env.run('JSON.stringify(validateLevel(app.level).errors)'), '[]');
});

test('keyboard shortcuts drive the tools through the real handler', () => {
  const env = boot();
  const tool = () => env.run('app.editor.currentTool');

  for (const [key, expected] of [['r', 'room'], ['w', 'wall'], ['d', 'door'], ['n', 'window'],
    ['l', 'light'], ['p', 'prop'], ['m', 'spawn'], ['v', 'select']]) {
    env.fireWindow('keydown', { key, target: env.document.body });
    assert.equal(tool(), expected, `${key} selects ${expected}`);
  }

  // Advanced-only tool.
  env.fireWindow('keydown', { key: 't', target: env.document.body });
  assert.equal(tool(), 'select', 'the patch tool stays hidden in simple mode');
  env.run('app.setAdvanced(true);');
  env.fireWindow('keydown', { key: 't', target: env.document.body });
  assert.equal(tool(), 'patch');
  env.fireWindow('keydown', { key: 'v', target: env.document.body });

  // View modes.
  env.fireWindow('keydown', { key: '2', target: env.document.body });
  assert.equal(env.run('app.viewMode'), '3d');
  env.fireWindow('keydown', { key: '3', target: env.document.body });
  assert.equal(env.run('app.viewMode'), 'split');
  env.fireWindow('keydown', { key: '1', target: env.document.body });
  assert.equal(env.run('app.viewMode'), '2d');

  // Snap toggle.
  const snapBefore = env.run('app.editor.snapEnabled');
  env.fireWindow('keydown', { key: 'g', target: env.document.body });
  assert.equal(env.run('app.editor.snapEnabled'), !snapBefore);

  // Rotation and delete on a selected prop.
  const propId = env.run('app.level.props[0].id');
  env.run(`app.editor.select(${JSON.stringify(propId)});`);
  const rotationBefore = env.run('app.level.props[0].rotation_degrees');
  env.fireWindow('keydown', { key: 'e', target: env.document.body });
  assert.notEqual(env.run('app.level.props[0].rotation_degrees'), rotationBefore);

  const propsBefore = env.run('app.level.props.length');
  env.fireWindow('keydown', { key: 'Delete', target: env.document.body });
  assert.equal(env.run('app.level.props.length'), propsBefore - 1);

  // Ctrl+Z restores it.
  env.fireWindow('keydown', { key: 'z', ctrlKey: true, target: env.document.body });
  assert.equal(env.run('app.level.props.length'), propsBefore);

  // Escape cancels back to the select tool.
  env.run(`app.setTool('door');`);
  env.fireWindow('keydown', { key: 'Escape', target: env.document.body });
  assert.equal(tool(), 'select');
});

test('every script referenced by index.html exists', () => {
  const env = createEnvironment();
  const loaded = env.loadAll();
  const referenced = Array.from(readIndexHtml().matchAll(/<script src="([^"]+)"/g)).map(m => m[1]);
  assert.deepEqual(referenced, [
    'js/jszip.min.js', 'js/model.js', 'js/lighting.js', 'js/geometry.js', 'js/props.js',
    'js/history.js', 'js/ops.js', 'js/renderer.js', 'js/camera3d.js', 'js/viewport3d.js',
    'js/properties.js', 'js/io.js', 'js/editor.js', 'js/app.js'
  ]);
  assert.equal(loaded.length, referenced.length, 'every referenced script exists on disk');
  assert.deepEqual(env.run('typeof JSZip'), 'function', 'level packs have JSZip available');
});

test('every object type renders its inspector panel', () => {
  const env = boot();
  const inspector = () => env.document.getElementById('inspector').innerHTML;
  const checks = [
    ['app.level.rooms[0].id', /Ceiling height/],
    ['app.level.walls[0].id', /Openings/],
    ['app.level.ceiling_lights[0].id', /Brightness/],
    ['app.level.props[0].id', /Model/],
    ["'spawn'", /Facing \(degrees\)/]
  ];
  for (const [expression, pattern] of checks) {
    env.run(`app.editor.clearSelection(); app.editor.select(${expression});`);
    const html = inspector();
    assert.match(html, pattern, `${expression} panel`);
    assert.doesNotMatch(html, /undefined/, `${expression} panel has no undefined values`);
  }

  // Multi-selection and the level panel.
  env.run(`app.editor.selectAll();`);
  assert.match(inspector(), /objects selected/);
  env.run(`app.editor.clearSelection();`);
  assert.match(inspector(), /Level surfaces/);

  // Advanced mode adds exact fields without breaking the simple ones.
  env.run(`app.editor.select(app.level.props[0].id); app.setAdvanced(true);`);
  const advanced = inspector();
  assert.match(advanced, /Vertical offset Y/);
  assert.match(advanced, /Scale/);
  assert.match(advanced, /Object ID/);
});

test('opening the sample fixture level works and it still validates', async () => {
  const env = boot();
  const fs = await import('node:fs');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const samplePath = path.resolve(here, '../../tests/fixtures/levels/test_room.json');
  const json = fs.readFileSync(samplePath, 'utf8');

  const opened = env.run(`app.io.importJSONString(${JSON.stringify(json)}, 'test_room.json')`);
  assert.equal(opened, true);
  assert.equal(env.run('app.level.id'), 'test_room');
  const openings = env.run(`app.level.walls.reduce((n, w) => n + w.openings.length, 0)`);
  assert.ok(openings >= 2, 'the sample level demonstrates doors and windows');
  const props = env.run('app.level.props.length');
  assert.ok(props >= 1, 'the sample level demonstrates props');
  const errors = env.run('JSON.stringify(validateLevel(app.level).errors)');
  assert.deepEqual(JSON.parse(errors), []);
  env.flushFrame();
});
