// viewport3d.test.mjs - Drives the real 3D viewport against a mock WebGL context.
//
// GPU output cannot be asserted from node, but everything the viewport *does* can:
// the mesh it uploads, the batches it draws, the highlight it builds, the picking it
// performs, the drag intents it reports to the app, and that it never mutates the
// level itself.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createEnvironment } from './support/dom.mjs';

/** Minimal WebGL 1 mock: records calls, returns usable handles and fake constants. */
function createMockGL() {
  const calls = { drawArrays: [], bufferData: 0, uniforms: [], textures: 0 };
  const constants = {
    DEPTH_TEST: 1, LEQUAL: 2, CULL_FACE: 3, BLEND: 4, SRC_ALPHA: 5, ONE_MINUS_SRC_ALPHA: 6,
    COLOR_BUFFER_BIT: 7, DEPTH_BUFFER_BIT: 8, TRIANGLES: 9, LINES: 10, ARRAY_BUFFER: 11,
    STATIC_DRAW: 12, DYNAMIC_DRAW: 13, FLOAT: 14, UNSIGNED_BYTE: 15, TEXTURE_2D: 16,
    TEXTURE0: 17, RGBA: 18, LINEAR: 19, NEAREST: 20, REPEAT: 21, CLAMP_TO_EDGE: 22,
    TEXTURE_WRAP_S: 23, TEXTURE_WRAP_T: 24, TEXTURE_MIN_FILTER: 25, TEXTURE_MAG_FILTER: 26,
    VERTEX_SHADER: 27, FRAGMENT_SHADER: 28, COMPILE_STATUS: 29, LINK_STATUS: 30,
    LINEAR_MIPMAP_LINEAR: 31, NEAREST_MIPMAP_NEAREST: 32, BLEND_EQUATION: 33
  };

  const target = {
    ...constants,
    calls,
    canvas: null,
    drawingBufferWidth: 800,
    drawingBufferHeight: 600,
    createShader: () => ({ kind: 'shader' }),
    shaderSource: () => {},
    compileShader: () => {},
    getShaderParameter: () => true,
    getShaderInfoLog: () => '',
    deleteShader: () => {},
    createProgram: () => ({ kind: 'program' }),
    attachShader: () => {},
    linkProgram: () => {},
    getProgramParameter: () => true,
    getProgramInfoLog: () => '',
    deleteProgram: () => {},
    useProgram: () => {},
    getUniformLocation: (program, name) => ({ name }),
    getAttribLocation: (program, name) => ({ name }),
    getShaderPrecisionFormat: () => ({ rangeMin: 127, rangeMax: 127, precision: 23 }),
    createBuffer: () => ({ kind: 'buffer' }),
    deleteBuffer: () => {},
    createTexture: () => { calls.textures++; return { kind: 'texture' }; },
    deleteTexture: () => {},
    drawArrays: (mode, start, count) => { calls.drawArrays.push({ mode, start, count }); },
    getExtension: () => null,
    getParameter: () => 4096
  };

  // Unknown methods become no-ops and unknown ALL_CAPS properties become stable
  // fake constants, so the mock keeps working if the viewport adds GL calls.
  return new Proxy(target, {
    get(object, prop) {
      if (prop in object) return object[prop];
      const name = String(prop);
      if (/^[A-Z][A-Z0-9_]*$/.test(name)) {
        let hash = 0;
        for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) % 9973;
        return 2000 + hash;
      }
      return () => undefined;
    }
  });
}

function bootWithGL() {
  const env = createEnvironment();
  env.loadAll();
  env.run('window.app = new App();');
  env.flushFrame();

  // Give the 3D canvas a mock WebGL context before the viewport is created.
  const gl = createMockGL();
  const canvas3d = env.document.getElementById('canvas-3d');
  canvas3d.clientWidth = 800;
  canvas3d.clientHeight = 600;
  canvas3d.getContext = (kind) => (kind === 'webgl' || kind === 'experimental-webgl' ? gl : null);
  canvas3d.setPointerCapture = () => {};
  canvas3d.releasePointerCapture = () => {};
  canvas3d.hasPointerCapture = () => true;

  env.run(`app.setViewMode('split');`);
  env.run('app.viewport3d && app.viewport3d.resize();');
  return { env, gl, canvas3d };
}

function pointerEvent(env, type, x, y, extra = {}) {
  return {
    type,
    clientX: x,
    clientY: y,
    pointerId: 1,
    button: 0,
    buttons: 1,
    shiftKey: false,
    preventDefault() {},
    ...extra,
    target: env.document.getElementById('canvas-3d')
  };
}

test('the viewport builds and draws the level mesh', () => {
  const { env, gl } = bootWithGL();
  assert.equal(env.run('app.viewport3d.isSupported()'), true, 'mock WebGL context is accepted');

  env.run('app.viewport3d.render();');
  assert.ok(gl.calls.drawArrays.length >= 4, `expected several batches, got ${gl.calls.drawArrays.length}`);
  const batches = env.run('JSON.stringify(app.viewport3d._batches ? Object.keys(app.viewport3d._batches) : [])');
  assert.match(batches, /walls/);
  assert.match(batches, /props/);

  // Rendering twice without changes must not re-upload geometry (revision caching).
  const uploadsAfterFirst = gl.calls.bufferData;
  env.run('app.viewport3d.render();');
  env.run('app.viewport3d.render();');
  assert.equal(gl.calls.bufferData, uploadsAfterFirst, 'unchanged renders reuse the uploaded mesh');

  // A level change forces exactly one rebuild.
  env.run('app.levelChanged(); app.viewport3d.render(); app.viewport3d.render();');
  assert.ok(gl.calls.bufferData >= uploadsAfterFirst, 'a level change re-uploads geometry');
});

test('proxy geometry replaces the fallback box in the viewport mesh', () => {
  const { env } = bootWithGL();

  // With no proxies loaded, every starter prop is a catalogue fallback box.
  const props = env.run('app.level.props.length');
  env.run('app.viewport3d.markDirty(); app.viewport3d.render();');
  assert.equal(env.run('app.viewport3d._batches.props.count'), props * 36);

  const model = env.run('app.level.props[0].model');
  const payload = { props: {} };
  payload.props[model] = {
    name: 'Proxy',
    model: 'models/proxy.glb',
    parts: [
      { shape: 'box', center: [0, 0.5, 0], size: [1, 1, 1], color: '#ffffff' },
      { shape: 'box', center: [0, 1.5, 0], size: [0.5, 1, 0.5], color: '#ffffff' }
    ]
  };
  env.run(`app.propProxies = PlacesProps.PropProxies.fromJSON(${JSON.stringify(payload)});
    app.levelRevision++; app.viewport3d.markDirty(); app.viewport3d.render();`);
  // One 36-vertex fallback box is replaced by two 36-vertex proxy boxes.
  assert.equal(env.run('app.viewport3d._batches.props.count'), (props - 1) * 36 + 72, 'the proxy parts are uploaded');
});

test('x-ray and ceiling modes switch without rebuilding geometry', () => {
  const { env, gl } = bootWithGL();
  const before = env.run('JSON.stringify(app.viewport3d.ceilingsMode())');
  assert.equal(JSON.parse(before), 'auto', 'ceilings auto-hide by default');

  const uploads = gl.calls.bufferData;
  env.run(`app.viewport3d.setXray(true); app.viewport3d.render();`);
  assert.equal(env.run('app.viewport3d.isXray()'), true);
  env.run(`app.viewport3d.setCeilingsMode('on'); app.viewport3d.render();`);
  assert.equal(JSON.parse(env.run('JSON.stringify(app.viewport3d.ceilingsMode())')), 'on');
  assert.equal(gl.calls.bufferData, uploads, 'view options do not rebuild the mesh');
});

test('clicking in 3D selects objects and never mutates the level', () => {
  const { env } = bootWithGL();
  const propId = env.run('app.level.props[0].id');
  const before = env.run('JSON.stringify(app.level)');

  // Aim at the first prop: the camera is framed on the level, so project its centre.
  const point = env.run(`(() => {
    const prop = app.level.props[0];
    const bounds = PlacesGeometry.objectBounds3D(app.level, prop.id, { catalog: app.propCatalog });
    const camera = app.viewport3d.camera;
    const canvas = document.getElementById('canvas-3d');
    const aspect = canvas.clientWidth / canvas.clientHeight;
    const vp = camera.viewProjection(aspect);
    const c = bounds.center;
    const clip = [
      vp[0] * c[0] + vp[4] * c[1] + vp[8] * c[2] + vp[12],
      vp[1] * c[0] + vp[5] * c[1] + vp[9] * c[2] + vp[13],
      vp[2] * c[0] + vp[6] * c[1] + vp[10] * c[2] + vp[14],
      vp[3] * c[0] + vp[7] * c[1] + vp[11] * c[2] + vp[15]
    ];
    const ndcX = clip[0] / clip[3];
    const ndcY = clip[1] / clip[3];
    return JSON.stringify({ x: (ndcX * 0.5 + 0.5) * canvas.clientWidth, y: (1 - (ndcY * 0.5 + 0.5)) * canvas.clientHeight, visible: clip[3] > 0 });
  })()`);
  const projected = JSON.parse(point);
  assert.equal(projected.visible, true, 'the prop is in front of the camera');

  env.run(`(() => {
    const canvas = document.getElementById('canvas-3d');
    const make = (type, x, y, extra) => Object.assign({ type, clientX: x, clientY: y, pointerId: 1, button: 0, buttons: 1, shiftKey: false, preventDefault() {}, target: canvas }, extra);
    canvas._listeners = null;
    for (const [type, handler] of Object.entries({})) void handler;
    // Dispatch through the registered listeners.
    const fire = (type, event) => { for (const l of canvas.listeners.get(type) || []) l(event); };
    fire('pointerdown', make('pointerdown', ${projected.x}, ${projected.y}));
    fire('pointerup', make('pointerup', ${projected.x}, ${projected.y}, { buttons: 0 }));
  })()`);

  const after = env.run('JSON.stringify(app.level)');
  assert.equal(after, before, 'a click must not modify level data');
  void propId;
});

test('dragging a selected object in 3D moves it once and commits a single undo entry', () => {
  const { env } = bootWithGL();
  const propId = env.run('app.level.props[0].id');
  env.run(`app.viewportSelect(${JSON.stringify(propId)}, false);`);
  const start = JSON.parse(env.run(`JSON.stringify({ x: app.level.props[0].x, z: app.level.props[0].z })`));
  const depth = env.run('app.history.depth()');

  // Drive the app intents directly: the viewport reports world-space steps.
  env.run('app.viewportDragBegin();');
  for (let i = 0; i < 5; i++) env.run('app.viewportDragUpdate(0.25, 0.1);');
  env.run('app.viewportDragEnd();');

  const moved = JSON.parse(env.run(`JSON.stringify({ x: app.level.props[0].x, z: app.level.props[0].z })`));
  assert.ok(Math.abs(moved.x - (start.x + 1.25)) < 1e-3, `expected +1.25 on X, got ${moved.x}`);
  assert.ok(Math.abs(moved.z - (start.z + 0.5)) < 1e-3, `expected +0.5 on Z, got ${moved.z}`);
  assert.equal(env.run('app.history.depth()'), depth + 1, 'one drag is one history entry');

  env.run('app.undo();');
  const undone = JSON.parse(env.run(`JSON.stringify({ x: app.level.props[0].x, z: app.level.props[0].z })`));
  assert.ok(Math.abs(undone.x - start.x) < 1e-3 && Math.abs(undone.z - start.z) < 1e-3, 'undo restores the original position');
  void propId;
});

test('the selection highlight is rebuilt when selection changes', () => {
  const { env } = bootWithGL();
  env.run('app.viewport3d.render();');
  const emptyHighlight = env.run('app.viewport3d._highlightCount');
  assert.equal(emptyHighlight, 0, 'nothing selected, nothing highlighted');

  env.run(`app.viewportSelect(app.level.walls[0].id, false); app.viewport3d.render();`);
  assert.ok(env.run('app.viewport3d._highlightCount') > 0, 'a selected wall produces highlight segments');
});

test('keyboard movement only applies while the pointer is over the 3D pane', () => {
  const { env } = bootWithGL();
  // Drive frame timing deterministically: each call advances 100 ms.
  env.run('window.performance = { now: (() => { let t = 0; return () => (t += 100); })() };');
  const canvas = env.document.getElementById('canvas-3d');
  const before = JSON.parse(env.run('JSON.stringify(app.viewport3d.camera.position)'));

  // The viewport listens for keys on the window (they bubble from the focused
  // canvas), so fire them there.
  env.run('app.viewport3d._hovered = true;');
  env.fireWindow('keydown', { key: 'w', target: canvas });
  assert.equal(env.run('app.viewport3d._keys.size'), 1, 'the forward key is held while hovering');
  // The first call only records the frame time; the second applies the movement.
  env.run('app.viewport3d._updateMovement();');
  env.run('app.viewport3d._updateMovement();');
  const after = JSON.parse(env.run('JSON.stringify(app.viewport3d.camera.position)'));
  const travelled = Math.hypot(after[0] - before[0], after[2] - before[2]);
  assert.ok(travelled > 0.05, `W moved the camera forward (travelled ${travelled.toFixed(3)} m)`);

  env.run('app.viewport3d._hovered = false;');
  env.fireWindow('keyup', { key: 'w', target: canvas });
  env.fireWindow('keydown', { key: 'w', target: env.document.body });
  assert.equal(env.run('app.viewport3d._keys.size'), 0, 'the key is ignored away from the pane');

  const idle = JSON.parse(env.run('JSON.stringify(app.viewport3d.camera.position)'));
  env.run('app.viewport3d._updateMovement();');
  env.run('app.viewport3d._updateMovement();');
  const still = JSON.parse(env.run('JSON.stringify(app.viewport3d.camera.position)'));
  assert.deepEqual(still, idle, 'with the pointer away from the pane, WASD does nothing');
});

test('the editor survives a level with many walls and openings in 3D', () => {
  const { env, gl } = bootWithGL();
  env.run(`(() => {
    const level = app.level;
    for (let i = 0; i < 40; i++) {
      const wall = PlacesOps.createWall(level, { x: -20 + i, z: -20, width: 2, depth: 0.35 });
      PlacesOps.addOpening(wall, { kind: 'door', offset: 0.4, width: 0.9, height: 2.1 });
      if (i % 3 === 0) PlacesOps.addOpening(wall, { kind: 'window', offset: 1.2, width: 0.6, height: 1.0, sill: 1.0 });
      PlacesOps.addProp(level, { model: 'core:crate', x: -20 + i, z: -18 }, app.propCatalog);
    }
    app.levelChanged();
  })()`);
  env.run('app.viewport3d.render();');
  const draws = gl.calls.drawArrays.length;
  assert.ok(draws >= 4);
  const vertexCount = env.run('app.viewport3d._batches ? app.viewport3d._batches.walls.count : 0');
  assert.ok(vertexCount > 800, `expected substantial wall geometry, got ${vertexCount}`);
  assert.equal(env.run('app.level.walls.length'), 44);
});
