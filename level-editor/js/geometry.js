// geometry.js - Pure level geometry: wall openings, collision boxes, 2D projections
// and the 3D mesh spec shared by the 2D canvas renderer, the 3D viewport and the tests.
//
// This module is deliberately DOM-free and framework-free: it is the single source of
// truth for "what shape is this level", mirroring `src/level.rs::wall_solid_slices` and
// `src/render.rs::build_level_geometry` in the game.
//
// Wall space convention (identical to the game):
//   * A wall is an axis-aligned box. Its *length* runs along X when width >= depth,
//     otherwise along Z. The other dimension is its *thickness*.
//   * Openings (doors/windows) are described in wall-local coordinates:
//     `offset` is measured along the length axis from the wall's minimum corner,
//     `sill` is the opening's bottom edge height above the wall base, and openings
//     always cut through the full thickness of the wall.
//   * Holes, overlaps and deliberate clipping between walls/rooms/props are allowed.

(function (root, factory) {
  const deps = (typeof module !== 'undefined' && module.exports)
    ? { lighting: require('./lighting.js') }
    : { lighting: root ? root.PlacesLighting : null };
  const api = factory(deps.lighting);
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.PlacesGeometry = api;
    for (const key of Object.keys(api)) root[key] = api[key];
  }
})(typeof window !== 'undefined' ? window : null, function (lighting) {
  'use strict';

  const EPS = 1e-6;
  const COLOR = {
    wall: [0.78, 0.72, 0.55],
    wallSide: [0.66, 0.60, 0.45],
    jamb: [0.55, 0.50, 0.38],
    head: [0.72, 0.66, 0.50],
    floor: [0.62, 0.57, 0.46],
    ceiling: [0.86, 0.86, 0.83],
    spawn: [0.30, 0.85, 0.45],
    prop: [0.70, 0.68, 0.64]
  };

  const OPENING_KINDS = {
    door: { label: 'Door', sill: 0.0, height: 2.1, width: 1.0, color: [0.95, 0.72, 0.30] },
    window: { label: 'Window', sill: 1.0, height: 1.2, width: 1.4, color: [0.42, 0.75, 0.95] },
    passage: { label: 'Passage', sill: 0.0, height: 2.4, width: 2.0, color: [0.80, 0.80, 0.80] },
    vent: { label: 'Vent', sill: 2.0, height: 0.5, width: 0.8, color: [0.70, 0.70, 0.70] }
  };
  const OPENING_TYPES = ['door', 'window', 'passage', 'vent'];

  function isNumber(v) {
    return typeof v === 'number' && Number.isFinite(v);
  }

  // ---------------------------------------------------------------- wall basics

  function wallAxis(wall) {
    const w = Math.abs(Number(wall.width) || 0);
    const d = Math.abs(Number(wall.depth) || 0);
    return w >= d ? 'x' : 'z';
  }

  function wallLength(wall) {
    return wallAxis(wall) === 'x' ? Math.abs(Number(wall.width) || 0) : Math.abs(Number(wall.depth) || 0);
  }

  function wallThickness(wall) {
    return wallAxis(wall) === 'x' ? Math.abs(Number(wall.depth) || 0) : Math.abs(Number(wall.width) || 0);
  }

  function wallMinCorner(wall) {
    const x = Number(wall.x) || 0;
    const z = Number(wall.z) || 0;
    return { x: Math.min(x, x + (Number(wall.width) || 0)), z: Math.min(z, z + (Number(wall.depth) || 0)) };
  }

  function wallBaseY(wall) {
    return Number(wall.y) || 0;
  }

  function wallResolvedHeight(wall, defaultCeiling) {
    const h = wall.height;
    if (h === null || h === undefined) return isNumber(defaultCeiling) ? defaultCeiling : 3.5;
    return Number(h) || 0;
  }

  /** World point for wall-local (offset along length, offset across thickness). */
  function wallLocalToWorld(wall, u, v) {
    const base = wallMinCorner(wall);
    if (wallAxis(wall) === 'x') return { x: base.x + u, z: base.z + v };
    return { x: base.x + v, z: base.z + u };
  }

  /** Projects a world point onto the wall's length axis, clamped to the wall. */
  function wallProjectOffset(wall, world) {
    const base = wallMinCorner(wall);
    const u = wallAxis(wall) === 'x' ? world.x - base.x : world.z - base.z;
    return Math.max(0, Math.min(wallLength(wall), u));
  }

  /** Distance from a world point to the wall's centreline (used for hit testing). */
  function wallDistanceToPoint(wall, world) {
    const base = wallMinCorner(wall);
    const u = wallAxis(wall) === 'x' ? world.x - base.x : world.z - base.z;
    const v = wallAxis(wall) === 'x' ? world.z - base.z : world.x - base.x;
    const du = Math.max(0, Math.max(-u, u - wallLength(wall)));
    const dv = Math.max(0, Math.max(-v, v - wallThickness(wall)));
    return Math.hypot(du, dv);
  }

  // ------------------------------------------------------------- wall openings

  /** Normalised opening in wall-local coordinates (clamped, y-overlap guaranteed). */
  function normalizeOpening(wall, opening, defaultCeiling) {
    const length = wallLength(wall);
    const baseY = wallBaseY(wall);
    const top = baseY + wallResolvedHeight(wall, defaultCeiling);
    const kind = String((opening && opening.kind) || 'door');

    let offset = Number(opening && opening.offset);
    let width = Number(opening && opening.width);
    let height = Number(opening && opening.height);
    let sill = Number((opening && opening.sill) || 0);
    if (!isNumber(offset)) offset = 0;
    if (!isNumber(width)) width = 0;
    if (!isNumber(height)) height = 0;
    if (!isNumber(sill) || sill < 0) sill = 0;

    const start = Math.max(0, Math.min(length, offset));
    const end = Math.max(start, Math.min(length, offset + Math.max(0, width)));
    const bottom = Math.min(top, baseY + sill);
    const upper = Math.min(top, baseY + sill + Math.max(0, height));
    if (end - start <= EPS || upper - bottom <= EPS) return null;
    return { kind, start, end, width: end - start, bottom, top: upper, sill: bottom - baseY, height: upper - bottom };
  }

  function openingsOf(wall) {
    return Array.isArray(wall && wall.openings) ? wall.openings : [];
  }

  function normalizeOpenings(wall, defaultCeiling) {
    const list = [];
    for (const raw of openingsOf(wall)) {
      const n = normalizeOpening(wall, raw, defaultCeiling);
      if (n) list.push(n);
    }
    list.sort((a, b) => a.start - b.start);
    return list;
  }

  /**
   * Splits a wall into solid vertical slices with its openings removed.
   * Mirrors `wall_solid_slices` in src/level.rs: results are ordered by `start`,
   * adjacent slices with identical vertical ranges are merged, and fully open
   * segments are dropped.
   */
  function wallSolidSlices(wall, defaultCeiling) {
    const length = wallLength(wall);
    const baseY = wallBaseY(wall);
    const height = wallResolvedHeight(wall, defaultCeiling);
    if (!(length > EPS) || !(height > EPS)) return [];
    const top = baseY + height;
    const openings = normalizeOpenings(wall, defaultCeiling);

    const bounds = [0, length];
    for (const o of openings) bounds.push(o.start, o.end);
    const unique = Array.from(new Set(bounds.map((b) => Number(b.toFixed(6))))).sort((a, b) => a - b);

    const slices = [];
    for (let i = 0; i < unique.length - 1; i++) {
      const start = unique[i];
      const end = unique[i + 1];
      if (end - start <= EPS) continue;

      // Openings fully covering this segment define the vertical holes.
      const holes = [];
      for (const o of openings) {
        if (o.start <= start + 1e-4 && o.end >= end - 1e-4) holes.push([o.bottom, o.top]);
      }
      holes.sort((a, b) => a[0] - b[0]);
      const merged = [];
      for (const hole of holes) {
        const last = merged[merged.length - 1];
        if (last && hole[0] <= last[1] + 1e-4) last[1] = Math.max(last[1], hole[1]);
        else merged.push([hole[0], hole[1]]);
      }

      let cursor = baseY;
      const ranges = [];
      for (const [hStart, hEnd] of merged) {
        if (hStart - cursor > 1e-4) ranges.push([cursor, Math.min(hStart, top)]);
        cursor = Math.max(cursor, hEnd);
      }
      if (top - cursor > 1e-4) ranges.push([cursor, top]);

      for (const [bottom, upper] of ranges) {
        const prev = slices[slices.length - 1];
        if (prev && Math.abs(prev.end - start) <= 1e-4 &&
            Math.abs(prev.bottom - bottom) <= 1e-4 && Math.abs(prev.top - upper) <= 1e-4) {
          prev.end = end;
        } else {
          slices.push({ start, end, bottom, top: upper });
        }
      }
    }
    return slices;
  }

  /** Horizontal XZ rectangles of the wall's solid material (openings appear as gaps). */
  function wallSolidRects2D(wall, defaultCeiling) {
    const slices = wallSolidSlices(wall, defaultCeiling);
    const thickness = wallThickness(wall);
    return slices.map((s) => {
      const a = wallLocalToWorld(wall, s.start, 0);
      return {
        x: a.x,
        z: a.z,
        width: wallAxis(wall) === 'x' ? s.end - s.start : thickness,
        depth: wallAxis(wall) === 'x' ? thickness : s.end - s.start,
        slice: s
      };
    });
  }

  /** Axis-aligned collision boxes ({minX,maxX,minY,maxY,minZ,maxZ}) for a wall. */
  function wallCollisionBoxes(wall, defaultCeiling) {
    const slices = wallSolidSlices(wall, defaultCeiling);
    const thickness = wallThickness(wall);
    return slices.map((s) => {
      const a = wallLocalToWorld(wall, s.start, 0);
      const length = s.end - s.start;
      const width = wallAxis(wall) === 'x' ? length : thickness;
      const depth = wallAxis(wall) === 'x' ? thickness : length;
      return {
        minX: a.x,
        maxX: a.x + width,
        minY: s.bottom,
        maxY: s.top,
        minZ: a.z,
        maxZ: a.z + depth
      };
    });
  }

  // ------------------------------------------------------------------ props

  const PROP_FALLBACK_SIZE = [0.6, 0.9, 0.6];
  const PROP_FALLBACK_COLOR = [0.54, 0.53, 0.5];

  // Per-face shading multipliers for a prop box in its local space, mirroring
  // `PROP_FACE_SHADES` in src/render.rs: top, bottom, +Z, -Z, -X, +X.
  const PROP_BOX_SHADES = [1.0, 0.62, 0.9, 0.8, 0.74, 0.86];

  function propSize(prop, catalog) {
    const entry = catalog && typeof catalog.get === 'function' ? catalog.get(prop.model) : null;
    let size = Array.isArray(prop.size) && prop.size.length === 3 ? prop.size.slice() : (entry && entry.size) || PROP_FALLBACK_SIZE;
    size = size.map((v) => (isNumber(v) && v > 0 ? v : 0));
    if (size.some((v) => v <= 0)) size = PROP_FALLBACK_SIZE.slice();
    const scale = isNumber(Number(prop.scale)) && Number(prop.scale) > 0 ? Number(prop.scale) : 1;
    return size.map((v) => v * scale);
  }

  /** World-space AABB of a prop (rotation-aware on the XZ plane). */
  function propBounds(prop, catalog) {
    const size = propSize(prop, catalog);
    const [w, h, d] = size;
    const rot = ((Number(prop.rotation_degrees) || 0) * Math.PI) / 180;
    const cos = Math.abs(Math.cos(rot));
    const sin = Math.abs(Math.sin(rot));
    const halfX = (w * cos + d * sin) / 2;
    const halfZ = (w * sin + d * cos) / 2;
    const x = Number(prop.x) || 0;
    const y = Number(prop.y) || 0;
    const z = Number(prop.z) || 0;
    return {
      minX: x - halfX, maxX: x + halfX,
      minY: y, maxY: y + h,
      minZ: z - halfZ, maxZ: z + halfZ
    };
  }

  // --------------------------------------------------------- object bounds (3D)

  /** Oriented box bounds for any selectable object, used for picking + highlights. */
  function objectBounds3D(level, id, options) {
    const catalog = options && options.catalog;
    const defaultCeiling = level && typeof level.getCeilingHeight === 'function' ? level.getCeilingHeight() : 3.5;

    const wall = findWall(level, id);
    if (wall) {
      const base = wallMinCorner(wall);
      const length = Math.max(wallLength(wall), 0.04);
      const thickness = Math.max(wallThickness(wall), 0.04);
      const height = Math.max(wallResolvedHeight(wall, defaultCeiling), 0.04);
      const center = wallAxis(wall) === 'x'
        ? { x: base.x + length / 2, z: base.z + thickness / 2 }
        : { x: base.x + thickness / 2, z: base.z + length / 2 };
      return {
        kind: 'wall',
        center: [center.x, wallBaseY(wall) + height / 2, center.z],
        half: [ (wallAxis(wall) === 'x' ? length : thickness) / 2, height / 2, (wallAxis(wall) === 'x' ? thickness : length) / 2 ],
        rotationY: 0
      };
    }

    const light = findLight(level, id);
    if (light) {
      // Same rule as the game and lighting.js, so the picked bounds, the drawn
      // panel and its light pool always agree.
      const [halfW, halfD] = lighting.fixtureHalfExtents(light.rotation_degrees);
      const y = (level && level.getCeilingHeight ? level.getCeilingHeight(light.x, light.z) : 3.5) - 0.06;
      return { kind: 'light', center: [light.x, y, light.z], half: [halfW, 0.06, halfD], rotationY: 0 };
    }

    const room = findRoom(level, id);
    if (room) {
      const min = { x: Math.min(room.x, room.x + room.width), z: Math.min(room.z, room.z + room.depth) };
      return {
        kind: 'room',
        center: [min.x + room.width / 2, room.height / 2, min.z + room.depth / 2],
        half: [Math.abs(room.width) / 2, room.height / 2, Math.abs(room.depth) / 2],
        rotationY: 0
      };
    }

    const prop = findProp(level, id);
    if (prop) {
      const size = propSize(prop, catalog);
      return {
        kind: 'prop',
        center: [prop.x, (Number(prop.y) || 0) + size[1] / 2, prop.z],
        half: [size[0] / 2, size[1] / 2, size[2] / 2],
        rotationY: Number(prop.rotation_degrees) || 0,
        prop
      };
    }

    if (id === 'spawn' && level && level.spawn) {
      return { kind: 'spawn', center: [level.spawn.x, 0.9, level.spawn.z], half: [0.45, 0.9, 0.45], rotationY: 0 };
    }
    return null;
  }

  function findWall(level, id) { return level && level.walls.find((w) => w.id === id); }
  function findLight(level, id) { return level && level.ceiling_lights.find((l) => l.id === id); }
  function findRoom(level, id) { return level && level.rooms.find((r) => r.id === id); }
  function findProp(level, id) { return level && Array.isArray(level.props) && level.props.find((p) => p.id === id); }
  function findOpening(level, id) {
    if (!level || !level.walls) return null;
    for (const wall of level.walls) {
      for (const opening of openingsOf(wall)) {
        if (opening.id === id) return { wall, opening };
      }
    }
    return null;
  }

  // ------------------------------------------------------------------ picking

  function rayBoxIntersection(origin, direction, center, half, rotationY) {
    const dx = direction[0], dy = direction[1], dz = direction[2];
    let ox = origin[0] - center[0];
    const oy = origin[1] - center[1];
    let oz = origin[2] - center[2];
    let rx = dx, ry = dy, rz = dz;
    if (rotationY) {
      // Inverse of the transform used by pushProp (and the game's prop boxes):
      //   world = (u*cos + v*sin, ., -u*sin + v*cos)
      // so world -> local is the same rotation applied to the offset.
      const rad = (rotationY * Math.PI) / 180;
      const cos = Math.cos(rad), sin = Math.sin(rad);
      const nx = ox * cos - oz * sin;
      const nz = ox * sin + oz * cos;
      ox = nx; oz = nz;
      const nx2 = rx * cos - rz * sin;
      const nz2 = rx * sin + rz * cos;
      rx = nx2; rz = nz2;
    }
    let tMin = -Infinity, tMax = Infinity;
    const axes = [[ox, rx, half[0]], [oy, ry, half[1]], [oz, rz, half[2]]];
    for (const [o, d, h] of axes) {
      if (Math.abs(d) < 1e-9) {
        if (o < -h || o > h) return null;
      } else {
        let t1 = (-h - o) / d;
        let t2 = (h - o) / d;
        if (t1 > t2) { const tmp = t1; t1 = t2; t2 = tmp; }
        tMin = Math.max(tMin, t1);
        tMax = Math.min(tMax, t2);
        if (tMin > tMax) return null;
      }
    }
    if (tMax < 0) return null;
    return tMin >= 0 ? tMin : tMax;
  }

  /** Oriented pick boxes for an object: walls use their solid slices so a click
   * through a doorway passes through instead of hitting the wall's bounding box. */
  function pickBoxesFor(level, id, options) {
    const wall = findWall(level, id);
    if (wall) {
      const defaultCeiling = options && options.defaultCeiling !== undefined
        ? options.defaultCeiling
        : (level && typeof level.getCeilingHeight === 'function' ? level.getCeilingHeight() : 3.5);
      const thickness = Math.max(wallThickness(wall), 0.04);
      return wallSolidSlices(wall, defaultCeiling).map((s) => {
        const base = wallMinCorner(wall);
        const length = s.end - s.start;
        const along = wallAxis(wall) === 'x';
        const cx = along ? base.x + s.start + length / 2 : base.x + thickness / 2;
        const cz = along ? base.z + thickness / 2 : base.z + s.start + length / 2;
        return {
          kind: 'wall',
          center: [cx, (s.bottom + s.top) / 2, cz],
          half: [(along ? length : thickness) / 2, (s.top - s.bottom) / 2, (along ? thickness : length) / 2],
          rotationY: 0
        };
      });
    }
    const bounds = objectBounds3D(level, id, options);
    return bounds ? [bounds] : [];
  }

  /** Nearest object hit by a ray, or null. `ray` is {origin:[x,y,z], direction:[x,y,z]}. */
  function pickObject(level, ray, options) {
    const ids = collectIds(level, options && options.includeRooms === false);
    let best = null;
    for (const id of ids) {
      for (const bounds of pickBoxesFor(level, id, options)) {
        const t = rayBoxIntersection(ray.origin, ray.direction, bounds.center, bounds.half, bounds.rotationY);
        if (t !== null && (!best || t < best.distance)) best = { id, kind: bounds.kind, distance: t, bounds };
      }
    }
    return best;
  }

  function collectIds(level, skipRooms) {
    const ids = [];
    if (!level) return ids;
    if (!skipRooms) for (const r of level.rooms || []) ids.push(r.id);
    for (const w of level.walls || []) ids.push(w.id);
    for (const l of level.ceiling_lights || []) ids.push(l.id);
    for (const p of level.props || []) ids.push(p.id);
    if (level.spawn) ids.push('spawn');
    return ids;
  }

  /** Bounds of all object types (used by the highlight overlay). */
  function selectionBounds(level, id, options) {
    return objectBounds3D(level, id, options);
  }

  // --------------------------------------------------------------- mesh build

  function createMeshBuilder() {
    return {
      positions: [],
      colors: [],
      uvs: [],
      owners: [],
      batches: [],
      _current: null,
      beginBatch(name, material) {
        this._current = { name, material: material || name, start: this.positions.length / 3, count: 0 };
        this.batches.push(this._current);
      },
      quad(p0, p1, p2, p3, colors, uvs, owner) {
        const pts = [p0, p1, p2, p0, p2, p3];
        const cols = [colors[0], colors[1], colors[2], colors[0], colors[2], colors[3]];
        const tex = [uvs[0], uvs[1], uvs[2], uvs[0], uvs[2], uvs[3]];
        for (let i = 0; i < 6; i++) {
          this.positions.push(pts[i][0], pts[i][1], pts[i][2]);
          this.colors.push(cols[i][0], cols[i][1], cols[i][2], 1);
          this.uvs.push(tex[i][0], tex[i][1]);
          this.owners.push(owner || '');
        }
        if (this._current) this._current.count += 6;
      },
      triangle(p0, p1, p2, colors, uvs, owner) {
        const pts = [p0, p1, p2];
        for (let i = 0; i < 3; i++) {
          this.positions.push(pts[i][0], pts[i][1], pts[i][2]);
          this.colors.push(colors[i][0], colors[i][1], colors[i][2], 1);
          this.uvs.push(uvs[i][0], uvs[i][1]);
          this.owners.push(owner || '');
        }
        if (this._current) this._current.count += 3;
      }
    };
  }

  function scaleColor(color, mult) {
    return [
      Math.min(1, color[0] * mult),
      Math.min(1, color[1] * mult),
      Math.min(1, color[2] * mult)
    ];
  }

  function pushWall(builder, wall, defaultCeiling) {
    const length = wallLength(wall);
    if (!(length > EPS)) return;
    const thickness = Math.max(wallThickness(wall), 0.01);
    const baseY = wallBaseY(wall);
    const height = wallResolvedHeight(wall, defaultCeiling);
    if (!(height > EPS)) return;
    const wallTop = baseY + height;
    const axis = wallAxis(wall);
    const world = (u, v, y) => {
      const p = wallLocalToWorld(wall, u, v);
      return [p.x, y, p.z];
    };
    const slices = wallSolidSlices(wall, defaultCeiling);
    const owner = wall.id;

    for (const s of slices) {
      // Wallpaper covers two metres per repeat in the game (see
      // WALL_TILE_METRES in src/render.rs), so the preview matches.
      const texUv = (u, y) => [u / 2, y / 2];
      const lowColors = [scaleColor(COLOR.wallSide, 0.92), scaleColor(COLOR.wallSide, 0.92), scaleColor(COLOR.wallSide, 1.0), scaleColor(COLOR.wallSide, 1.0)];
      const highColors = [scaleColor(COLOR.wall, 0.92), scaleColor(COLOR.wall, 0.92), scaleColor(COLOR.wall, 1.0), scaleColor(COLOR.wall, 1.0)];
      const sideUv = [texUv(s.start, s.bottom), texUv(s.end, s.bottom), texUv(s.end, s.top), texUv(s.start, s.top)];
      // Face at v = 0 and face at v = thickness.
      builder.quad(world(s.start, 0, s.bottom), world(s.end, 0, s.bottom), world(s.end, 0, s.top), world(s.start, 0, s.top), lowColors, sideUv, owner);
      builder.quad(world(s.end, thickness, s.bottom), world(s.start, thickness, s.bottom), world(s.start, thickness, s.top), world(s.end, thickness, s.top), highColors, sideUv, owner);

      if (s.top < wallTop - 1e-3) {
        const topColor = scaleColor(COLOR.wall, 1.06);
        const uv = [[s.start / 2, 0], [s.end / 2, 0], [s.end / 2, thickness / 2], [s.start / 2, thickness / 2]];
        builder.quad(world(s.start, 0, s.top), world(s.end, 0, s.top), world(s.end, thickness, s.top), world(s.start, thickness, s.top), [topColor, topColor, topColor, topColor], uv, owner);
      }
      if (s.bottom > baseY + 1e-3) {
        const botColor = scaleColor(COLOR.wallSide, 0.82);
        const uv = [[s.start / 2, 0], [s.end / 2, 0], [s.end / 2, thickness / 2], [s.start / 2, thickness / 2]];
        builder.quad(world(s.start, thickness, s.bottom), world(s.end, thickness, s.bottom), world(s.end, 0, s.bottom), world(s.start, 0, s.bottom), [botColor, botColor, botColor, botColor], uv, owner);
      }
    }

    // Reveal faces (jambs and header undersides) where the solid range changes.
    const boundaries = new Map();
    const intervalsAt = (u, side) => {
      for (const s of slices) {
        if (Math.abs(s.start - u) <= 1e-4) return side === 'right' ? s : null;
        if (Math.abs(s.end - u) <= 1e-4) return side === 'left' ? s : null;
      }
      return null;
    };
    const values = [0, length];
    for (const s of slices) values.push(s.start, s.end);
    for (const v of values) boundaries.set(Number(v.toFixed(6)), v);
    for (const u of boundaries.values()) {
      const left = intervalsAt(u, 'left');
      const right = intervalsAt(u, 'right');
      const holes = [];
      if (left && !right) holes.push([left.bottom, left.top, -1]);
      if (right && !left) holes.push([right.bottom, right.top, 1]);
      if (left && right) {
        if (left.bottom < right.bottom - 1e-4) holes.push([left.bottom, right.bottom, 1]);
        if (right.bottom < left.bottom - 1e-4) holes.push([right.bottom, left.bottom, -1]);
        if (left.top > right.top + 1e-4) holes.push([right.top, left.top, 1]);
        if (right.top > left.top + 1e-4) holes.push([left.top, right.top, -1]);
      }
      for (const [bottom, top, dir] of holes) {
        const color = bottom <= baseY + 1e-3 ? scaleColor(COLOR.jamb, 1.0) : scaleColor(COLOR.head, 1.0);
        const colors = [color, color, scaleColor(color, 1.06), scaleColor(color, 1.06)];
        const uv = [[0, bottom / 2], [thickness / 2, bottom / 2], [thickness / 2, top / 2], [0, top / 2]];
        if (dir > 0) {
          builder.quad(world(u, 0, bottom), world(u, thickness, bottom), world(u, thickness, top), world(u, 0, top), colors, uv, owner);
        } else {
          builder.quad(world(u, thickness, bottom), world(u, 0, bottom), world(u, 0, top), world(u, thickness, top), colors, uv, owner);
        }
      }
    }
  }

  // ------------------------------------------------------------ proxy props
  //
  // `assets/prop_proxies.json` (derived from the shipped GLBs) describes a
  // prop as local-space boxes/cylinders/tubes/planes. Each part is transformed by
  // the instance's position, Y rotation and uniform scale, then flat-shaded with
  // the same multipliers the game bakes into its prop meshes.

  // Same corner order, colour and per-face shading as the game's `add_prop_box`
  // (src/render.rs); the part's own rotation (if any) is applied first.
  const PROXY_BOX_FACES = [
    { signs: [[-1, 1, -1], [-1, 1, 1], [1, 1, 1], [1, 1, -1]], shade: PROP_BOX_SHADES[0] },
    { signs: [[-1, -1, -1], [1, -1, -1], [1, -1, 1], [-1, -1, 1]], shade: PROP_BOX_SHADES[1] },
    { signs: [[-1, -1, 1], [1, -1, 1], [1, 1, 1], [-1, 1, 1]], shade: PROP_BOX_SHADES[2] },
    { signs: [[1, -1, -1], [-1, -1, -1], [-1, 1, -1], [1, 1, -1]], shade: PROP_BOX_SHADES[3] },
    { signs: [[-1, -1, -1], [-1, -1, 1], [-1, 1, 1], [-1, 1, -1]], shade: PROP_BOX_SHADES[4] },
    { signs: [[1, -1, 1], [1, -1, -1], [1, 1, -1], [1, 1, 1]], shade: PROP_BOX_SHADES[5] }
  ];

  function propScale(prop) {
    const scale = Number(prop && prop.scale);
    return Number.isFinite(scale) && scale > 0 ? scale : 1;
  }

  /** Prop-local point -> world: uniform scale, Y rotation, translation. */
  function proxyToWorld(prop) {
    const scale = propScale(prop);
    const rot = ((Number(prop.rotation_degrees) || 0) * Math.PI) / 180;
    const cos = Math.cos(rot), sin = Math.sin(rot);
    const x = Number(prop.x) || 0;
    const y = Number(prop.y) || 0;
    const z = Number(prop.z) || 0;
    return (lx, ly, lz) => {
      const sx = lx * scale, sy = ly * scale, sz = lz * scale;
      return [x + sx * cos + sz * sin, y + sy, z - sx * sin + sz * cos];
    };
  }

  /** XYZ Euler rotation in degrees, in the same order as the asset toolkit. */
  function rotateProxyPoint(point, rotation) {
    let x = point[0], y = point[1], z = point[2];
    const rx = ((Number(rotation[0]) || 0) * Math.PI) / 180;
    if (rx) { const c = Math.cos(rx), s = Math.sin(rx); const ny = y * c - z * s; z = y * s + z * c; y = ny; }
    const ry = ((Number(rotation[1]) || 0) * Math.PI) / 180;
    if (ry) { const c = Math.cos(ry), s = Math.sin(ry); const nx = x * c + z * s; z = -x * s + z * c; x = nx; }
    const rz = ((Number(rotation[2]) || 0) * Math.PI) / 180;
    if (rz) { const c = Math.cos(rz), s = Math.sin(rz); const nx = x * c - y * s; y = x * s + y * c; x = nx; }
    return [x, y, z];
  }

  function pushProxyBox(builder, part, toWorld, owner) {
    const center = part.center;
    const hx = part.size[0] / 2, hy = part.size[1] / 2, hz = part.size[2] / 2;
    const rotation = part.rotation;
    const corner = (signs) => {
      const local = rotateProxyPoint([signs[0] * hx, signs[1] * hy, signs[2] * hz], rotation);
      return toWorld(center[0] + local[0], center[1] + local[1], center[2] + local[2]);
    };
    for (const face of PROXY_BOX_FACES) {
      const color = scaleColor(part.color, face.shade);
      builder.quad(
        corner(face.signs[0]), corner(face.signs[1]), corner(face.signs[2]), corner(face.signs[3]),
        [color, color, color, color], [[0, 0], [1, 0], [1, 1], [0, 1]], owner
      );
    }
  }

  function pushProxyCylinder(builder, part, toWorld, owner) {
    const base = part.base;
    const axis = part.axis;
    const radius = part.radius, height = part.height, taper = part.taper;
    const segments = part.segments;
    const point = (angle, along, radial) => {
      const cos = Math.cos(angle), sin = Math.sin(angle);
      const ox = cos * radial, oz = sin * radial;
      // Same basis as the toolkit's `point_at` for x/y/z cylinders.
      if (axis === 'x') return toWorld(base[0] + along, base[1] + oz, base[2] + ox);
      if (axis === 'z') return toWorld(base[0] + ox, base[1] + oz, base[2] + along);
      return toWorld(base[0] + ox, base[1] + along, base[2] + oz);
    };
    const sideUv = [[0, 1], [1, 1], [1, 0], [0, 0]];
    for (let i = 0; i < segments; i++) {
      const a0 = (i / segments) * Math.PI * 2;
      const a1 = ((i + 1) / segments) * Math.PI * 2;
      const shade = 0.72 + 0.28 * (0.5 + 0.5 * Math.cos(a0 - 0.9));
      const color = scaleColor(part.color, shade);
      builder.quad(
        point(a0, 0, radius), point(a1, 0, radius),
        point(a1, height, radius * taper), point(a0, height, radius * taper),
        [color, color, color, color], sideUv, owner
      );
    }
    // Top cap only: proxy cylinders always rest on, or are drawn from, their base.
    const capColor = scaleColor(part.color, 0.96);
    const capUv = [[0.5, 0.5], [0, 1], [1, 1]];
    const capCenter = point(0, height, 0);
    for (let i = 0; i < segments; i++) {
      const a0 = (i / segments) * Math.PI * 2;
      const a1 = ((i + 1) / segments) * Math.PI * 2;
      builder.triangle(
        capCenter, point(a0, height, radius * taper), point(a1, height, radius * taper),
        [capColor, capColor, capColor], capUv, owner
      );
    }
  }

  function cross3(a, b) {
    return [
      a[1] * b[2] - a[2] * b[1],
      a[2] * b[0] - a[0] * b[2],
      a[0] * b[1] - a[1] * b[0]
    ];
  }

  function normalize3(v) {
    const length = Math.hypot(v[0], v[1], v[2]);
    return length < 1e-9 ? [0, 0, 1] : [v[0] / length, v[1] / length, v[2] / length];
  }

  function pushProxyTube(builder, part, toWorld, owner) {
    const start = part.start, end = part.end, radius = part.radius;
    const dx = end[0] - start[0], dy = end[1] - start[1], dz = end[2] - start[2];
    const length = Math.hypot(dx, dy, dz);
    if (!(length > EPS)) return;
    const direction = [dx / length, dy / length, dz / length];
    const reference = Math.abs(direction[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0];
    const ux = normalize3(cross3(reference, direction));
    const uy = cross3(direction, ux);
    const ring = (point, angle) => {
      const cos = Math.cos(angle), sin = Math.sin(angle);
      return toWorld(
        point[0] + (ux[0] * cos + uy[0] * sin) * radius,
        point[1] + (ux[1] * cos + uy[1] * sin) * radius,
        point[2] + (ux[2] * cos + uy[2] * sin) * radius
      );
    };
    const segments = 6; // the toolkit's default tube resolution
    const sideUv = [[0, 1], [1, 1], [1, 0], [0, 0]];
    for (let i = 0; i < segments; i++) {
      const a0 = (i / segments) * Math.PI * 2;
      const a1 = ((i + 1) / segments) * Math.PI * 2;
      const shade = 0.74 + 0.26 * (0.5 + 0.5 * Math.cos(a0 - 1.1));
      const color = scaleColor(part.color, shade);
      builder.quad(
        ring(start, a0), ring(start, a1), ring(end, a1), ring(end, a0),
        [color, color, color, color], sideUv, owner
      );
    }
    const endColor = scaleColor(part.color, 0.95);
    const startColor = scaleColor(part.color, 0.7);
    const capUv = [[0.5, 0.5], [0, 1], [1, 1]];
    const startPoint = toWorld(start[0], start[1], start[2]);
    const endPoint = toWorld(end[0], end[1], end[2]);
    for (let i = 0; i < segments; i++) {
      const a0 = (i / segments) * Math.PI * 2;
      const a1 = ((i + 1) / segments) * Math.PI * 2;
      builder.triangle(endPoint, ring(end, a0), ring(end, a1), [endColor, endColor, endColor], capUv, owner);
      builder.triangle(startPoint, ring(start, a1), ring(start, a0), [startColor, startColor, startColor], capUv, owner);
    }
  }

  function pushProxyPlane(builder, part, toWorld, owner) {
    const center = part.center;
    const hx = part.size[0] / 2, hy = part.size[1] / 2, hz = part.size[2] / 2;
    // Corner order matches the toolkit's `plane` helper for each normal.
    let corners;
    if (part.normal === 'z') {
      corners = [
        [center[0] - hx, center[1] - hy, center[2]], [center[0] + hx, center[1] - hy, center[2]],
        [center[0] + hx, center[1] + hy, center[2]], [center[0] - hx, center[1] + hy, center[2]]
      ];
    } else if (part.normal === '-z') {
      corners = [
        [center[0] + hx, center[1] - hy, center[2]], [center[0] - hx, center[1] - hy, center[2]],
        [center[0] - hx, center[1] + hy, center[2]], [center[0] + hx, center[1] + hy, center[2]]
      ];
    } else if (part.normal === 'x') {
      corners = [
        [center[0], center[1] - hy, center[2] + hz], [center[0], center[1] - hy, center[2] - hz],
        [center[0], center[1] + hy, center[2] - hz], [center[0], center[1] + hy, center[2] + hz]
      ];
    } else if (part.normal === '-x') {
      corners = [
        [center[0], center[1] - hy, center[2] - hz], [center[0], center[1] - hy, center[2] + hz],
        [center[0], center[1] + hy, center[2] + hz], [center[0], center[1] + hy, center[2] - hz]
      ];
    } else {
      corners = [
        [center[0] - hx, center[1], center[2] - hz], [center[0] + hx, center[1], center[2] - hz],
        [center[0] + hx, center[1], center[2] + hz], [center[0] - hx, center[1], center[2] + hz]
      ];
    }
    const color = part.color;
    builder.quad(
      toWorld(corners[0][0], corners[0][1], corners[0][2]),
      toWorld(corners[1][0], corners[1][1], corners[1][2]),
      toWorld(corners[2][0], corners[2][1], corners[2][2]),
      toWorld(corners[3][0], corners[3][1], corners[3][2]),
      [color, color, color, color], [[0, 0], [1, 0], [1, 1], [0, 1]], owner
    );
  }

  function pushProxyProp(builder, prop, proxy) {
    const toWorld = proxyToWorld(prop);
    const owner = prop.id;
    for (const part of proxy.parts) {
      if (part.shape === 'box') pushProxyBox(builder, part, toWorld, owner);
      else if (part.shape === 'cylinder') pushProxyCylinder(builder, part, toWorld, owner);
      else if (part.shape === 'tube') pushProxyTube(builder, part, toWorld, owner);
      else if (part.shape === 'plane') pushProxyPlane(builder, part, toWorld, owner);
    }
  }

  function pushProp(builder, prop, catalog, proxies) {
    const size = propSize(prop, catalog);
    const entry = catalog && typeof catalog.get === 'function' ? catalog.get(prop.model) : null;
    const proxy = proxies && typeof proxies.get === 'function' ? proxies.get(prop.model) : null;
    if (proxy && Array.isArray(proxy.parts) && proxy.parts.length) {
      pushProxyProp(builder, prop, proxy);
      return;
    }
    const base = (entry && entry.color) || PROP_FALLBACK_COLOR;
    const rot = ((Number(prop.rotation_degrees) || 0) * Math.PI) / 180;
    const cos = Math.cos(rot), sin = Math.sin(rot);
    const hx = size[0] / 2, hz = size[2] / 2;
    const cx = Number(prop.x) || 0;
    const cy = (Number(prop.y) || 0);
    const cz = Number(prop.z) || 0;
    const y0 = cy, y1 = cy + size[1];
    const corner = (sx, sz) => [cx + sx * hx * cos + sz * hz * sin, 0, cz - sx * hx * sin + sz * hz * cos];
    const c00 = corner(-1, -1), c10 = corner(1, -1), c11 = corner(1, 1), c01 = corner(-1, 1);
    const at = (p, y) => [p[0], y, p[2]];
    const shades = [1.0, 0.86, 0.74, 0.92, 1.12, 0.62];
    const owner = prop.id;
    const face = (p0, p1, p2, p3, shade) => {
      const color = scaleColor(base, shade);
      builder.quad(p0, p1, p2, p3, [color, color, scaleColor(color, 1.06), scaleColor(color, 1.06)], [[0, 0], [1, 0], [1, 1], [0, 1]], owner);
    };
    face(at(c00, y0), at(c10, y0), at(c10, y1), at(c00, y1), shades[0]);
    face(at(c11, y0), at(c01, y0), at(c01, y1), at(c11, y1), shades[1]);
    face(at(c10, y0), at(c11, y0), at(c11, y1), at(c10, y1), shades[2]);
    face(at(c01, y0), at(c00, y0), at(c00, y1), at(c01, y1), shades[3]);
    face(at(c00, y1), at(c10, y1), at(c11, y1), at(c01, y1), shades[4]);
    face(at(c01, y0), at(c11, y0), at(c10, y0), at(c00, y0), shades[5]);
  }

  function pushLight(builder, light, ceilingHeight) {
    // Same rule as the game and lighting.js (see lighting.fixtureIsTurned).
    const [halfW, halfD] = lighting.fixtureHalfExtents(light.rotation_degrees);
    const y = Math.max(0.05, (ceilingHeight || 3.5) - 0.06);
    const x0 = light.x - halfW, x1 = light.x + halfW;
    const z0 = light.z - halfD, z1 = light.z + halfD;
    // The face is texture-first: it carries only the fixture's neutral
    // emission strength, never the authored light colour, so the artwork keeps
    // its own colour (see `emit_fixtures` in src/render/common/geometry.rs). The
    // authored colour still lights the room through `lighting.bakeLevelLighting`.
    const authored = light.brightness !== undefined && light.brightness !== null
      ? light.brightness
      : (light.intensity !== undefined && light.intensity !== null ? light.intensity : 1.0);
    const intensity = lighting.sanitizeIntensity(authored);
    // An explicitly zero-output fixture is off and shows no glow (see render.rs).
    const output = intensity <= 0
      ? 0
      : Math.min(Math.max(0.40 * Math.min(intensity, 2.0) + 0.60, 0), 1);
    const color = [output, output, output];
    builder.quad(
      [x0, y, z1], [x1, y, z1], [x1, y, z0], [x0, y, z0],
      [scaleColor(color, 0.85), color, color, scaleColor(color, 0.85)],
      [[0, 0], [1, 0], [1, 1], [0, 1]],
      light.id
    );
  }

  /**
   * Builds the complete 3D mesh spec for a level.
   * options: { catalog, proxies, includeRooms, includeCeilings, defaultCeiling }
   * `proxies` is the derived prop_proxies.json accessor; without it (or for an
   * unknown prop) the catalogue box is drawn instead.
   */
  function buildLevelMesh(level, options) {
    const opts = options || {};
    const builder = createMeshBuilder();
    const ceilings = opts.includeCeilings === undefined ? true : !!opts.includeCeilings;

    builder.beginBatch('floor', 'floor');
    for (const room of level.rooms || []) {
      const x0 = Math.min(room.x, room.x + room.width);
      const x1 = Math.max(room.x, room.x + room.width);
      const z0 = Math.min(room.z, room.z + room.depth);
      const z1 = Math.max(room.z, room.z + room.depth);
      builder.quad(
        [x0, 0, z0], [x1, 0, z0], [x1, 0, z1], [x0, 0, z1],
        [COLOR.floor, COLOR.floor, COLOR.floor, COLOR.floor],
        [[x0 / 2, z0 / 2], [x1 / 2, z0 / 2], [x1 / 2, z1 / 2], [x0 / 2, z1 / 2]],
        room.id
      );
    }

    if (ceilings) {
      builder.beginBatch('ceiling', 'ceiling');
      for (const room of level.rooms || []) {
        const x0 = Math.min(room.x, room.x + room.width);
        const x1 = Math.max(room.x, room.x + room.width);
        const z0 = Math.min(room.z, room.z + room.depth);
        const z1 = Math.max(room.z, room.z + room.depth);
        const h = Math.max(0, Number(room.height) || 0);
        builder.quad(
          [x0, h, z1], [x1, h, z1], [x1, h, z0], [x0, h, z0],
          [COLOR.ceiling, COLOR.ceiling, COLOR.ceiling, COLOR.ceiling],
          [[x0 / 2, z1 / 2], [x1 / 2, z1 / 2], [x1 / 2, z0 / 2], [x0 / 2, z0 / 2]],
          room.id
        );
      }
    }

    builder.beginBatch('walls', 'wall');
    for (const wall of level.walls || []) {
      pushWall(builder, wall, opts.defaultCeiling);
    }

    builder.beginBatch('lights', 'light');
    for (const light of level.ceiling_lights || []) {
      const ceiling = level.getCeilingHeight ? level.getCeilingHeight(light.x, light.z) : 3.5;
      pushLight(builder, light, ceiling);
    }

    builder.beginBatch('props', 'prop');
    for (const prop of level.props || []) {
      pushProp(builder, prop, opts.catalog, opts.proxies);
    }

    // Bake the static lighting into every generated vertex colour so the
    // preview's room brightness matches the game (src/lighting.rs). The
    // fixture panels themselves stay emissive.
    if (opts.lighting !== false && lighting && typeof lighting.bakeLevelLighting === 'function') {
      applyBakedLighting(builder, level, opts.lightingBake);
    }

    return {
      positions: new Float32Array(builder.positions),
      colors: new Float32Array(builder.colors),
      uvs: new Float32Array(builder.uvs),
      owners: builder.owners,
      batches: builder.batches,
      vertexCount: builder.positions.length / 3
    };
  }

  /**
   * Multiplies every vertex colour by the baked lighting sampled at its world
   * position, using the owning room for wall/room surfaces so boundary vertices
   * are lit by the surface's own room (exactly like the game). Vertices whose
   * owner has no room fall back to the deterministic containment rule, and the
   * emissive fixture batch is left alone.
   */
  function applyBakedLighting(builder, level, bake) {
    const baked = bake || lighting.bakeLevelLighting(level);

    // Map each batch owner to the room that lights it.
    const ownerRoom = new Map();
    (level.rooms || []).forEach((room, index) => {
      if (room && room.id) ownerRoom.set(room.id, index);
    });
    for (const wall of level.walls || []) {
      if (!wall || !wall.id) continue;
      const x = (Number(wall.x) || 0) + (Number(wall.width) || 0) * 0.5;
      const z = (Number(wall.z) || 0) + (Number(wall.depth) || 0) * 0.5;
      ownerRoom.set(wall.id, baked.roomIndexAt(x, z));
    }
    for (const prop of level.props || []) {
      if (!prop || !prop.id) continue;
      ownerRoom.set(prop.id, baked.roomIndexAt(Number(prop.x) || 0, Number(prop.z) || 0));
    }

    const positions = builder.positions;
    const colors = builder.colors;
    const owners = builder.owners;
    for (const batch of builder.batches) {
      if (batch.material === 'light') continue;
      for (let vertex = batch.start; vertex < batch.start + batch.count; vertex++) {
        const p = vertex * 3;
        let roomIndex = ownerRoom.get(owners[vertex]);
        if (roomIndex === undefined) {
          roomIndex = baked.roomIndexAt(positions[p], positions[p + 2]);
        }
        if (roomIndex === undefined || roomIndex < 0) continue;
        const light = baked.sampleInRoom(roomIndex, positions[p], positions[p + 1], positions[p + 2]);
        const c = vertex * 4;
        colors[c] = Math.min(1, colors[c] * light[0]);
        colors[c + 1] = Math.min(1, colors[c + 1] * light[1]);
        colors[c + 2] = Math.min(1, colors[c + 2] * light[2]);
      }
    }
  }

  /** Level statistics for the "nothing selected" inspector state. */
  function levelStats(level) {
    let openings = 0;
    for (const wall of (level && level.walls) || []) openings += openingsOf(wall).length;
    return {
      rooms: (level && level.rooms ? level.rooms.length : 0),
      walls: (level && level.walls ? level.walls.length : 0),
      openings,
      lights: (level && level.ceiling_lights ? level.ceiling_lights.length : 0),
      props: (level && level.props ? level.props.length : 0),
      patches: (level && level.floor_patches ? level.floor_patches.length : 0)
    };
  }

  return {
    EPS,
    COLOR,
    OPENING_KINDS,
    OPENING_TYPES,
    PROP_FALLBACK_SIZE,
    PROP_FALLBACK_COLOR,
    wallAxis,
    wallLength,
    wallThickness,
    wallMinCorner,
    wallBaseY,
    wallResolvedHeight,
    wallLocalToWorld,
    wallProjectOffset,
    wallDistanceToPoint,
    normalizeOpening,
    normalizeOpenings,
    openingsOf,
    wallSolidSlices,
    wallSolidRects2D,
    wallCollisionBoxes,
    propSize,
    propBounds,
    objectBounds3D,
    pickBoxesFor,
    selectionBounds,
    rayBoxIntersection,
    pickObject,
    collectIds,
    findWall,
    findLight,
    findRoom,
    findProp,
    findOpening,
    createMeshBuilder,
    buildLevelMesh,
    levelStats
  };
});
