// lighting.js - Static baked-lighting mirror for the editor's 3D preview.
//
// The game (`src/lighting.rs`) is authoritative: this module only exists so the
// editor preview can approximate the same room illumination while an author
// edits. It implements the same model with the same tuned constants:
//
//   * every light emits an [r, g, b] colour and the bake accumulates per
//     channel, so a red fixture tints surrounding geometry red;
//   * room baseline = AMBIENT + (MAX_BRIGHTNESS - AMBIENT) * c, where the
//     fixture density is logarithmically compressed and then saturated
//     (`c = n / (1 + n)`, `n = ln(1 + density * REFERENCE_LIGHT_AREA_M2)`);
//   * broad local fixture pools with a smooth falloff, capped per channel;
//   * bounded blending through walk-through openings;
//   * the same deterministic ownership rule (smallest containing room wins).
//
// Deliberate differences from the game: the preview works on flat-shaded quads,
// so it is an approximation, not a pixel-exact match. It never allocates during
// drawing and is recomputed only when the preview mesh is rebuilt.

(function (root, factory) {
  const deps = (typeof module !== 'undefined' && module.exports)
    ? { model: require('./model.js') }
    : { model: root };
  const api = factory(deps.model);
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.LiminalLighting = api;
  }
})(typeof window !== 'undefined' ? window : null, function (model) {
  'use strict';

  // Mirrors the constants in src/lighting.rs. Keep both files in step.
  const TUNING = {
    REFERENCE_LIGHT_AREA_M2: 500.0,
    REFERENCE_CEILING_HEIGHT_M: 3.5,
    HEIGHT_FALLOFF: 0.5,
    AMBIENT_LEVEL: 0.10,
    MAX_BRIGHTNESS: 1.0,
    MAX_LIGHT_COLOR: 1.0,
    DEFAULT_LIGHT_COLOR: [1.0, 0.96, 0.88],
    LOCAL_LIGHT_RADIUS_M: 6.0,
    LOCAL_LIGHT_STRENGTH: 0.42,
    LOCAL_LIGHT_MAX: 0.45,
    OPENING_BLEND_RADIUS_M: 6.0,
    OPENING_BLEND_STRENGTH: 0.5,
    OPENING_VERTICAL_FADE_M: 1.0,
    FIXTURE_HALF_WIDTH_M: 0.6,
    FIXTURE_HALF_DEPTH_M: 0.3,
    FIXTURE_DROP_M: 0.01,
    MAX_LIGHT_INTENSITY: model && Number.isFinite(model.LIGHT_INTENSITY_MAX)
      ? model.LIGHT_INTENSITY_MAX
      : 8.0
  };

  const ROOM_EDGE_EPS_M = 0.01;
  const OPENING_PROBE_M = 0.05;

  // Mirrors the constants in src/lighting/visibility.rs.
  const SURFACE_EPS_M = 5.0e-3;
  const POINT_GRID_CELL_M = 4.0;
  const CLEAR_SAMPLE_STEP_M = 0.05;
  const CLEAR_SAMPLE_MAX_STEPS = 64;

  function isFiniteNumber(value) {
    return typeof value === 'number' && Number.isFinite(value);
  }

  /** Neutral ambient fill, mirroring `ambient_color()` in the game. */
  function ambientColor() {
    return [TUNING.AMBIENT_LEVEL, TUNING.AMBIENT_LEVEL, TUNING.AMBIENT_LEVEL];
  }

  /** Sanitises an authored fixture intensity exactly like the game does. */
  function sanitizeIntensity(intensity) {
    const value = Number(intensity);
    if (Number.isNaN(value)) return 1.0;
    if (!Number.isFinite(value)) return value > 0 ? TUNING.MAX_LIGHT_INTENSITY : 0.0;
    return Math.min(Math.max(value, 0.0), TUNING.MAX_LIGHT_INTENSITY);
  }

  /**
   * Sanitises one authored light colour into an [r, g, b] array.
   *
   * Mirrors `LightColor::sanitized`: non-finite channels become 0, finite
   * out-of-range channels clamp to [0, MAX_LIGHT_COLOR]. An omitted or malformed
   * entry falls back to the documented restrained warm default, which is what
   * every legacy level without a `color` key emits.
   */
  function sanitizeColor(color) {
    if (color === undefined || color === null || typeof color === 'object' && !Array.isArray(color)) {
      return TUNING.DEFAULT_LIGHT_COLOR.slice();
    }
    const source = Array.isArray(color) ? color : [];
    const channel = (index) => {
      const value = Number(source[index]);
      if (Number.isNaN(value)) return 0.0;
      if (!Number.isFinite(value)) return value > 0 ? TUNING.MAX_LIGHT_COLOR : 0.0;
      return Math.min(Math.max(value, 0.0), TUNING.MAX_LIGHT_COLOR);
    };
    return [channel(0), channel(1), channel(2)];
  }

  /** Emitted colour of one light definition, defaulted like the game. */
  function emittedColor(light) {
    if (!light || light.color === undefined || light.color === null) {
      return TUNING.DEFAULT_LIGHT_COLOR.slice();
    }
    return sanitizeColor(light.color);
  }

  /** Rec. 709 luminance, mirroring `LightColor::luminance`. */
  function luminance(color) {
    return 0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2];
  }

  /** Per-channel clamp into [low, high]. */
  function clampColor(color, low, high) {
    return [
      Math.min(Math.max(color[0], low), high),
      Math.min(Math.max(color[1], low), high),
      Math.min(Math.max(color[2], low), high)
    ];
  }

  /** Gentle ceiling-height correction: lower ceilings make fixtures count more. */
  function ceilingHeightFactor(height) {
    if (!isFiniteNumber(height) || height <= 0) return 1.0;
    const clamped = Math.min(Math.max(height, 0.5), 100);
    return Math.pow(TUNING.REFERENCE_CEILING_HEIGHT_M / clamped, TUNING.HEIGHT_FALLOFF);
  }

  /** Smooth saturating brightness component, `n / (1 + n)`. */
  function saturatingBrightness(normalizedDensity) {
    if (Number.isNaN(normalizedDensity)) return 0.0;
    if (!Number.isFinite(normalizedDensity)) return normalizedDensity > 0 ? 1.0 : 0.0;
    const n = Math.max(normalizedDensity, 0.0);
    return n / (1 + n);
  }

  /** Logarithmic fixture-density compression, mirroring `compressed_density`. */
  function compressedDensity(normalizedDensity) {
    if (Number.isNaN(normalizedDensity)) return 0.0;
    if (!(normalizedDensity > 0)) return 0.0;
    if (!Number.isFinite(normalizedDensity)) return Infinity;
    return Math.log1p(normalizedDensity);
  }

  /** `1 - smoothstep(t)`: 1 at t = 0, 0 at t = 1, flat at both ends. */
  function smoothFalloff(t) {
    if (Number.isNaN(t)) return 0.0;
    const clamped = Math.min(Math.max(t, 0.0), 1.0);
    const u = 1.0 - clamped;
    return u * u * (1 + 2 * clamped);
  }

  /**
   * Whether a fixture's panel is turned 90 degrees from its default.
   *
   * Mirrors `fixture_is_turned` in src/lighting.rs: the authored rotation is
   * rounded to whole degrees and tested against 180, so the drawn panel and its
   * light pool always agree (a 180-degree rotation is back to default, a 90 is
   * turned, and fractional rotations cannot make the two disagree).
   */
  function fixtureIsTurned(rotationDegrees) {
    const rotation = Number(rotationDegrees) || 0;
    return Math.abs(Math.round(rotation)) % 180 > 0;
  }

  /** Half-extents of a fixture panel in world X/Z after rotation. */
  function fixtureHalfExtents(rotationDegrees) {
    return fixtureIsTurned(rotationDegrees)
      ? [TUNING.FIXTURE_HALF_DEPTH_M, TUNING.FIXTURE_HALF_WIDTH_M]
      : [TUNING.FIXTURE_HALF_WIDTH_M, TUNING.FIXTURE_HALF_DEPTH_M];
  }

  /**
   * Baseline illumination of a room from its floor area and the summed emitted
   * colour of its fixtures. `effectivePower` is an [r, g, b] array; the result
   * is an [r, g, b] array.
   */
  function roomBaseline(areaM2, effectivePower) {
    const area = isFiniteNumber(areaM2) ? Math.max(areaM2, 0.01) : 0.01;
    const power = Array.isArray(effectivePower)
      ? effectivePower
      : [Number(effectivePower) || 0, Number(effectivePower) || 0, Number(effectivePower) || 0];
    const channel = (value) => {
      const finite = isFiniteNumber(value) ? Math.max(value, 0.0) : value > 0 ? Infinity : 0.0;
      const normalized = compressedDensity((finite / area) * TUNING.REFERENCE_LIGHT_AREA_M2);
      const component = saturatingBrightness(normalized);
      const baseline = TUNING.AMBIENT_LEVEL
        + (TUNING.MAX_BRIGHTNESS - TUNING.AMBIENT_LEVEL) * component;
      return Math.min(Math.max(baseline, TUNING.AMBIENT_LEVEL), TUNING.MAX_BRIGHTNESS);
    };
    return [channel(power[0]), channel(power[1]), channel(power[2])];
  }

  function roomBounds(room) {
    const x = Number(room.x) || 0;
    const z = Number(room.z) || 0;
    const width = Number(room.width) || 0;
    const depth = Number(room.depth) || 0;
    return {
      x0: Math.min(x, x + width),
      x1: Math.max(x, x + width),
      z0: Math.min(z, z + depth),
      z1: Math.max(z, z + depth)
    };
  }

  function wallAxis(wall) {
    return Math.abs(Number(wall.width) || 0) >= Math.abs(Number(wall.depth) || 0) ? 'x' : 'z';
  }

  /**
   * Splits one wall into its solid length columns and their vertical spans.
   *
   * Mirrors `wall_solid_slices_profiled` in `src/level.rs`, flattened to the
   * single clear height the editor preview models: `clearCeilingAt` is the
   * room's own eave height, so a gable profile previews as its eave.
   */
  function wallSolidColumns(wall, clearCeilingAt) {
    const axis = wallAxis(wall);
    const length = axis === 'x' ? Math.abs(Number(wall.width) || 0) : Math.abs(Number(wall.depth) || 0);
    if (!(length > 1e-4)) return [];
    const baseY = Number(wall.y) || 0;
    const authoredHeight = Number(wall.height);
    const hasHeight = Number.isFinite(authoredHeight) && wall.height !== undefined && wall.height !== null;
    if (hasHeight && !(authoredHeight > 0)) return [];
    const topAt = (offset) => baseY + (hasHeight ? authoredHeight : clearCeilingAt(offset));
    const base = Math.min(baseY, topAt(0), topAt(length));
    if (!Number.isFinite(base)) return [];

    const columns = [];
    const openings = [];
    for (const opening of wall.openings || []) {
      const offset = Number(opening.offset);
      const width = Number(opening.width);
      const height = Number(opening.height);
      const sill = Number(opening.sill) || 0;
      if (!isFiniteNumber(offset) || !isFiniteNumber(width) || !isFiniteNumber(height)) continue;
      if (!(width > 0) || !(height > 0) || !Number.isFinite(sill)) continue;
      const start = Math.min(Math.max(offset, 0), length);
      const end = Math.min(Math.max(offset + width, 0), length);
      if (!(end > start + 1e-4)) continue;
      const localCeiling = Math.max(topAt(start), topAt(end));
      if (!Number.isFinite(localCeiling)) continue;
      const low = Math.min(base, localCeiling);
      const bottom = Math.min(Math.max(base + Math.max(sill, 0), low), localCeiling);
      const top = Math.min(Math.max(base + Math.max(sill, 0) + height, low), localCeiling);
      if (!(top > bottom + 1e-4)) continue;
      openings.push({ start, end, bottom, top });
    }

    const cuts = [0, length];
    for (const opening of openings) cuts.push(opening.start, opening.end);
    cuts.sort((a, b) => a - b);
    const unique = [];
    for (const cut of cuts) {
      if (unique.length === 0 || Math.abs(cut - unique[unique.length - 1]) > 1e-4) unique.push(cut);
    }

    for (let i = 0; i + 1 < unique.length; i++) {
      const start = unique[i];
      const end = unique[i + 1];
      if (!(end > start + 1e-4)) continue;
      const segmentCeiling = Math.max(topAt(start), topAt(end));
      if (!Number.isFinite(segmentCeiling) || !(segmentCeiling > base + 1e-4)) continue;
      const holes = openings
        .filter((o) => o.start <= start + 1e-4 && o.end + 1e-4 >= end)
        .map((o) => [o.bottom, o.top])
        .sort((a, b) => a[0] - b[0]);
      const spans = [];
      let cursor = base;
      for (const [low, high] of holes) {
        if (low > cursor + 1e-4) spans.push([cursor, Math.min(low, segmentCeiling)]);
        cursor = Math.max(cursor, high);
      }
      if (cursor < segmentCeiling - 1e-4) spans.push([cursor, segmentCeiling]);
      if (spans.length > 0) columns.push({ start, end, spans });
    }
    return columns;
  }

  /**
   * Builds the editor's opaque-box set and the per-site segment query.
   *
   * Mirrors `Visibility` in `src/lighting/visibility.rs`: one box per solid
   * patch of wall, shrunk by `SURFACE_EPS_M` so a light or a surfel flush with
   * a wall face is not blocked by it, plus a per-site range of the boxes whose
   * footprint reaches that site's radius.
   */
  function buildVisibility(level, rooms, lights, blendSites) {
    const blockers = [];
    const roomAt = (x, z) => {
      let best = -1;
      for (let i = 0; i < rooms.length; i++) {
        const room = rooms[i];
        if (x < room.x0 - ROOM_EDGE_EPS_M || x > room.x1 + ROOM_EDGE_EPS_M) continue;
        if (z < room.z0 - ROOM_EDGE_EPS_M || z > room.z1 + ROOM_EDGE_EPS_M) continue;
        if (best === -1 || room.area < rooms[best].area) best = i;
      }
      return best;
    };
    const clearCeilingAt = (x, z, fallback) => {
      const index = roomAt(x, z);
      return index >= 0 ? rooms[index].height : fallback;
    };

    for (const wall of (level && level.walls) || []) {
      const axis = wallAxis(wall);
      const x = Number(wall.x) || 0;
      const z = Number(wall.z) || 0;
      const width = Number(wall.width) || 0;
      const depth = Number(wall.depth) || 0;
      const x0 = Math.min(x, x + width);
      const x1 = Math.max(x, x + width);
      const z0 = Math.min(z, z + depth);
      const z1 = Math.max(z, z + depth);
      const originX = axis === 'x' ? x0 : z0;
      const originZ = axis === 'x' ? z0 : x0;
      // The preview resolves the clear height at the wall's own midpoint for
      // every offset, which is exact for the flat ceilings it draws.
      const fallback = rooms.length > 0 ? rooms[0].height : TUNING.REFERENCE_CEILING_HEIGHT_M;
      const along = axis === 'x' ? (x0 + x1) * 0.5 : (z0 + z1) * 0.5;
      const across = axis === 'x' ? (z0 + z1) * 0.5 : (x0 + x1) * 0.5;
      const clear = clearCeilingAt(along, across, fallback);
      for (const column of wallSolidColumns(wall, () => clear)) {
        const lengthMin = originX + column.start;
        const lengthMax = originX + column.end;
        const acrossMin = axis === 'x' ? z0 : x0;
        const acrossMax = axis === 'x' ? z1 : x1;
        for (const [bottom, top] of column.spans) {
          const min = axis === 'x'
            ? [lengthMin + SURFACE_EPS_M, bottom + SURFACE_EPS_M, acrossMin + SURFACE_EPS_M]
            : [acrossMin + SURFACE_EPS_M, bottom + SURFACE_EPS_M, lengthMin + SURFACE_EPS_M];
          const max = axis === 'x'
            ? [lengthMax - SURFACE_EPS_M, top - SURFACE_EPS_M, acrossMax - SURFACE_EPS_M]
            : [acrossMax - SURFACE_EPS_M, top - SURFACE_EPS_M, lengthMax - SURFACE_EPS_M];
          if (min[0] < max[0] && min[1] < max[1] && min[2] < max[2]) blockers.push({ min, max });
        }
      }
    }

    const sites = lights.map((light) => ({
      x: light.x,
      z: light.z,
      radius: Math.max(TUNING.LOCAL_LIGHT_RADIUS_M, light.halfW, light.halfD)
    })).concat(blendSites.map((site) => ({ x: site.x, z: site.z, radius: site.radius })));
    const ranges = sites.map((site) => {
      const x0 = site.x - site.radius;
      const x1 = site.x + site.radius;
      const z0 = site.z - site.radius;
      const z1 = site.z + site.radius;
      const list = [];
      for (let i = 0; i < blockers.length; i++) {
        const box = blockers[i];
        if (box.min[0] <= x1 && box.max[0] >= x0 && box.min[2] <= z1 && box.max[2] >= z0) list.push(i);
      }
      return list;
    });

    // A uniform grid over the boxes answers "is this point inside a wall?" for
    // the surface-sample walk, exactly like `PointGrid` in the game.
    let gridMinX = Infinity;
    let gridMinZ = Infinity;
    let gridMaxX = -Infinity;
    let gridMaxZ = -Infinity;
    for (const box of blockers) {
      gridMinX = Math.min(gridMinX, box.min[0]);
      gridMinZ = Math.min(gridMinZ, box.min[2]);
      gridMaxX = Math.max(gridMaxX, box.max[0]);
      gridMaxZ = Math.max(gridMaxZ, box.max[2]);
    }
    const hasGrid = blockers.length > 0 && Number.isFinite(gridMinX) && Number.isFinite(gridMaxX)
      && Number.isFinite(gridMinZ) && Number.isFinite(gridMaxZ);
    const cellsX = hasGrid ? Math.max(1, Math.min(1024, Math.ceil((gridMaxX - gridMinX) / POINT_GRID_CELL_M) + 1)) : 0;
    const cellsZ = hasGrid ? Math.max(1, Math.min(1024, Math.ceil((gridMaxZ - gridMinZ) / POINT_GRID_CELL_M) + 1)) : 0;
    const grid = hasGrid ? new Array(cellsX * cellsZ).fill(null).map(() => []) : [];
    if (hasGrid) {
      for (let index = 0; index < blockers.length; index++) {
        const box = blockers[index];
        const lowX = Math.floor((box.min[0] - gridMinX) / POINT_GRID_CELL_M);
        const lowZ = Math.floor((box.min[2] - gridMinZ) / POINT_GRID_CELL_M);
        const highX = Math.floor((box.max[0] - gridMinX) / POINT_GRID_CELL_M);
        const highZ = Math.floor((box.max[2] - gridMinZ) / POINT_GRID_CELL_M);
        for (let iz = lowZ; iz <= highZ; iz++) {
          for (let ix = lowX; ix <= highX; ix++) {
            if (ix < 0 || iz < 0 || ix >= cellsX || iz >= cellsZ) continue;
            grid[iz * cellsX + ix].push(index);
          }
        }
      }
    }

    function segmentHitsBox(box, from, to) {
      let enter = 0.0;
      let exit = 1.0;
      for (let axis = 0; axis < 3; axis++) {
        const start = from[axis];
        const delta = to[axis] - start;
        const low = box.min[axis];
        const high = box.max[axis];
        if (Math.abs(delta) <= Number.EPSILON) {
          if (start < low || start > high) return false;
          continue;
        }
        const inverse = 1.0 / delta;
        let near = (low - start) * inverse;
        let far = (high - start) * inverse;
        if (near > far) {
          const swap = near;
          near = far;
          far = swap;
        }
        enter = Math.max(enter, near);
        exit = Math.min(exit, far);
        if (enter > exit) return false;
      }
      return true;
    }

    function occludes(site, from, to) {
      if (!from.every(isFiniteNumber) || !to.every(isFiniteNumber)) return true;
      const list = ranges[site];
      if (!list) return false;
      for (const index of list) {
        if (segmentHitsBox(blockers[index], from, to)) return true;
      }
      return false;
    }

    function containsPoint(x, z) {
      if (!hasGrid || !isFiniteNumber(x) || !isFiniteNumber(z)) return false;
      const ix = Math.floor((x - gridMinX) / POINT_GRID_CELL_M);
      const iz = Math.floor((z - gridMinZ) / POINT_GRID_CELL_M);
      if (ix < 0 || iz < 0 || ix >= cellsX || iz >= cellsZ) return false;
      for (const index of grid[iz * cellsX + ix]) {
        const box = blockers[index];
        if (x >= box.min[0] && x <= box.max[0] && z >= box.min[2] && z <= box.max[2]) return true;
      }
      return false;
    }

    return { occludes, containsPoint };
  }

  /**
   * Bakes the level's room baselines, fixture pools and opening blends.
   *
   * Missing or malformed data never throws and never produces NaN, matching the
   * game's tolerance for community levels.
   */
  function bakeLevelLighting(level) {
    const rooms = ((level && level.rooms) || []).map((room) => {
      const bounds = roomBounds(room);
      const width = Math.max(bounds.x1 - bounds.x0, 0);
      const depth = Math.max(bounds.z1 - bounds.z0, 0);
      const rawHeight = Number(room.height);
      const rawFloor = Number(room.floor_y);
      return {
        id: room.id,
        x0: bounds.x0,
        x1: bounds.x1,
        z0: bounds.z0,
        z1: bounds.z1,
        height: isFiniteNumber(rawHeight) && rawHeight > 0 ? rawHeight : TUNING.REFERENCE_CEILING_HEIGHT_M,
        floorY: isFiniteNumber(rawFloor) ? rawFloor : 0.0,
        area: width * depth,
        fixtureCount: 0,
        effectivePower: [0, 0, 0],
        baseline: ambientColor()
      };
    });

    const roomIndexAt = (x, z) => {
      if (!isFiniteNumber(x) || !isFiniteNumber(z)) return -1;
      let best = -1;
      for (let i = 0; i < rooms.length; i++) {
        const room = rooms[i];
        if (x < room.x0 - ROOM_EDGE_EPS_M || x > room.x1 + ROOM_EDGE_EPS_M) continue;
        if (z < room.z0 - ROOM_EDGE_EPS_M || z > room.z1 + ROOM_EDGE_EPS_M) continue;
        // Smallest containing room wins; ties keep the earlier room.
        if (best === -1 || room.area < rooms[best].area) best = i;
      }
      return best;
    };

    const fallbackHeight = rooms.length > 0 ? rooms[0].height : TUNING.REFERENCE_CEILING_HEIGHT_M;
    const lights = [];
    for (const light of (level && level.ceiling_lights) || []) {
      const x = Number(light.x);
      const z = Number(light.z);
      if (!isFiniteNumber(x) || !isFiniteNumber(z)) continue;
      const roomIndex = roomIndexAt(x, z);
      const height = roomIndex >= 0 ? rooms[roomIndex].height : fallbackHeight;
      const intensity = sanitizeIntensity(light.brightness !== undefined && light.brightness !== null
        ? light.brightness
        : light.intensity);
      const heightFactor = ceilingHeightFactor(height);
      const rotation = Number(light.rotation_degrees) || 0;
      const [halfW, halfD] = fixtureHalfExtents(rotation);
      const color = emittedColor(light);
      if (roomIndex >= 0) {
        const power = intensity * heightFactor;
        const room = rooms[roomIndex];
        room.fixtureCount += 1;
        room.effectivePower[0] += power * color[0];
        room.effectivePower[1] += power * color[1];
        room.effectivePower[2] += power * color[2];
      }
      lights.push({
        x,
        z,
        y: height - TUNING.FIXTURE_DROP_M,
        intensity,
        color,
        heightFactor,
        halfW,
        halfD,
        room: roomIndex
      });
    }

    for (const room of rooms) {
      room.baseline = roomBaseline(room.area, room.effectivePower);
    }

    // Walk-through openings join the rooms on either side of their wall.
    const blends = rooms.map(() => []);
    const blendSites = [];
    const lightSiteCount = lights.length;
    for (const wall of (level && level.walls) || []) {
      const axis = wallAxis(wall);
      const x = Number(wall.x) || 0;
      const z = Number(wall.z) || 0;
      const width = Number(wall.width) || 0;
      const depth = Number(wall.depth) || 0;
      const length = axis === 'x' ? Math.abs(width) : Math.abs(depth);
      if (!(length > 0)) continue;
      const originAlong = axis === 'x' ? Math.min(x, x + width) : Math.min(z, z + depth);
      const across0 = axis === 'x' ? Math.min(z, z + depth) : Math.min(x, x + width);
      const across1 = axis === 'x' ? Math.max(z, z + depth) : Math.max(x, x + width);
      const halfThickness = Math.abs(across1 - across0) * 0.5;
      const baseY = Number(wall.y) || 0;
      for (const opening of wall.openings || []) {
        const kind = opening.kind || 'door';
        const sill = Number(opening.sill) || 0;
        if ((kind !== 'door' && kind !== 'passage') || sill > 1e-3) continue;
        const offset = Number(opening.offset) || 0;
        const openingWidth = Number(opening.width) || 0;
        const openingHeight = Number(opening.height) || 0;
        // The same guards the game applies (src/lighting.rs): only openings the
        // geometry actually cuts blend light, and a wall raised off the floor is
        // a header rather than a walk-through.
        if (![offset, openingWidth, openingHeight, sill].every(Number.isFinite)) continue;
        if (!(openingWidth > 0) || !(openingHeight > 0)) continue;
        const center = Math.min(Math.max(offset + openingWidth * 0.5, 0), length);
        const across = (across0 + across1) * 0.5;
        const probe = halfThickness + OPENING_PROBE_M;
        const along = originAlong + center;
        const centerX = axis === 'x' ? along : across;
        const centerZ = axis === 'x' ? across : along;
        const sideA = axis === 'x' ? [centerX, centerZ + probe] : [centerX + probe, centerZ];
        const sideB = axis === 'x' ? [centerX, centerZ - probe] : [centerX - probe, centerZ];
        const roomA = roomIndexAt(sideA[0], sideA[1]);
        const roomB = roomIndexAt(sideB[0], sideB[1]);
        if (roomA < 0 || roomB < 0 || roomA === roomB) continue;
        const topY = baseY + sill + openingHeight;
        const floor = Math.min(rooms[roomA].floorY, rooms[roomB].floorY);
        // A walk-through opening has to reach the floor it connects: a wall
        // raised off the floor is a header or lintel, not a passage.
        if (baseY + Math.max(sill, 0) > floor + 1e-3) continue;
        const site = lightSiteCount + blendSites.length;
        blendSites.push({ x: centerX, z: centerZ, radius: TUNING.OPENING_BLEND_RADIUS_M });
        blends[roomA].push({ x: centerX, z: centerZ, baseY: floor, topY, site, neighborBaseline: rooms[roomB].baseline });
        blends[roomB].push({ x: centerX, z: centerZ, baseY: floor, topY, site, neighborBaseline: rooms[roomA].baseline });
      }
    }

    // Static wall visibility: mirrors src/lighting/visibility.rs. Every opaque
    // patch of every wall becomes a world-space box; a light only reaches a
    // surface whose connecting segment crosses no box, and a doorway only lets
    // light through the hole it actually cuts.
    const { occludes, containsPoint } = buildVisibility(level, rooms, lights, blendSites);

    /** Moves a surface sample out of a wall it lies inside, toward its room. */
    function clearSample(roomIndex, x, z) {
      if (!containsPoint(x, z)) return [x, z];
      const info = rooms[roomIndex];
      if (!info) return [x, z];
      const targetX = (info.x0 + info.x1) * 0.5;
      const targetZ = (info.z0 + info.z1) * 0.5;
      const deltaX = targetX - x;
      const deltaZ = targetZ - z;
      const distance = Math.sqrt(deltaX * deltaX + deltaZ * deltaZ);
      if (!(distance > ROOM_EDGE_EPS_M)) return [x, z];
      for (let step = 1; step <= CLEAR_SAMPLE_MAX_STEPS; step++) {
        const walked = step * CLEAR_SAMPLE_STEP_M;
        if (walked > distance) break;
        const t = walked / distance;
        const probeX = x + deltaX * t;
        const probeZ = z + deltaZ * t;
        if (!containsPoint(probeX, probeZ)) return [probeX, probeZ];
      }
      return [targetX, targetZ];
    }

    function localLight(x, y, z) {
      if (!isFiniteNumber(x) || !isFiniteNumber(y) || !isFiniteNumber(z)) return [0, 0, 0];
      const sum = [0, 0, 0];
      for (let i = 0; i < lights.length; i++) {
        const light = lights[i];
        const dx = Math.max(Math.abs(x - light.x) - light.halfW, 0);
        const dz = Math.max(Math.abs(z - light.z) - light.halfD, 0);
        const horizontal = Math.sqrt(dx * dx + dz * dz);
        if (!(horizontal < TUNING.LOCAL_LIGHT_RADIUS_M)) continue;
        const vertical = y - light.y;
        const distance = Math.sqrt(horizontal * horizontal + vertical * vertical);
        if (!(distance < TUNING.LOCAL_LIGHT_RADIUS_M)) continue;
        // The segment starts at the closest point of the panel, exactly like
        // `LevelLighting::local_light` in the game.
        const source = [
          Math.min(Math.max(x, light.x - light.halfW), light.x + light.halfW),
          light.y,
          Math.min(Math.max(z, light.z - light.halfD), light.z + light.halfD)
        ];
        if (occludes(i, source, [x, y, z])) continue;
        const strength = TUNING.LOCAL_LIGHT_STRENGTH * light.intensity * light.heightFactor
          * smoothFalloff(distance / TUNING.LOCAL_LIGHT_RADIUS_M);
        sum[0] += strength * light.color[0];
        sum[1] += strength * light.color[1];
        sum[2] += strength * light.color[2];
        if (Math.min(sum[0], sum[1], sum[2]) >= TUNING.LOCAL_LIGHT_MAX) {
          return [TUNING.LOCAL_LIGHT_MAX, TUNING.LOCAL_LIGHT_MAX, TUNING.LOCAL_LIGHT_MAX];
        }
      }
      return clampColor(sum, 0, TUNING.LOCAL_LIGHT_MAX);
    }

    /**
     * The doorway-blend part of `sampleInRoom`, isolated for the parity
     * vectors and the doorway tests.
     */
    function openingBlend(roomIndex, x, y, z) {
      if (!(roomIndex >= 0) || roomIndex >= rooms.length) return [0, 0, 0];
      if (!isFiniteNumber(x) || !isFiniteNumber(y) || !isFiniteNumber(z)) return [0, 0, 0];
      const [clearX, clearZ] = clearSample(roomIndex, x, z);
      return blendDelta(roomIndex, clearX, y, clearZ);
    }

    function blendDelta(roomIndex, x, y, z) {
      const info = rooms[roomIndex];
      const delta = [0, 0, 0];
      const list = blends[roomIndex];
      for (let i = 0; i < list.length; i++) {
        const blend = list[i];
        const dx = x - blend.x;
        const dz = z - blend.z;
        const distance = Math.sqrt(dx * dx + dz * dz);
        if (!(distance < TUNING.OPENING_BLEND_RADIUS_M)) continue;
        const source = [blend.x, (blend.baseY + blend.topY) * 0.5, blend.z];
        if (occludes(blend.site, source, [x, y, z])) continue;
        let influence = TUNING.OPENING_BLEND_STRENGTH
          * smoothFalloff(distance / TUNING.OPENING_BLEND_RADIUS_M);
        if (y > blend.topY) {
          influence *= smoothFalloff((y - blend.topY) / TUNING.OPENING_VERTICAL_FADE_M);
        }
        if (!(influence > 0)) continue;
        for (let channel = 0; channel < 3; channel++) {
          delta[channel] += (blend.neighborBaseline[channel] - info.baseline[channel]) * influence;
        }
      }
      return delta;
    }

    function sampleInRoom(roomIndex, x, y, z) {
      if (roomIndex < 0 || roomIndex >= rooms.length) return sample(x, y, z);
      if (!isFiniteNumber(x) || !isFiniteNumber(y) || !isFiniteNumber(z)) return ambientColor();
      const info = rooms[roomIndex];
      const [clearX, clearZ] = clearSample(roomIndex, x, z);
      const local = localLight(clearX, y, clearZ);
      const delta = blendDelta(roomIndex, clearX, y, clearZ);
      const value = [
        info.baseline[0] + local[0] + delta[0],
        info.baseline[1] + local[1] + delta[1],
        info.baseline[2] + local[2] + delta[2]
      ];
      if (!Number.isFinite(value[0]) || !Number.isFinite(value[1]) || !Number.isFinite(value[2])) {
        return ambientColor();
      }
      return clampColor(value, TUNING.AMBIENT_LEVEL, TUNING.MAX_BRIGHTNESS);
    }

    function sample(x, y, z) {
      const roomIndex = roomIndexAt(x, z);
      if (roomIndex < 0) {
        const local = localLight(x, y, z);
        return clampColor(
          [local[0] + TUNING.AMBIENT_LEVEL, local[1] + TUNING.AMBIENT_LEVEL, local[2] + TUNING.AMBIENT_LEVEL],
          TUNING.AMBIENT_LEVEL,
          TUNING.MAX_BRIGHTNESS
        );
      }
      return sampleInRoom(roomIndex, x, y, z);
    }

    function summary() {
      if (rooms.length === 0) {
        return { rooms: 0, lights: lights.length, minBaseline: 0, maxBaseline: 0, averageBaseline: 0 };
      }
      let min = Infinity;
      let max = -Infinity;
      let total = 0;
      for (const room of rooms) {
        const value = luminance(room.baseline);
        min = Math.min(min, value);
        max = Math.max(max, value);
        total += value;
      }
      return {
        rooms: rooms.length,
        lights: lights.length,
        minBaseline: min,
        maxBaseline: max,
        averageBaseline: total / rooms.length
      };
    }

    return {
      rooms,
      lights,
      roomIndexAt,
      sampleInRoom,
      openingBlend,
      sample,
      summary,
      /** World Y of the fixture panel at (x, z). */
      fixtureY(x, z) {
        const roomIndex = roomIndexAt(x, z);
        const height = roomIndex >= 0 ? rooms[roomIndex].height : fallbackHeight;
        return height - TUNING.FIXTURE_DROP_M;
      }
    };
  }

  return {
    TUNING,
    ambientColor,
    sanitizeIntensity,
    sanitizeColor,
    emittedColor,
    luminance,
    ceilingHeightFactor,
    saturatingBrightness,
    compressedDensity,
    smoothFalloff,
    fixtureIsTurned,
    fixtureHalfExtents,
    roomBaseline,
    bakeLevelLighting
  };
});
