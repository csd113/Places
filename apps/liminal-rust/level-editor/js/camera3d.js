// camera3d.js - Pure, DOM-free camera math for the 3D viewport.
// Owns the yaw/pitch fly camera, the WebGL view/projection matrices and the
// screen-ray helpers used by viewport3d.js. No DOM access, no dependencies, so
// the tests can exercise it directly under node.
//
// Conventions (identical to the game's renderer in src/render.rs):
//   * Y is up, yaw 0 looks down -Z, yaw/pitch are radians.
//   * forward = (sin(yaw)*cos(pitch), sin(pitch), -cos(yaw)*cos(pitch)).
//   * Matrices are Float32Array(16) in column-major (WebGL) order.
//   * Screen pixels are CSS pixels with (0, 0) at the top-left.
//   * Vector helpers take an optional `out` array; pass one to avoid allocating.
//
// Only the `LiminalCamera3D` namespace is attached to the window: the vector
// and matrix helpers have generic names that must not shadow app globals.

(function (root, factory) {
  const api = factory();
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.LiminalCamera3D = api;
  }
})(typeof window !== 'undefined' ? window : null, function () {
  'use strict';

  const DEG = Math.PI / 180;
  const WORLD_UP = [0, 1, 0];
  // Near-vertical pitch keeps the look-at basis stable; the viewport reports
  // and clamps the same limit through `look()`.
  const MAX_PITCH = 89 * DEG;
  const DEFAULT_BOUNDS = { center: [0, 1.75, 0], radius: 12 };
  const FALLBACK_X = [1, 0, 0];
  const FALLBACK_Z = [0, 0, 1];

  function num(value, fallback) {
    const n = Number(value);
    return Number.isFinite(n) ? n : fallback;
  }

  function clamp(value, min, max) {
    return value < min ? min : value > max ? max : value;
  }

  /** Keeps an angle in [-PI, PI] so long mouse drags cannot drift to Infinity. */
  function wrapAngle(angle) {
    if (!Number.isFinite(angle)) return 0;
    const twoPi = Math.PI * 2;
    let a = angle % twoPi;
    if (a > Math.PI) a -= twoPi;
    if (a < -Math.PI) a += twoPi;
    return a;
  }

  // -------------------------------------------------------------- vector math

  /** Unit vector copy of `v` into `out` (returns out). */
  function normalize(v, out) {
    out = out || [0, 0, 0];
    const len = Math.sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if (!(len > 1e-12)) {
      out[0] = 0;
      out[1] = 0;
      out[2] = 0;
      return out;
    }
    out[0] = v[0] / len;
    out[1] = v[1] / len;
    out[2] = v[2] / len;
    return out;
  }

  function cross(a, b, out) {
    out = out || [0, 0, 0];
    const x = a[1] * b[2] - a[2] * b[1];
    const y = a[2] * b[0] - a[0] * b[2];
    const z = a[0] * b[1] - a[1] * b[0];
    out[0] = x;
    out[1] = y;
    out[2] = z;
    return out;
  }

  function subtract(a, b, out) {
    out = out || [0, 0, 0];
    out[0] = a[0] - b[0];
    out[1] = a[1] - b[1];
    out[2] = a[2] - b[2];
    return out;
  }

  function add(a, b, out) {
    out = out || [0, 0, 0];
    out[0] = a[0] + b[0];
    out[1] = a[1] + b[1];
    out[2] = a[2] + b[2];
    return out;
  }

  function scale(v, factor, out) {
    out = out || [0, 0, 0];
    out[0] = v[0] * factor;
    out[1] = v[1] * factor;
    out[2] = v[2] * factor;
    return out;
  }

  function dot(a, b) {
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  }

  function length(v) {
    return Math.sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
  }

  // ------------------------------------------------------------ matrix helpers

  function mat4Identity(out) {
    out = out || new Float32Array(16);
    out[0] = 1; out[1] = 0; out[2] = 0; out[3] = 0;
    out[4] = 0; out[5] = 1; out[6] = 0; out[7] = 0;
    out[8] = 0; out[9] = 0; out[10] = 1; out[11] = 0;
    out[12] = 0; out[13] = 0; out[14] = 0; out[15] = 1;
    return out;
  }

  /** Right-handed OpenGL perspective with a [-1, 1] depth range. */
  function mat4Perspective(fovYRadians, aspect, near, far, out) {
    out = out || new Float32Array(16);
    const fov = num(fovYRadians, 60 * DEG);
    const a = num(aspect, 1) > 0 ? num(aspect, 1) : 1;
    const n = Math.max(num(near, 0.1), 1e-4);
    const f = Math.max(num(far, 500), n + 1e-4);
    const focal = 1 / Math.tan(fov / 2);
    const invRange = 1 / (n - f);
    out[0] = focal / a; out[1] = 0; out[2] = 0; out[3] = 0;
    out[4] = 0; out[5] = focal; out[6] = 0; out[7] = 0;
    out[8] = 0; out[9] = 0; out[10] = (f + n) * invRange; out[11] = -1;
    out[12] = 0; out[13] = 0; out[14] = 2 * f * n * invRange; out[15] = 0;
    return out;
  }

  const _lookBack = [0, 0, 0];
  const _lookRight = [0, 0, 0];
  const _lookUp = [0, 0, 0];

  /** Right-handed look-at view matrix. */
  function mat4LookAt(eye, target, up, out) {
    out = out || new Float32Array(16);
    subtract(eye, target, _lookBack); // camera backward = +Z in view space
    if (!(length(_lookBack) > 1e-9)) {
      _lookBack[0] = 0;
      _lookBack[1] = 0;
      _lookBack[2] = 1;
    }
    normalize(_lookBack, _lookBack);

    const upVector = up && up.length >= 3 ? up : WORLD_UP;
    cross(upVector, _lookBack, _lookRight);
    if (!(length(_lookRight) > 1e-9)) {
      // View direction parallel to up: pick a stable fallback axis.
      cross(FALLBACK_X, _lookBack, _lookRight);
      if (!(length(_lookRight) > 1e-9)) cross(FALLBACK_Z, _lookBack, _lookRight);
    }
    normalize(_lookRight, _lookRight);
    cross(_lookBack, _lookRight, _lookUp);

    out[0] = _lookRight[0]; out[1] = _lookUp[0]; out[2] = _lookBack[0]; out[3] = 0;
    out[4] = _lookRight[1]; out[5] = _lookUp[1]; out[6] = _lookBack[1]; out[7] = 0;
    out[8] = _lookRight[2]; out[9] = _lookUp[2]; out[10] = _lookBack[2]; out[11] = 0;
    out[12] = -dot(_lookRight, eye);
    out[13] = -dot(_lookUp, eye);
    out[14] = -dot(_lookBack, eye);
    out[15] = 1;
    return out;
  }

  // Aliasing-safe (a or b may equal out) at the cost of one module-level temp.
  const _mulTemp = new Float32Array(16);

  /** Matrix product `a * b` (column-major). */
  function mat4Multiply(a, b, out) {
    out = out || new Float32Array(16);
    for (let col = 0; col < 4; col++) {
      for (let row = 0; row < 4; row++) {
        _mulTemp[col * 4 + row] =
          a[row] * b[col * 4] +
          a[4 + row] * b[col * 4 + 1] +
          a[8 + row] * b[col * 4 + 2] +
          a[12 + row] * b[col * 4 + 3];
      }
    }
    out.set(_mulTemp);
    return out;
  }

  // ------------------------------------------------------------- bounds helper

  /**
   * Normalises framing input to {center, radius}. Accepts an AABB
   * ({min, max}), a centre with a half extent ({center, half}), a bounding
   * sphere ({center, radius}) or nothing at all.
   */
  function boundsCenterRadius(bounds) {
    let center = null;
    if (bounds && Array.isArray(bounds.center) && bounds.center.length >= 3) {
      center = [num(bounds.center[0], 0), num(bounds.center[1], 0), num(bounds.center[2], 0)];
    } else if (bounds && Array.isArray(bounds.min) && Array.isArray(bounds.max) &&
               bounds.min.length >= 3 && bounds.max.length >= 3) {
      center = [
        (num(bounds.min[0], 0) + num(bounds.max[0], 0)) / 2,
        (num(bounds.min[1], 0) + num(bounds.max[1], 0)) / 2,
        (num(bounds.min[2], 0) + num(bounds.max[2], 0)) / 2
      ];
    }
    if (!center) center = [0, 0, 0];

    let radius = num(bounds && bounds.radius, 0);
    if (!(radius > 0)) {
      if (bounds && Array.isArray(bounds.half) && bounds.half.length >= 3) {
        radius = length([num(bounds.half[0], 0), num(bounds.half[1], 0), num(bounds.half[2], 0)]);
      } else if (bounds && Array.isArray(bounds.min) && Array.isArray(bounds.max) &&
                 bounds.min.length >= 3 && bounds.max.length >= 3) {
        radius = length([
          (num(bounds.max[0], 0) - num(bounds.min[0], 0)) / 2,
          (num(bounds.max[1], 0) - num(bounds.min[1], 0)) / 2,
          (num(bounds.max[2], 0) - num(bounds.min[2], 0)) / 2
        ]);
      } else {
        radius = 1;
      }
    }
    return { center, radius: Math.max(0, radius) };
  }

  /**
   * Intersects a ray with the horizontal plane y = planeY. Returns the world
   * point in `out`, or null when the ray is parallel or the plane is behind it.
   */
  function rayPlaneY(origin, direction, planeY, out) {
    const dy = num(direction && direction[1], 0);
    if (!(Math.abs(dy) > 1e-6)) return null;
    const oy = num(origin && origin[1], 0);
    const t = (num(planeY, 0) - oy) / dy;
    if (!(t >= 0)) return null;
    out = out || [0, 0, 0];
    out[0] = num(origin && origin[0], 0) + num(direction && direction[0], 0) * t;
    out[1] = num(planeY, 0);
    out[2] = num(origin && origin[2], 0) + num(direction && direction[2], 0) * t;
    return out;
  }

  // ------------------------------------------------------------- Camera3D

  /**
   * Yaw/pitch fly camera mirroring the game's `render_scene` maths.
   * Options: { x, y, z, yaw, pitch (radians), fov (degrees), near, far,
   *            moveSpeed (m/s), lookSpeed (rad/px), panDistance (m) }.
   */
  class Camera3D {
    constructor(options) {
      const opts = options || {};
      this.position = [num(opts.x, 0), num(opts.y, 1.7), num(opts.z, 0)];
      this.yaw = num(opts.yaw, 0);
      this.pitch = clamp(num(opts.pitch, 0), -MAX_PITCH, MAX_PITCH);
      this.fov = num(opts.fov, 65); // vertical, degrees
      this.near = num(opts.near, 0.1);
      this.far = num(opts.far, 500);
      this.moveSpeed = num(opts.moveSpeed, 6);
      this.lookSpeed = num(opts.lookSpeed, 0.0045);
      // Pan/dolly scale: the world distance the camera is framing. focusBounds
      // updates it so wheel and pan feel constant for any level size.
      this.panDistance = num(opts.panDistance, 10);
      // Scratch space so viewProjection()/rayFromScreen() allocate nothing.
      this._forward = [0, 0, 0];
      this._right = [0, 0, 0];
      this._up = [0, 0, 0];
      this._target = [0, 0, 0];
      this._view = new Float32Array(16);
      this._projection = new Float32Array(16);
      this._viewProjection = new Float32Array(16);
    }

    _fovRadians() {
      return this.fov * DEG;
    }

    /** Unit view direction. */
    forward(out) {
      out = out || [0, 0, 0];
      const cosPitch = Math.cos(this.pitch);
      out[0] = Math.sin(this.yaw) * cosPitch;
      out[1] = Math.sin(this.pitch);
      out[2] = -Math.cos(this.yaw) * cosPitch;
      return out;
    }

    /** Unit horizontal right vector (independent of pitch). */
    right(out) {
      out = out || [0, 0, 0];
      out[0] = Math.cos(this.yaw);
      out[1] = 0;
      out[2] = Math.sin(this.yaw);
      return out;
    }

    /** Unit screen-up vector (tilts with pitch). */
    up(out) {
      out = out || [0, 0, 0];
      const sinPitch = Math.sin(this.pitch);
      out[0] = -Math.sin(this.yaw) * sinPitch;
      out[1] = Math.cos(this.pitch);
      out[2] = Math.cos(this.yaw) * sinPitch;
      return out;
    }

    /** projection * view for the given framebuffer aspect ratio. */
    viewProjection(aspect, out) {
      const a = num(aspect, 1) > 0 ? num(aspect, 1) : 1;
      const forward = this.forward(this._forward);
      add(this.position, forward, this._target);
      mat4LookAt(this.position, this._target, WORLD_UP, this._view);
      mat4Perspective(this._fovRadians(), a, this.near, this.far, this._projection);
      return mat4Multiply(this._projection, this._view, out || this._viewProjection);
    }

    /**
     * Ray through a canvas pixel: {origin:[x,y,z], direction:[x,y,z]} with a
     * unit direction. Pass `out` (same shape) to reuse arrays.
     */
    rayFromScreen(px, py, width, height, out) {
      const w = num(width, 1) > 0 ? num(width, 1) : 1;
      const h = num(height, 1) > 0 ? num(height, 1) : 1;
      const ndcX = (num(px, 0) / w) * 2 - 1;
      const ndcY = 1 - (num(py, 0) / h) * 2;
      const tanY = Math.tan(this._fovRadians() / 2);
      const tanX = tanY * (w / h);
      const forward = this.forward(this._forward);
      const right = this.right(this._right);
      const up = this.up(this._up);

      const ray = out || { origin: [0, 0, 0], direction: [0, 0, 0] };
      const origin = ray.origin || (ray.origin = [0, 0, 0]);
      const direction = ray.direction || (ray.direction = [0, 0, 0]);
      direction[0] = forward[0] + right[0] * ndcX * tanX + up[0] * ndcY * tanY;
      direction[1] = forward[1] + right[1] * ndcX * tanX + up[1] * ndcY * tanY;
      direction[2] = forward[2] + right[2] * ndcX * tanX + up[2] * ndcY * tanY;
      normalize(direction, direction);
      origin[0] = this.position[0];
      origin[1] = this.position[1];
      origin[2] = this.position[2];
      return ray;
    }

    /** Mouse-look in pixels: +dx looks right, +dy looks down; pitch clamps at 89. */
    look(dxPixels, dyPixels) {
      const dx = num(dxPixels, 0);
      const dy = num(dyPixels, 0);
      this.yaw = wrapAngle(this.yaw + dx * this.lookSpeed);
      this.pitch = clamp(this.pitch - dy * this.lookSpeed, -MAX_PITCH, MAX_PITCH);
    }

    /** Screen-parallel pan; the pixel scale follows `panDistance`. */
    pan(dxPixels, dyPixels, width, height) {
      const h = num(height, 1) > 0 ? num(height, 1) : 1;
      const perPixel = (2 * Math.tan(this._fovRadians() / 2) * Math.max(0.1, this.panDistance)) / h;
      const dx = num(dxPixels, 0) * perPixel;
      const dy = num(dyPixels, 0) * perPixel;
      const right = this.right(this._right);
      const up = this.up(this._up);
      this.position[0] += up[0] * dy - right[0] * dx;
      this.position[1] += up[1] * dy - right[1] * dx;
      this.position[2] += up[2] * dy - right[2] * dx;
    }

    /** Moves along the view direction (positive = forward / zoom in). */
    dolly(distance) {
      const d = num(distance, 0);
      const forward = this.forward(this._forward);
      this.position[0] += forward[0] * d;
      this.position[1] += forward[1] * d;
      this.position[2] += forward[2] * d;
    }

    /** Moves along the horizontal forward axis; Y is untouched. */
    moveForward(distance) {
      const d = num(distance, 0);
      const forward = this.forward(this._forward);
      const flat = Math.sqrt(forward[0] * forward[0] + forward[2] * forward[2]);
      if (!(flat > 1e-9)) return;
      this.position[0] += (forward[0] / flat) * d;
      this.position[2] += (forward[2] / flat) * d;
    }

    /** Moves along the horizontal right axis; Y is untouched. */
    moveRight(distance) {
      const d = num(distance, 0);
      const right = this.right(this._right);
      this.position[0] += right[0] * d;
      this.position[2] += right[2] * d;
    }

    /** Moves along world +Y. */
    moveUp(distance) {
      this.position[1] += num(distance, 0);
    }

    /**
     * Applies WASD/QE/Space/Ctrl movement for one frame. `keys` is a Set or a
     * plain object of lowercase key names; Shift triples the speed.
     */
    moveWithKeys(keys, dtSeconds) {
      const dt = num(dtSeconds, 0);
      if (!(dt > 0)) return;
      const step = this.moveSpeed * Math.min(dt, 1);
      const speed = keyDown(keys, 'shift') ? step * 3 : step;
      let forward = 0;
      let strafe = 0;
      let lift = 0;
      if (keyDown(keys, 'w')) forward += 1;
      if (keyDown(keys, 's')) forward -= 1;
      if (keyDown(keys, 'd')) strafe += 1;
      if (keyDown(keys, 'a')) strafe -= 1;
      if (keyDown(keys, 'e') || keyDown(keys, ' ') || keyDown(keys, 'space')) lift += 1;
      if (keyDown(keys, 'q') || keyDown(keys, 'control') || keyDown(keys, 'ctrl')) lift -= 1;
      const diagonal = Math.sqrt(forward * forward + strafe * strafe);
      if (diagonal > 0) {
        forward /= diagonal;
        strafe /= diagonal;
      }
      if (forward) this.moveForward(forward * speed);
      if (strafe) this.moveRight(strafe * speed);
      if (lift) this.moveUp(lift * speed);
    }

    /**
     * Points the camera at a bounding volume from its current orientation.
     * `factor` scales the fitted distance (default 1.2). Returns the distance.
     */
    focusBounds(bounds, factor) {
      const info = boundsCenterRadius(bounds);
      const radius = Math.max(info.radius, 0.25);
      const fit = radius / Math.max(Math.tan(this._fovRadians() / 2), 1e-3);
      const distance = Math.max(0.5, fit * Math.max(0.1, num(factor, 1.2)));
      const forward = this.forward(this._forward);
      this.position[0] = info.center[0] - forward[0] * distance;
      this.position[1] = info.center[1] - forward[1] * distance;
      this.position[2] = info.center[2] - forward[2] * distance;
      this.panDistance = distance;
      return distance;
    }

    /** Default three-quarter view of `bounds` (or a fallback room-sized box). */
    reset(bounds) {
      this.yaw = 45 * DEG;
      this.pitch = -22 * DEG;
      return this.focusBounds(bounds || DEFAULT_BOUNDS, 1.35);
    }
  }

  /** True when a Set or object keys-map reports `name` as held. */
  function keyDown(keys, name) {
    if (!keys) return false;
    if (typeof keys.has === 'function') return keys.has(name);
    return keys[name] === true;
  }

  return {
    DEG,
    MAX_PITCH,
    Camera3D,
    normalize,
    cross,
    subtract,
    add,
    scale,
    dot,
    length,
    clamp,
    wrapAngle,
    mat4Identity,
    mat4Perspective,
    mat4LookAt,
    mat4Multiply,
    boundsCenterRadius,
    rayPlaneY,
    keyDown
  };
});
