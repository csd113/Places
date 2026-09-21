// model.js - Data structures, core materials, and validation for Liminal 2D Level Editor

const CORE_MATERIALS = {
  'core:wallpaper_yellow_01': {
    id: 'core:wallpaper_yellow_01',
    name: 'Yellow Wallpaper',
    category: 'wall',
    color: '#b89c56',
    border: '#8e7436'
  },
  'core:wallpaper_stained_01': {
    id: 'core:wallpaper_stained_01',
    name: 'Stained Wallpaper',
    category: 'wall',
    color: '#8c733e',
    border: '#604c24'
  },
  'core:carpet_beige_01': {
    id: 'core:carpet_beige_01',
    name: 'Beige Carpet',
    category: 'floor',
    color: '#7a6f58',
    border: '#574d3a'
  },
  'core:carpet_damp_01': {
    id: 'core:carpet_damp_01',
    name: 'Damp Carpet',
    category: 'floor',
    color: '#474032',
    border: '#2c271e'
  },
  'core:ceiling_panel_01': {
    id: 'core:ceiling_panel_01',
    name: 'Ceiling Panel',
    category: 'ceiling',
    color: '#d5d5ce',
    border: '#9a9a92'
  },
  'core:fluorescent_panel_01': {
    id: 'core:fluorescent_panel_01',
    name: 'Fluorescent Panel Light',
    category: 'fixture',
    color: '#fffde8',
    border: '#e6c84b'
  }
};

// Generates procedural thumbnails matching the game's built-in textures
const CORE_THUMBNAILS = {};
function initCoreThumbnails() {
  const size = 32;
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext('2d');

  // 1. Yellow Wallpaper
  ctx.fillStyle = '#c4a65c';
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = '#a68a44';
  for (let x = 0; x < size; x += 4) {
    ctx.fillRect(x, 0, 1.5, size);
  }
  CORE_THUMBNAILS['core:wallpaper_yellow_01'] = canvas.toDataURL();

  // 2. Stained Wallpaper
  ctx.fillStyle = '#8f743c';
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = '#59441f';
  for (let x = 0; x < size; x += 4) {
    ctx.fillRect(x, 0, 1.5, size);
  }
  ctx.fillStyle = 'rgba(40, 28, 10, 0.45)';
  ctx.beginPath();
  ctx.arc(16, 20, 10, 0, Math.PI * 2);
  ctx.fill();
  CORE_THUMBNAILS['core:wallpaper_stained_01'] = canvas.toDataURL();

  // 3. Beige Carpet
  ctx.fillStyle = '#7a6f58';
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = '#615743';
  for (let i = 0; i < 120; i++) {
    const rx = (i * 17) % size;
    const ry = (i * 23) % size;
    ctx.fillRect(rx, ry, 1, 1);
  }
  CORE_THUMBNAILS['core:carpet_beige_01'] = canvas.toDataURL();

  // 4. Damp Carpet
  ctx.fillStyle = '#484032';
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = '#312a1f';
  for (let i = 0; i < 150; i++) {
    const rx = (i * 13) % size;
    const ry = (i * 29) % size;
    ctx.fillRect(rx, ry, 1, 1);
  }
  ctx.fillStyle = 'rgba(20, 20, 15, 0.5)';
  ctx.beginPath();
  ctx.ellipse(16, 16, 12, 8, Math.PI / 4, 0, Math.PI * 2);
  ctx.fill();
  CORE_THUMBNAILS['core:carpet_damp_01'] = canvas.toDataURL();

  // 5. Ceiling Panel
  ctx.fillStyle = '#d5d5ce';
  ctx.fillRect(0, 0, size, size);
  ctx.strokeStyle = '#9a9a92';
  ctx.lineWidth = 1;
  ctx.strokeRect(1, 1, size - 2, size - 2);
  ctx.strokeRect(size / 2, 0, 0, size);
  ctx.strokeRect(0, size / 2, size, 0);
  CORE_THUMBNAILS['core:ceiling_panel_01'] = canvas.toDataURL();

  // 6. Fluorescent Panel Light
  ctx.fillStyle = '#333333';
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = '#fffde8';
  ctx.fillRect(4, 8, size - 8, size - 16);
  ctx.strokeStyle = '#e6c84b';
  ctx.lineWidth = 1;
  ctx.strokeRect(4, 8, size - 8, size - 16);
  CORE_THUMBNAILS['core:fluorescent_panel_01'] = canvas.toDataURL();
}

let nextIdCounter = 1;
function generateUniqueId(prefix) {
  return `${prefix}_${Date.now().toString(36)}_${(nextIdCounter++).toString(36)}`;
}

class Wall {
  constructor(data = {}) {
    this.id = data.id || generateUniqueId('wall');
    this.x = Number(data.x ?? 0);
    this.y = Number(data.y ?? 0);
    this.z = Number(data.z ?? 0);
    this.width = Number(data.width ?? 1);
    this.depth = Number(data.depth ?? 0.35);
    // height: null/undefined represents default room ceiling height
    this.height = data.height !== undefined && data.height !== null ? Number(data.height) : null;
    this.faces = data.faces ? { ...data.faces } : {};
    this.material = data.material || null; // optional primary wall material shorthand
  }

  isFullHeight(ceilingHeight = 3.5) {
    return this.y === 0 && (this.height === null || this.height >= ceilingHeight);
  }

  isRaised() {
    return this.y > 0.001;
  }

  isHalfHeight(ceilingHeight = 3.5) {
    return this.y === 0 && this.height !== null && this.height < ceilingHeight;
  }

  getResolvedHeight(defaultCeiling = 3.5) {
    return this.height !== null && this.height !== undefined ? this.height : defaultCeiling;
  }

  clone() {
    return new Wall({
      id: generateUniqueId('wall'),
      x: this.x,
      y: this.y,
      z: this.z,
      width: this.width,
      depth: this.depth,
      height: this.height,
      faces: { ...this.faces },
      material: this.material
    });
  }

  toJSON() {
    const obj = {
      x: Number(this.x.toFixed(3)),
      z: Number(this.z.toFixed(3)),
      width: Number(this.width.toFixed(3)),
      depth: Number(this.depth.toFixed(3))
    };
    if (this.y !== 0) {
      obj.y = Number(this.y.toFixed(3));
    }
    if (this.height !== null && this.height !== undefined) {
      obj.height = Number(this.height.toFixed(3));
    }
    const faces = {};
    if (this.faces) {
      for (const [k, v] of Object.entries(this.faces)) {
        if (v && v.trim()) faces[k] = v.trim();
      }
    }
    if (Object.keys(faces).length > 0) {
      obj.faces = faces;
    }
    return obj;
  }
}

class CeilingLight {
  constructor(data = {}) {
    this.id = data.id || generateUniqueId('light');
    this.fixture = data.fixture || 'core:fluorescent_panel_01';
    this.x = Number(data.x ?? 0);
    this.z = Number(data.z ?? 0);
    this.rotation_degrees = Number(data.rotation_degrees ?? 0);
    this.brightness = data.brightness !== undefined && data.brightness !== null ? Number(data.brightness) : 1.0;
  }

  clone() {
    return new CeilingLight({
      id: generateUniqueId('light'),
      fixture: this.fixture,
      x: this.x,
      z: this.z,
      rotation_degrees: this.rotation_degrees,
      brightness: this.brightness
    });
  }

  toJSON() {
    const obj = {
      fixture: this.fixture,
      x: Number(this.x.toFixed(3)),
      z: Number(this.z.toFixed(3)),
      rotation_degrees: Number(this.rotation_degrees.toFixed(1))
    };
    if (this.brightness !== null && this.brightness !== undefined && this.brightness !== 1.0) {
      obj.brightness = Number(this.brightness.toFixed(2));
    }
    return obj;
  }
}

class Spawn {
  constructor(data = {}) {
    this.x = Number(data.x ?? 0);
    this.z = Number(data.z ?? 0);
    this.yaw_degrees = Number(data.yaw_degrees ?? 0);
  }

  clone() {
    return new Spawn({
      x: this.x,
      z: this.z,
      yaw_degrees: this.yaw_degrees
    });
  }

  toJSON() {
    return {
      x: Number(this.x.toFixed(3)),
      z: Number(this.z.toFixed(3)),
      yaw_degrees: Number(this.yaw_degrees.toFixed(1))
    };
  }
}

class Room {
  constructor(data = {}) {
    this.id = data.id || generateUniqueId('room');
    this.x = Number(data.x ?? -10);
    this.z = Number(data.z ?? -10);
    this.width = Number(data.width ?? 20);
    this.depth = Number(data.depth ?? 20);
    this.height = Number(data.height ?? 3.5);
    // Optional per-room surface material overrides if specified
    this.material = data.material || null;
    this.ceiling_material = data.ceiling_material || null;
  }

  clone() {
    return new Room({
      id: generateUniqueId('room'),
      x: this.x,
      z: this.z,
      width: this.width,
      depth: this.depth,
      height: this.height,
      material: this.material,
      ceiling_material: this.ceiling_material
    });
  }

  toJSON() {
    const obj = {
      x: Number(this.x.toFixed(3)),
      z: Number(this.z.toFixed(3)),
      width: Number(this.width.toFixed(3)),
      depth: Number(this.depth.toFixed(3)),
      height: Number(this.height.toFixed(3))
    };
    if (this.material) obj.material = this.material;
    if (this.ceiling_material) obj.ceiling_material = this.ceiling_material;
    return obj;
  }
}

class FloorPatch {
  constructor(data = {}) {
    this.id = data.id || generateUniqueId('patch');
    this.x = Number(data.x ?? 0);
    this.z = Number(data.z ?? 0);
    this.width = Number(data.width ?? 2);
    this.depth = Number(data.depth ?? 2);
    this.material = data.material || 'core:carpet_damp_01';
  }

  clone() {
    return new FloorPatch({
      id: generateUniqueId('patch'),
      x: this.x,
      z: this.z,
      width: this.width,
      depth: this.depth,
      material: this.material
    });
  }

  toJSON() {
    return {
      x: Number(this.x.toFixed(3)),
      z: Number(this.z.toFixed(3)),
      width: Number(this.width.toFixed(3)),
      depth: Number(this.depth.toFixed(3)),
      material: this.material
    };
  }
}

class LevelDefaults {
  constructor(data = {}) {
    this.wall = data.wall || 'core:wallpaper_yellow_01';
    this.floor = data.floor || 'core:carpet_beige_01';
    this.ceiling = data.ceiling || 'core:ceiling_panel_01';
  }

  clone() {
    return new LevelDefaults({ ...this });
  }

  toJSON() {
    return {
      wall: this.wall,
      floor: this.floor,
      ceiling: this.ceiling
    };
  }
}

class Level {
  constructor(data = {}) {
    this.format_version = data.format_version || 1;
    this.id = data.id || 'new_level';
    this.name = data.name || 'New Level';
    this.author = data.author || '';
    this.spawn = new Spawn(data.spawn || { x: 0, z: 0, yaw_degrees: 0 });
    this.defaults = new LevelDefaults(data.defaults || {});
    
    // Support rooms array or single room
    this.rooms = [];
    if (data.rooms && Array.isArray(data.rooms) && data.rooms.length > 0) {
      this.rooms = data.rooms.map(r => new Room(r));
    } else if (data.room) {
      this.rooms = [new Room(data.room)];
    } else {
      this.rooms = [new Room({ x: -10, z: -10, width: 20, depth: 20, height: 3.5 })];
    }

    this.walls = (data.walls || []).map(w => new Wall(w));
    this.ceiling_lights = (data.ceiling_lights || []).map(l => new CeilingLight(l));
    this.floor_patches = (data.floor_patches || []).map(p => new FloorPatch(p));

    // Custom textures map: material_id -> { filename, dataUrl, width, height, bytes }
    this.custom_textures = {};
    if (data.custom_textures) {
      for (const [k, v] of Object.entries(data.custom_textures)) {
        this.custom_textures[k] = { ...v };
      }
    }
  }

  getCeilingHeight(x = 0, z = 0) {
    for (const r of this.rooms) {
      const minX = Math.min(r.x, r.x + r.width);
      const maxX = Math.max(r.x, r.x + r.width);
      const minZ = Math.min(r.z, r.z + r.depth);
      const maxZ = Math.max(r.z, r.z + r.depth);
      if (x >= minX - 0.01 && x <= maxX + 0.01 && z >= minZ - 0.01 && z <= maxZ + 0.01) {
        return r.height;
      }
    }
    return this.rooms[0]?.height || 3.5;
  }

  clone() {
    return new Level({
      format_version: this.format_version,
      id: this.id,
      name: this.name,
      author: this.author,
      spawn: this.spawn.clone(),
      defaults: this.defaults.clone(),
      rooms: this.rooms.map(r => r.clone()),
      walls: this.walls.map(w => w.clone()),
      ceiling_lights: this.ceiling_lights.map(l => l.clone()),
      floor_patches: this.floor_patches.map(p => p.clone()),
      custom_textures: JSON.parse(JSON.stringify(this.custom_textures))
    });
  }

  toJSON() {
    const result = {
      format_version: this.format_version,
      id: this.id,
      name: this.name,
      author: this.author,
      spawn: this.spawn.toJSON(),
      defaults: this.defaults.toJSON()
    };

    if (this.rooms.length === 1) {
      result.room = this.rooms[0].toJSON();
    } else {
      result.rooms = this.rooms.map(r => r.toJSON());
    }

    result.walls = this.walls.map(w => w.toJSON());

    if (this.floor_patches.length > 0) {
      result.floor_patches = this.floor_patches.map(p => p.toJSON());
    }

    result.ceiling_lights = this.ceiling_lights.map(l => l.toJSON());

    return result;
  }
}

// Level Validation logic mirroring liminal-rust loader.rs validate_level
function validateLevel(level) {
  const errors = [];
  const warnings = [];

  // 1. Format version
  if (level.format_version !== 1) {
    errors.push(`Unsupported format_version: ${level.format_version} (expected 1)`);
  }

  // 2. Identity
  if (!level.id || !level.id.trim()) {
    errors.push("Level 'id' cannot be empty");
  } else if (!/^[a-zA-Z0-9_-]+$/.test(level.id.trim())) {
    warnings.push("Level 'id' should use only letters, numbers, underscores, and hyphens");
  }

  if (!level.name || !level.name.trim()) {
    errors.push("Level 'name' cannot be empty");
  }

  // 3. Player Spawn
  if (!level.spawn) {
    errors.push("Player spawn definition is missing");
  } else {
    if (!Number.isFinite(level.spawn.x) || !Number.isFinite(level.spawn.z)) {
      errors.push("Player spawn coordinates must be finite numbers");
    }
    if (!Number.isFinite(level.spawn.yaw_degrees)) {
      errors.push("Player spawn yaw_degrees must be a finite number");
    }

    // Check if spawn is inside room bounds
    if (level.rooms.length > 0) {
      let insideAnyRoom = false;
      for (const r of level.rooms) {
        const x0 = Math.min(r.x, r.x + r.width);
        const x1 = Math.max(r.x, r.x + r.width);
        const z0 = Math.min(r.z, r.z + r.depth);
        const z1 = Math.max(r.z, r.z + r.depth);
        if (level.spawn.x >= x0 && level.spawn.x <= x1 && level.spawn.z >= z0 && level.spawn.z <= z1) {
          insideAnyRoom = true;
          break;
        }
      }
      if (!insideAnyRoom) {
        warnings.push("Player spawn point is outside all room boundaries");
      }
    }
  }

  // 4. Geometry limits (liminal-rust limits)
  if (level.rooms.length > 500) {
    errors.push(`Level contains too many rooms: ${level.rooms.length} (limit: 500)`);
  }
  if (level.walls.length > 5000) {
    errors.push(`Level contains too many walls: ${level.walls.length} (limit: 5000)`);
  }
  if (level.ceiling_lights.length > 5000) {
    errors.push(`Level contains too many ceiling lights: ${level.ceiling_lights.length} (limit: 5000)`);
  }

  level.rooms.forEach((r, i) => {
    if (!Number.isFinite(r.width) || !Number.isFinite(r.depth) || !Number.isFinite(r.height)) {
      errors.push(`Room ${i} dimensions must be finite numbers`);
    } else {
      if (r.width <= 0 || r.depth <= 0 || r.height <= 0) {
        errors.push(`Room ${i} width, depth, and height must be positive numbers`);
      }
      if (r.width > 2000 || r.depth > 2000 || r.height > 50) {
        errors.push(`Room ${i} dimensions exceed maximum limits (max 2000x2000x50m)`);
      }
    }
  });

  level.walls.forEach((w, i) => {
    if (!Number.isFinite(w.x) || !Number.isFinite(w.y) || !Number.isFinite(w.z) || !Number.isFinite(w.width) || !Number.isFinite(w.depth)) {
      errors.push(`Wall ${i} position/dimensions must be finite numbers`);
    } else {
      if (w.width <= 0 || w.depth <= 0) {
        errors.push(`Wall ${i} width and depth must be positive numbers (> 0)`);
      }
      if (w.height !== null && w.height !== undefined) {
        if (!Number.isFinite(w.height)) {
          errors.push(`Wall ${i} height must be a finite number`);
        } else if (w.height <= 0) {
          errors.push(`Wall ${i} height must be positive (> 0)`);
        }
      }
    }
  });

  // 5. Materials validation
  const knownMaterials = new Set(Object.keys(CORE_MATERIALS));
  if (level.custom_textures) {
    for (const id of Object.keys(level.custom_textures)) {
      knownMaterials.add(id);
    }
  }

  const checkMaterial = (matId, context) => {
    if (!matId) return;
    if (matId.startsWith('pack:') && !knownMaterials.has(matId)) {
      warnings.push(`Missing texture reference: "${matId}" used in ${context}`);
    } else if (!matId.startsWith('core:') && !matId.startsWith('pack:')) {
      warnings.push(`Unrecognized material namespace: "${matId}" in ${context} (expected "core:" or "pack:")`);
    }
  };

  checkMaterial(level.defaults.wall, 'defaults.wall');
  checkMaterial(level.defaults.floor, 'defaults.floor');
  checkMaterial(level.defaults.ceiling, 'defaults.ceiling');

  level.walls.forEach((w, i) => {
    if (w.material) checkMaterial(w.material, `Wall ${i}`);
    if (w.faces) {
      for (const [face, mat] of Object.entries(w.faces)) {
        checkMaterial(mat, `Wall ${i} face "${face}"`);
      }
    }
  });

  level.ceiling_lights.forEach((l, i) => {
    checkMaterial(l.fixture, `Ceiling light ${i}`);
  });

  level.floor_patches.forEach((p, i) => {
    checkMaterial(p.material, `Floor patch ${i}`);
  });

  // 6. Texture dimensions check
  if (level.custom_textures) {
    for (const [id, tex] of Object.entries(level.custom_textures)) {
      if (tex.width > 1024 || tex.height > 1024) {
        errors.push(`Custom texture "${id}" dimensions ${tex.width}x${tex.height} exceed engine limit of 1024x1024`);
      }
    }
  }

  return {
    valid: errors.length === 0,
    errors,
    warnings
  };
}

// Global initialization
if (typeof window !== 'undefined') {
  initCoreThumbnails();
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    CORE_MATERIALS,
    Wall,
    CeilingLight,
    Spawn,
    Room,
    FloorPatch,
    LevelDefaults,
    Level,
    validateLevel
  };
}
