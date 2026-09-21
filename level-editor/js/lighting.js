// lighting.js - Static baked-lighting mirror for the editor's 3D preview.
//
// The game (`src/lighting.rs`) is authoritative: this module only exists so the
// editor preview can approximate the same room brightness while an author edits.
// It implements the same model with the same tuned constants:
//
//   * room baseline  = MIN_AMBIENT + (MAX_BRIGHTNESS - MIN_AMBIENT) * d / (1 + d)
//     where d = (sum of intensity x height factor) / floor area, normalised by
//     one fixture per REFERENCE_LIGHT_AREA_M2 square metres;
//   * broad local fixture pools with a smooth falloff;
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
    REFERENCE_LIGHT_AREA_M2: 8.0,
    REFERENCE_CEILING_HEIGHT_M: 3.5,
    HEIGHT_FALLOFF: 0.5,
    MIN_AMBIENT: 0.55,
    MAX_BRIGHTNESS: 1.0,
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

  function isFiniteNumber(value) {
    return typeof value === 'number' && Number.isFinite(value);
  }

  /** Sanitises an authored fixture intensity exactly like the game does. */
  function sanitizeIntensity(intensity) {
    const value = Number(intensity);
    if (Number.isNaN(value)) return 1.0;
    if (!Number.isFinite(value)) return value > 0 ? TUNING.MAX_LIGHT_INTENSITY : 0.0;
    return Math.min(Math.max(value, 0.0), TUNING.MAX_LIGHT_INTENSITY);
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

  /** Baseline brightness of a room from its floor area and effective fixture power. */
  function roomBaseline(areaM2, effectivePower) {
    const area = isFiniteNumber(areaM2) ? Math.max(areaM2, 0.01) : 0.01;
    const power = isFiniteNumber(effectivePower) ? Math.max(effectivePower, 0.0) : 0.0;
    const normalized = (power / area) * TUNING.REFERENCE_LIGHT_AREA_M2;
    const component = saturatingBrightness(normalized);
    const baseline = TUNING.MIN_AMBIENT + (TUNING.MAX_BRIGHTNESS - TUNING.MIN_AMBIENT) * component;
    return Math.min(Math.max(baseline, TUNING.MIN_AMBIENT), TUNING.MAX_BRIGHTNESS);
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
      return {
        id: room.id,
        x0: bounds.x0,
        x1: bounds.x1,
        z0: bounds.z0,
        z1: bounds.z1,
        height: isFiniteNumber(rawHeight) && rawHeight > 0 ? rawHeight : TUNING.REFERENCE_CEILING_HEIGHT_M,
        area: width * depth,
        fixtureCount: 0,
        effectivePower: 0,
        baseline: TUNING.MIN_AMBIENT
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
      if (roomIndex >= 0) {
        rooms[roomIndex].fixtureCount += 1;
        rooms[roomIndex].effectivePower += intensity * heightFactor;
      }
      lights.push({
        x,
        z,
        y: height - TUNING.FIXTURE_DROP_M,
        intensity,
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
        if (!(openingWidth > 0)) continue;
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
        const topY = baseY + sill + (Number(opening.height) || 0);
        blends[roomA].push({ x: centerX, z: centerZ, topY, neighborBaseline: rooms[roomB].baseline });
        blends[roomB].push({ x: centerX, z: centerZ, topY, neighborBaseline: rooms[roomA].baseline });
      }
    }

    function localLight(x, y, z) {
      if (!isFiniteNumber(x) || !isFiniteNumber(y) || !isFiniteNumber(z)) return 0;
      let sum = 0;
      for (let i = 0; i < lights.length; i++) {
        const light = lights[i];
        const dx = Math.max(Math.abs(x - light.x) - light.halfW, 0);
        const dz = Math.max(Math.abs(z - light.z) - light.halfD, 0);
        const horizontal = Math.sqrt(dx * dx + dz * dz);
        if (horizontal >= TUNING.LOCAL_LIGHT_RADIUS_M) continue;
        const vertical = y - light.y;
        const distance = Math.sqrt(horizontal * horizontal + vertical * vertical);
        if (distance >= TUNING.LOCAL_LIGHT_RADIUS_M) continue;
        sum += TUNING.LOCAL_LIGHT_STRENGTH * light.intensity * light.heightFactor
          * smoothFalloff(distance / TUNING.LOCAL_LIGHT_RADIUS_M);
        if (sum >= TUNING.LOCAL_LIGHT_MAX) return TUNING.LOCAL_LIGHT_MAX;
      }
      return Math.min(sum, TUNING.LOCAL_LIGHT_MAX);
    }

    function sampleInRoom(roomIndex, x, y, z) {
      if (roomIndex < 0 || roomIndex >= rooms.length) return sample(x, y, z);
      if (!isFiniteNumber(x) || !isFiniteNumber(y) || !isFiniteNumber(z)) return TUNING.MIN_AMBIENT;
      const info = rooms[roomIndex];
      let value = info.baseline + localLight(x, y, z);
      const list = blends[roomIndex];
      for (let i = 0; i < list.length; i++) {
        const blend = list[i];
        const dx = x - blend.x;
        const dz = z - blend.z;
        const distance = Math.sqrt(dx * dx + dz * dz);
        if (distance >= TUNING.OPENING_BLEND_RADIUS_M) continue;
        let influence = TUNING.OPENING_BLEND_STRENGTH
          * smoothFalloff(distance / TUNING.OPENING_BLEND_RADIUS_M);
        if (y > blend.topY) {
          influence *= smoothFalloff((y - blend.topY) / TUNING.OPENING_VERTICAL_FADE_M);
        }
        value += (blend.neighborBaseline - info.baseline) * influence;
      }
      if (!Number.isFinite(value)) return TUNING.MIN_AMBIENT;
      return Math.min(Math.max(value, TUNING.MIN_AMBIENT), TUNING.MAX_BRIGHTNESS);
    }

    function sample(x, y, z) {
      const roomIndex = roomIndexAt(x, z);
      if (roomIndex < 0) {
        const value = TUNING.MIN_AMBIENT + localLight(x, y, z);
        return Math.min(Math.max(value, TUNING.MIN_AMBIENT), TUNING.MAX_BRIGHTNESS);
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
        min = Math.min(min, room.baseline);
        max = Math.max(max, room.baseline);
        total += room.baseline;
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
    sanitizeIntensity,
    ceilingHeightFactor,
    saturatingBrightness,
    smoothFalloff,
    fixtureIsTurned,
    fixtureHalfExtents,
    roomBaseline,
    bakeLevelLighting
  };
});
