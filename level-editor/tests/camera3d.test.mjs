// camera3d.test.mjs - Camera maths: matrices, screen rays, movement, framing.
// These lock the conventions the 3D viewport and the game's renderer share
// (Y up, yaw 0 looks down -Z, column-major matrices, CSS-pixel screen rays).
import test from 'node:test';
import assert from 'node:assert/strict';
import camera from '../js/camera3d.js';

const {
  Camera3D,
  MAX_PITCH,
  mat4Identity,
  mat4Perspective,
  mat4LookAt,
  mat4Multiply,
  rayPlaneY
} = camera;

const DEG = Math.PI / 180;

/** Applies a column-major matrix to a position and divides by w. */
function project(matrix, point) {
  const x = matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12];
  const y = matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13];
  const z = matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14];
  const w = matrix[3] * point[0] + matrix[7] * point[1] + matrix[11] * point[2] + matrix[15];
  return [x / w, y / w, z / w];
}

function assertClose(actual, expected, tolerance = 1e-6, message = '') {
  assert.ok(Math.abs(actual - expected) <= tolerance, `${message} expected ${expected}, got ${actual}`);
}

function assertVectorClose(actual, expected, tolerance = 1e-6) {
  for (let i = 0; i < 3; i++) assertClose(actual[i], expected[i], tolerance, `component ${i}`);
}

function assertMatrixClose(actual, expected, tolerance = 1e-6) {
  assert.equal(actual.length, 16);
  for (let i = 0; i < 16; i++) assertClose(actual[i], expected[i], tolerance, `element ${i}`);
}

function vecLength(v) {
  return Math.sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
}

test('perspective matrix is finite, right-handed and maps near/far to the NDC edges', () => {
  const near = 0.1;
  const far = 100;
  const matrix = mat4Perspective(60 * DEG, 16 / 9, near, far);
  assert.equal(matrix.length, 16);
  for (let i = 0; i < 16; i++) assert.ok(Number.isFinite(matrix[i]), `element ${i} is finite`);
  assert.notEqual(matrix[5], 0);
  assert.notEqual(matrix[0], 0);
  // The w row is the perspective divide: -z_camera, so w == 1 for the eye itself.
  assert.equal(matrix[3], 0);
  assert.equal(matrix[7], 0);
  assert.equal(matrix[11], -1);
  assert.equal(matrix[15], 0);
  // x and y scale by 1/aspect in a column-major matrix.
  assertClose(matrix[0] / matrix[5], 9 / 16);
  // Camera looks down -Z: the near plane lands on ndc z = -1, the far on +1.
  assertClose(project(matrix, [0, 0, -near])[2], -1, 1e-4, 'near plane');
  assertClose(project(matrix, [0, 0, -far])[2], 1, 1e-4, 'far plane');
  // A point on the view axis projects to the centre of the screen.
  assertClose(project(matrix, [0, 0, -10])[0], 0, 1e-6, 'x');
  assertClose(project(matrix, [0, 0, -10])[1], 0, 1e-6, 'y');
});

test('lookAt from a known pose maps the eye to the origin and the target down -Z', () => {
  const eye = [0, 0, 5];
  const view = mat4LookAt(eye, [0, 0, 0], [0, 1, 0]);
  assertVectorClose(project(view, eye), [0, 0, 0], 1e-6);
  assertVectorClose(project(view, [0, 0, 0]), [0, 0, -5], 1e-6);
  // Screen right stays world +X for this pose.
  assertVectorClose(project(view, [1, 0, 5]), [1, 0, 0], 1e-6);
  // ... and up stays world +Y.
  assertVectorClose(project(view, [0, 1, 5]), [0, 1, 0], 1e-6);
});

test('mat4Multiply is identity-compatible and matches its operands', () => {
  const identity = mat4Identity();
  const perspective = mat4Perspective(50 * DEG, 1.5, 0.1, 50);
  assertMatrixClose(mat4Multiply(identity, perspective), perspective);
  assertMatrixClose(mat4Multiply(perspective, identity), perspective);
  const look = mat4LookAt([3, 2, 4], [0, 1, 0], [0, 1, 0]);
  const viewProjection = mat4Multiply(perspective, look);
  for (let i = 0; i < 16; i++) assert.ok(Number.isFinite(viewProjection[i]));
});

test('screen-centre ray matches the camera forward direction', () => {
  const cam = new Camera3D({ x: 3, y: 2, z: -5, yaw: 0.7, pitch: -0.3 });
  const ray = cam.rayFromScreen(400, 300, 800, 600);
  assert.deepEqual(ray.origin, [3, 2, -5]);
  assertVectorClose(ray.direction, cam.forward(), 1e-9);
  assert.equal(ray.direction.length, 3);
});

test('ray directions are unit length in every screen corner', () => {
  const cam = new Camera3D({ yaw: -1.1, pitch: 0.4, fov: 75 });
  const pixels = [[0, 0], [799, 0], [0, 599], [799, 599], [123, 456], [800, 600]];
  for (const [px, py] of pixels) {
    const ray = cam.rayFromScreen(px, py, 800, 600);
    assertClose(vecLength(ray.direction), 1, 1e-9, `unit ray at ${px},${py}`);
  }
  // Corner rays must actually fan out from the forward direction.
  const corner = cam.rayFromScreen(0, 0, 800, 600);
  assert.ok(corner.direction[0] !== cam.forward()[0] || corner.direction[1] !== cam.forward()[1]);
});

test('a point on the forward axis projects inside the frustum near NDC (0,0)', () => {
  const cam = new Camera3D({ x: -2, y: 1.5, z: 4, yaw: 2.2, pitch: -0.6 });
  const forward = cam.forward();
  const target = [
    cam.position[0] + forward[0] * 10,
    cam.position[1] + forward[1] * 10,
    cam.position[2] + forward[2] * 10
  ];
  const viewProjection = cam.viewProjection(16 / 9, new Float32Array(16));
  const ndc = project(viewProjection, target);
  assertClose(ndc[0], 0, 1e-4, 'ndc x');
  assertClose(ndc[1], 0, 1e-4, 'ndc y');
  assert.ok(ndc[2] > -1 && ndc[2] < 1, `frustum depth ${ndc[2]} is inside the clip range`);
});

test('look() clamps pitch to +/-89 degrees and keeps yaw wrapped', () => {
  const cam = new Camera3D({});
  cam.look(0, 100000);
  assertClose(cam.pitch, -MAX_PITCH, 1e-9, 'down clamp');
  cam.look(0, -100000);
  assertClose(cam.pitch, MAX_PITCH, 1e-9, 'up clamp');
  cam.look(1234, 5);
  assert.ok(Math.abs(cam.pitch) <= MAX_PITCH + 1e-9);
  assert.ok(cam.yaw >= -Math.PI && cam.yaw <= Math.PI, 'yaw stays wrapped');
  assert.ok(Number.isFinite(cam.yaw));
});

test('moveWithKeys maps W/A/S/D to the horizontal axes and leaves Y untouched', () => {
  const cam = new Camera3D({ x: 0, y: 5, z: 0, yaw: 0, pitch: 0.5, moveSpeed: 4 });
  cam.moveWithKeys({ w: true }, 0.5); // forward is horizontal -Z
  assert.deepEqual(cam.position, [0, 5, -2]);
  cam.moveWithKeys({ a: true }, 0.5); // left is world -X at yaw 0
  assert.deepEqual(cam.position, [-2, 5, -2]);
  cam.moveWithKeys({ s: true }, 0.5);
  assert.deepEqual(cam.position, [-2, 5, 0]);
  cam.moveWithKeys({ d: true }, 0.5);
  assert.deepEqual(cam.position, [0, 5, 0]);
  cam.moveWithKeys({ q: true }, 0.25); // Ctrl/Q drop the eye
  assert.deepEqual(cam.position, [0, 4, 0]);
  cam.moveWithKeys(new Set(['e']), 0.25);
  assert.deepEqual(cam.position, [0, 5, 0]);
});

test('moveWithKeys honours shift speed, diagonals and dt guards', () => {
  const cam = new Camera3D({ x: 0, y: 2, z: 0, yaw: 0, pitch: 0, moveSpeed: 4 });
  cam.moveWithKeys({ w: true, shift: true }, 0.5); // 3x speed => 6 m
  assertClose(cam.position[2], -6, 1e-9, 'shift speed');
  cam.moveWithKeys({ w: true }, 0); // zero dt is a no-op
  cam.moveWithKeys({ w: true }, -1);
  assertClose(cam.position[2], -6, 1e-9, 'guarded dt');
  cam.moveWithKeys({ w: true, d: true }, 1); // diagonals are normalised
  const step = 4 / Math.SQRT2;
  assertClose(cam.position[2], -6 - step, 1e-9, 'diagonal z');
  assertClose(cam.position[0], step, 1e-9, 'diagonal x');
  assertClose(cam.position[1], 2, 1e-9, 'y untouched');
});

test('focusBounds centres the box and keeps a sensible distance', () => {
  const cam = new Camera3D({ yaw: 0.6, pitch: -0.4 });
  const centre = [0, 1.5, 0];
  const distance = cam.focusBounds({ min: [-2, 0, -1], max: [2, 3, 1] }, 1.2);
  assert.ok(Number.isFinite(distance) && distance > 2.5 && distance < 20, `distance ${distance}`);
  const eyeDistance = Math.hypot(
    cam.position[0] - centre[0],
    cam.position[1] - centre[1],
    cam.position[2] - centre[2]
  );
  assertClose(eyeDistance, distance, 1e-6, 'eye sits at the reported distance');
  // The box centre is exactly on the view axis, so it projects to NDC (0,0).
  const ndc = project(cam.viewProjection(16 / 9, new Float32Array(16)), centre);
  assertClose(ndc[0], 0, 1e-4, 'centred x');
  assertClose(ndc[1], 0, 1e-4, 'centred y');
  // The eye is above the target for a downward pitch.
  assert.ok(cam.position[1] > centre[1]);
});

test('focusBounds also accepts centre/half, centre/radius and empty bounds', () => {
  const cam = new Camera3D({ yaw: -2, pitch: -0.9 });
  const fromHalf = cam.focusBounds({ center: [1, 2, 3], half: [1, 1, 1] }, 1);
  assertClose(Math.hypot(1 / Math.sqrt(3), 1 / Math.sqrt(3), 1 / Math.sqrt(3)), 1, 1e-9, 'sanity');
  assert.ok(Number.isFinite(fromHalf));
  for (const v of cam.position) assert.ok(Number.isFinite(v));
  const fromRadius = cam.focusBounds({ center: [0, 0, 0], radius: 3 }, 1);
  assert.ok(fromRadius > 3);
  const empty = cam.focusBounds(null, 1);
  assert.ok(Number.isFinite(empty) && empty >= 0.5);
});

test('reset() yields a finite three-quarter pose above the level', () => {
  const cam = new Camera3D({ x: 99, y: -40, z: 12, yaw: 9, pitch: 3 });
  cam.reset();
  for (const v of cam.position) assert.ok(Number.isFinite(v), `position ${v}`);
  assert.ok(Number.isFinite(cam.yaw) && Number.isFinite(cam.pitch));
  assert.ok(Math.abs(cam.pitch) <= MAX_PITCH + 1e-9);
  const bounds = { min: [-10, 0, -10], max: [10, 4, 10] };
  cam.reset(bounds);
  for (const v of cam.position) assert.ok(Number.isFinite(v));
  const viewProjection = cam.viewProjection(1.5, new Float32Array(16));
  for (let i = 0; i < 16; i++) assert.ok(Number.isFinite(viewProjection[i]), `vp element ${i}`);
  const ndc = project(viewProjection, [0, 2, 0]);
  assert.ok(Math.abs(ndc[0]) < 1e-4 && Math.abs(ndc[1]) < 1e-4, 'level centre is framed');
  assert.ok(cam.position[1] > 4, 'camera ends up above the level');
});

test('rayPlaneY intersects the plane and rejects parallel or backwards rays', () => {
  assert.deepEqual(rayPlaneY([0, 2, 0], [0, -1, 0], 0), [0, 0, 0]);
  const diagonal = rayPlaneY([0, 4, 0], [1, -2, 0], 0);
  assertVectorClose(diagonal, [2, 0, 0], 1e-9);
  assert.equal(rayPlaneY([0, 2, 0], [0, 0, 1], 0), null, 'parallel ray');
  assert.equal(rayPlaneY([0, 2, 0], [0, 1, 0], 0), null, 'plane behind the ray');
});
