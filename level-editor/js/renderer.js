// renderer.js - 2D plan renderer for the Liminal Level Editor.
//
// Deliberately quiet: floors, walls (with their openings), lights, props and the
// player spawn, plus selection feedback. Heavy debug decoration (coordinate labels,
// compass, height badges) is either gone or only shown in Advanced mode.

class Renderer {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');

    // Camera: world metres -> CSS pixels.
    this.cameraX = 0;
    this.cameraZ = 0;
    this.zoom = 28;
    this.minZoom = 2;
    this.maxZoom = 200;

    this.showGrid = true;
    this.showHeightBadges = false; // Advanced only
    this.dpr = window.devicePixelRatio || 1;
    this._glow = null;

    this.resize();
  }

  resize() {
    const rect = this.canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.floor(rect.width));
    const height = Math.max(1, Math.floor(rect.height));
    this.dpr = dpr;
    this.canvas.width = Math.max(1, Math.floor(width * dpr));
    this.canvas.height = Math.max(1, Math.floor(height * dpr));
    this.viewWidth = width;
    this.viewHeight = height;
  }

  get isVisible() {
    return this.canvas.width > 1 && this.canvas.height > 1 && this.canvas.offsetParent !== null;
  }

  // ------------------------------------------------------------ coordinates

  worldToScreen(wx, wz) {
    return {
      x: this.viewWidth / 2 + (wx - this.cameraX) * this.zoom,
      y: this.viewHeight / 2 + (wz - this.cameraZ) * this.zoom
    };
  }

  screenToWorld(sx, sy) {
    return {
      x: this.cameraX + (sx - this.viewWidth / 2) / this.zoom,
      z: this.cameraZ + (sy - this.viewHeight / 2) / this.zoom
    };
  }

  worldDistToScreen(d) { return d * this.zoom; }
  screenDistToWorld(d) { return d / this.zoom; }

  fitToGeometry(level) {
    let minX = -10, maxX = 10, minZ = -10, maxZ = 10;
    let found = false;

    const expand = (x, z, w, d) => {
      const x0 = Math.min(x, x + w), x1 = Math.max(x, x + w);
      const z0 = Math.min(z, z + d), z1 = Math.max(z, z + d);
      if (!found) {
        minX = x0; maxX = x1; minZ = z0; maxZ = z1; found = true;
      } else {
        minX = Math.min(minX, x0); maxX = Math.max(maxX, x1);
        minZ = Math.min(minZ, z0); maxZ = Math.max(maxZ, z1);
      }
    };

    for (const r of level.rooms) expand(r.x, r.z, r.width, r.depth);
    for (const w of level.walls) expand(w.x, w.z, w.width, w.depth);
    for (const l of level.ceiling_lights) expand(l.x - 0.6, l.z - 0.6, 1.2, 1.2);
    for (const p of level.props) expand(p.x - 0.6, p.z - 0.6, 1.2, 1.2);
    if (level.spawn) expand(level.spawn.x - 1, level.spawn.z - 1, 2, 2);

    const pad = 2;
    minX -= pad; maxX += pad; minZ -= pad; maxZ += pad;
    this.cameraX = (minX + maxX) / 2;
    this.cameraZ = (minZ + maxZ) / 2;
    const zoomX = this.viewWidth / Math.max(0.001, maxX - minX);
    const zoomZ = this.viewHeight / Math.max(0.001, maxZ - minZ);
    this.zoom = Math.max(this.minZoom, Math.min(80, Math.min(zoomX, zoomZ)));
  }

  /** Centres the view on an object without changing the zoom level. */
  centerOn(bounds, zoomToFit) {
    if (!bounds) return;
    this.cameraX = bounds.x + bounds.width / 2;
    this.cameraZ = bounds.z + bounds.depth / 2;
    if (zoomToFit) {
      const zoomX = this.viewWidth / Math.max(0.5, bounds.width * 3);
      const zoomZ = this.viewHeight / Math.max(0.5, bounds.depth * 3);
      this.zoom = Math.max(this.minZoom, Math.min(60, Math.min(zoomX, zoomZ)));
    }
  }

  // ----------------------------------------------------------------- render

  render(level, editorState, options) {
    const opts = options || {};
    if (!this.isVisible) return;
    const ctx = this.ctx;
    const catalog = opts.catalog;
    const defaultCeiling = level.getCeilingHeight();

    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    ctx.fillStyle = '#101216';
    ctx.fillRect(0, 0, this.viewWidth, this.viewHeight);

    if (this.showGrid) this.drawGrid();

    this.drawRooms(level, editorState);
    this.drawFloorPatches(level);
    this.drawWalls(level, editorState, defaultCeiling);
    this.drawOpenings(level, editorState);
    this.drawLights(level, editorState);
    this.drawProps(level, editorState, catalog);
    this.drawSpawn(level, editorState);
    this.drawPreview(editorState, defaultCeiling);
    this.drawSelection(level, editorState, catalog, defaultCeiling);
    if (editorState.marqueeBox) this.drawMarquee(editorState.marqueeBox);
  }

  drawGrid() {
    const ctx = this.ctx;
    const topLeft = this.screenToWorld(0, 0);
    const bottomRight = this.screenToWorld(this.viewWidth, this.viewHeight);

    const step = this.zoom > 40 ? 0.5 : this.zoom > 16 ? 1 : this.zoom > 6 ? 5 : 20;
    const startX = Math.floor(topLeft.x / step) * step;
    const startZ = Math.floor(topLeft.z / step) * step;

    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.045)';
    for (let x = startX; x <= bottomRight.x; x += step) {
      const sx = Math.round(this.worldToScreen(x, 0).x) + 0.5;
      ctx.moveTo(sx, 0);
      ctx.lineTo(sx, this.viewHeight);
    }
    for (let z = startZ; z <= bottomRight.z; z += step) {
      const sy = Math.round(this.worldToScreen(0, z).y) + 0.5;
      ctx.moveTo(0, sy);
      ctx.lineTo(this.viewWidth, sy);
    }
    ctx.stroke();

    // Every 5th line a little brighter so distances stay readable.
    const major = step * 5;
    ctx.beginPath();
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.08)';
    for (let x = Math.floor(topLeft.x / major) * major; x <= bottomRight.x; x += major) {
      const sx = Math.round(this.worldToScreen(x, 0).x) + 0.5;
      ctx.moveTo(sx, 0);
      ctx.lineTo(sx, this.viewHeight);
    }
    for (let z = Math.floor(topLeft.z / major) * major; z <= bottomRight.z; z += major) {
      const sy = Math.round(this.worldToScreen(0, z).y) + 0.5;
      ctx.moveTo(0, sy);
      ctx.lineTo(this.viewWidth, sy);
    }
    ctx.stroke();

    const origin = this.worldToScreen(0, 0);
    ctx.beginPath();
    ctx.strokeStyle = 'rgba(76, 194, 255, 0.16)';
    if (origin.x >= 0 && origin.x <= this.viewWidth) {
      ctx.moveTo(Math.round(origin.x) + 0.5, 0);
      ctx.lineTo(Math.round(origin.x) + 0.5, this.viewHeight);
    }
    if (origin.y >= 0 && origin.y <= this.viewHeight) {
      ctx.moveTo(0, Math.round(origin.y) + 0.5);
      ctx.lineTo(this.viewWidth, Math.round(origin.y) + 0.5);
    }
    ctx.stroke();
  }

  // ------------------------------------------------------------- materials

  /** RGBA fill for a material id, mixed towards the dark plan-view tone. */
  materialFill(id, alpha, mix) {
    const material = CORE_MATERIALS[id];
    if (!material) return `rgba(70, 76, 82, ${alpha})`;
    const [r, g, b] = [1, 3, 5].map(i => parseInt(material.color.slice(i, i + 2), 16));
    const blend = mix === undefined ? 0.45 : mix;
    const base = [46, 50, 56];
    const out = [r, g, b].map((channel, index) => Math.round(channel * blend + base[index] * (1 - blend)));
    return `rgba(${out[0]}, ${out[1]}, ${out[2]}, ${alpha})`;
  }

  drawRooms(level, editorState) {
    const ctx = this.ctx;
    level.rooms.forEach((room, index) => {
      const selected = editorState.selectedIds.has(room.id);
      const s = this.worldToScreen(room.x, room.z);
      const w = this.worldDistToScreen(room.width);
      const h = this.worldDistToScreen(room.depth);

      ctx.fillStyle = selected
        ? 'rgba(58, 70, 82, 0.8)'
        : this.materialFill(room.material || level.defaults.floor, 0.72);
      ctx.fillRect(s.x, s.y, w, h);

      ctx.lineWidth = 1;
      ctx.setLineDash([5, 4]);
      ctx.strokeStyle = selected ? '#4cc2ff' : this.materialFill(room.ceiling_material || level.defaults.ceiling, 0.55, 0.7);
      ctx.strokeRect(s.x + 0.5, s.y + 0.5, w, h);
      ctx.setLineDash([]);

      if (this.zoom >= 14 && w > 70 && h > 24) {
        ctx.font = '11px ui-monospace, monospace';
        ctx.fillStyle = selected ? '#8fd6ff' : 'rgba(200, 208, 216, 0.7)';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText(`Room ${index + 1} · ${room.width.toFixed(1)}×${room.depth.toFixed(1)} m`, s.x + 7, s.y + 6);
      }
    });
  }

  drawFloorPatches(level) {
    const ctx = this.ctx;
    for (const patch of level.floor_patches) {
      const s = this.worldToScreen(patch.x, patch.z);
      const w = this.worldDistToScreen(patch.width);
      const h = this.worldDistToScreen(patch.depth);
      ctx.fillStyle = 'rgba(30, 34, 30, 0.75)';
      ctx.fillRect(s.x, s.y, w, h);
      ctx.lineWidth = 1;
      ctx.strokeStyle = 'rgba(120, 130, 110, 0.5)';
      ctx.strokeRect(s.x + 0.5, s.y + 0.5, w - 1, h - 1);
    }
  }

  drawWalls(level, editorState, defaultCeiling) {
    const ctx = this.ctx;
    for (const wall of level.walls) {
      const selected = editorState.selectedIds.has(wall.id);
      const rects = LiminalGeometry.wallSolidRects2D(wall, defaultCeiling);
      const raised = wall.y > 0.001;
      // Walls read as their material, so material choices are visible at a glance.
      const fill = raised
        ? this.materialFill(wall.material || level.defaults.wall, 0.85, 0.6)
        : this.materialFill(wall.material || level.defaults.wall, 1, 0.5);
      for (const rect of rects) {
        const s = this.worldToScreen(rect.x, rect.z);
        const w = Math.max(1, this.worldDistToScreen(rect.width));
        const h = Math.max(1, this.worldDistToScreen(rect.depth));
        ctx.fillStyle = selected ? '#5b6672' : fill;
        ctx.fillRect(s.x, s.y, w, h);
        ctx.lineWidth = 1;
        ctx.strokeStyle = selected ? '#4cc2ff' : 'rgba(226, 232, 238, 0.32)';
        ctx.strokeRect(s.x + 0.5, s.y + 0.5, Math.max(1, w - 1), Math.max(1, h - 1));
      }

      if (this.showHeightBadges && this.zoom >= 12) {
        const rect = LiminalGeometry.wallSolidRects2D(wall, defaultCeiling)[0];
        if (rect) {
          const s = this.worldToScreen(rect.x, rect.z);
          const height = LiminalGeometry.wallResolvedHeight(wall, defaultCeiling);
          ctx.font = '10px ui-monospace, monospace';
          ctx.fillStyle = 'rgba(220, 200, 140, 0.8)';
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          const label = wall.y > 0.001
            ? `y ${wall.y.toFixed(1)} · h ${height.toFixed(1)}`
            : wall.height === null ? 'full height' : `h ${height.toFixed(1)}`;
          ctx.fillText(label, this.worldToScreen(wall.x + wall.width / 2, wall.z + wall.depth / 2).x,
            this.worldToScreen(wall.x + wall.width / 2, wall.z + wall.depth / 2).y);
        }
      }
    }
  }

  drawOpenings(level, editorState) {
    const ctx = this.ctx;
    for (const wall of level.walls) {
      for (const opening of wall.openings) {
        const selected = editorState.selectedIds.has(opening.id);
        const hovered = editorState.hoverOpeningId === opening.id;
        const rect = LiminalOps.openingBounds2D(wall, opening);
        const s = this.worldToScreen(rect.x, rect.z);
        const w = this.worldDistToScreen(rect.width);
        const h = this.worldDistToScreen(rect.depth);
        const preset = LiminalGeometry.OPENING_KINDS[opening.kind] || LiminalGeometry.OPENING_KINDS.door;
        const color = `rgb(${preset.color.map(v => Math.round(v * 255)).join(',')})`;

        // A doorway reads as a gap in the wall; a window keeps its sill line.
        ctx.save();
        ctx.beginPath();
        ctx.rect(s.x, s.y, Math.max(1, w), Math.max(1, h));
        ctx.clip();
        ctx.fillStyle = 'rgba(16, 18, 22, 0.92)';
        ctx.fillRect(s.x, s.y, Math.max(1, w), Math.max(1, h));
        if (opening.sill > 0.01) {
          ctx.fillStyle = 'rgba(255, 255, 255, 0.14)';
          const along = LiminalGeometry.wallAxis(wall) === 'x';
          if (along) ctx.fillRect(s.x, s.y + h / 2 - 1, w, 2);
          else ctx.fillRect(s.x + w / 2 - 1, s.y, 2, h);
        }
        ctx.restore();

        ctx.lineWidth = selected || hovered ? 2 : 1.5;
        ctx.strokeStyle = selected ? '#4cc2ff' : color;
        ctx.strokeRect(s.x + 0.5, s.y + 0.5, Math.max(1, w - 1), Math.max(1, h - 1));

        if (this.zoom >= 18 && opening.width * this.zoom > 34) {
          ctx.font = '10px ui-monospace, monospace';
          ctx.fillStyle = color;
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          const label = opening.kind === 'door' ? 'door' : opening.kind;
          ctx.fillText(label, s.x + w / 2, s.y + h / 2);
        }
      }
    }
  }

  drawLights(level, editorState) {
    const ctx = this.ctx;
    if (!this._glow) {
      const size = 64;
      const canvas = document.createElement('canvas');
      canvas.width = size;
      canvas.height = size;
      const gctx = canvas.getContext('2d');
      const gradient = gctx.createRadialGradient(size / 2, size / 2, 1, size / 2, size / 2, size / 2);
      gradient.addColorStop(0, 'rgba(255, 248, 208, 0.5)');
      gradient.addColorStop(0.6, 'rgba(255, 240, 170, 0.12)');
      gradient.addColorStop(1, 'rgba(255, 240, 170, 0)');
      gctx.fillStyle = gradient;
      gctx.fillRect(0, 0, size, size);
      this._glow = canvas;
    }

    for (const light of level.ceiling_lights) {
      const selected = editorState.selectedIds.has(light.id);
      const s = this.worldToScreen(light.x, light.z);
      const turned = Math.round(light.rotation_degrees / 90) % 2 !== 0;
      const halfW = (turned ? 0.3 : 0.6) * this.zoom;
      const halfD = (turned ? 0.6 : 0.3) * this.zoom;
      const glow = Math.max(26, 3 * this.zoom);
      ctx.drawImage(this._glow, s.x - glow / 2, s.y - glow / 2, glow, glow);

      ctx.fillStyle = selected ? '#4cc2ff' : '#fdf6d0';
      ctx.fillRect(s.x - halfW, s.y - halfD, halfW * 2, halfD * 2);
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = selected ? '#4cc2ff' : '#e0c95a';
      ctx.strokeRect(s.x - halfW - 0.5, s.y - halfD - 0.5, halfW * 2 + 1, halfD * 2 + 1);
    }
  }

  drawProps(level, editorState, catalog) {
    const ctx = this.ctx;
    for (const prop of level.props) {
      const selected = editorState.selectedIds.has(prop.id);
      const entry = catalog && typeof catalog.get === 'function' ? catalog.get(prop.model) : null;
      const size = LiminalGeometry.propSize(prop, catalog);
      const s = this.worldToScreen(prop.x, prop.z);
      const w = this.worldDistToScreen(size[0]);
      const d = this.worldDistToScreen(size[2]);

      ctx.save();
      ctx.translate(s.x, s.y);
      ctx.rotate((prop.rotation_degrees * Math.PI) / 180);
      const color = entry
        ? `rgb(${entry.color.map(v => Math.round(v * 255)).join(',')})`
        : '#8a8a8a';
      ctx.fillStyle = selected ? 'rgba(76, 194, 255, 0.85)' : color;
      ctx.strokeStyle = selected ? '#d7f0ff' : 'rgba(20, 22, 26, 0.8)';
      ctx.lineWidth = 1.5;
      ctx.fillRect(-w / 2, -d / 2, w, d);
      ctx.strokeRect(-w / 2, -d / 2, w, d);
      if (this.zoom >= 22 && Math.max(w, d) > 26) {
        ctx.fillStyle = 'rgba(15, 17, 20, 0.85)';
        ctx.font = '9px ui-monospace, monospace';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        const label = (entry && entry.name) || prop.model;
        ctx.fillText(label.length > 14 ? label.slice(0, 13) + '…' : label, 0, 0);
      }
      ctx.restore();
    }
  }

  drawSpawn(level, editorState) {
    if (!level.spawn) return;
    const ctx = this.ctx;
    const spawn = level.spawn;
    const selected = editorState.selectedIds.has('spawn');
    const s = this.worldToScreen(spawn.x, spawn.z);
    const radius = Math.max(7, Math.min(16, this.worldDistToScreen(0.45)));

    ctx.beginPath();
    ctx.arc(s.x, s.y, radius, 0, Math.PI * 2);
    ctx.fillStyle = selected ? '#4cc2ff' : '#5bd18b';
    ctx.fill();
    ctx.lineWidth = 1.5;
    ctx.strokeStyle = 'rgba(12, 14, 16, 0.8)';
    ctx.stroke();

    // Facing arrow: yaw 0 points towards -Z (up in the plan view).
    const angle = ((spawn.yaw_degrees - 90) * Math.PI) / 180;
    const len = radius + 9;
    const ax = s.x + Math.cos(angle) * len;
    const ay = s.y + Math.sin(angle) * len;
    ctx.strokeStyle = '#eafaf1';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(s.x, s.y);
    ctx.lineTo(ax, ay);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(ax, ay);
    ctx.lineTo(ax - 5 * Math.cos(angle - 0.5), ay - 5 * Math.sin(angle - 0.5));
    ctx.lineTo(ax - 5 * Math.cos(angle + 0.5), ay - 5 * Math.sin(angle + 0.5));
    ctx.closePath();
    ctx.fillStyle = '#eafaf1';
    ctx.fill();
  }

  drawPreview(editorState, defaultCeiling) {
    const ctx = this.ctx;
    const preview = editorState.preview;
    if (!preview) return;

    if (preview.type === 'rect') {
      const p1 = preview.start;
      const p2 = preview.current;
      const minX = Math.min(p1.x, p2.x);
      const minZ = Math.min(p1.z, p2.z);
      const width = Math.abs(p2.x - p1.x);
      const depth = Math.abs(p2.z - p1.z);
      const s = this.worldToScreen(minX, minZ);
      const w = this.worldDistToScreen(width);
      const h = this.worldDistToScreen(depth);
      const color = preview.tool === 'room' ? '#4cc2ff' : preview.tool === 'patch' ? '#9ad17a' : '#e0d3a0';

      ctx.fillStyle = preview.tool === 'room' ? 'rgba(76, 194, 255, 0.12)' : 'rgba(255, 255, 255, 0.08)';
      ctx.fillRect(s.x, s.y, w, h);
      ctx.lineWidth = 1.5;
      ctx.setLineDash([4, 3]);
      ctx.strokeStyle = color;
      ctx.strokeRect(s.x, s.y, w, h);
      ctx.setLineDash([]);
      this.drawMeasure(s.x + w / 2, s.y + h + 14, `${width.toFixed(2)} × ${depth.toFixed(2)} m`, color);
    }

    if (preview.type === 'opening' && preview.rect) {
      const rect = preview.rect;
      const s = this.worldToScreen(rect.x, rect.z);
      const w = this.worldDistToScreen(rect.width);
      const h = this.worldDistToScreen(rect.depth);
      const preset = LiminalGeometry.OPENING_KINDS[preview.kind] || LiminalGeometry.OPENING_KINDS.door;
      const color = `rgb(${preset.color.map(v => Math.round(v * 255)).join(',')})`;
      ctx.fillStyle = 'rgba(255, 255, 255, 0.1)';
      ctx.fillRect(s.x, s.y, w, h);
      ctx.lineWidth = 2;
      ctx.strokeStyle = color;
      ctx.strokeRect(s.x, s.y, w, h);
      this.drawMeasure(s.x + w / 2, s.y + h + 14, `${preview.kind} ${preview.width.toFixed(2)} m`, color);
      void defaultCeiling;
    }
  }

  drawMeasure(x, y, text, color) {
    const ctx = this.ctx;
    ctx.font = '11px ui-monospace, monospace';
    const width = ctx.measureText(text).width;
    ctx.fillStyle = 'rgba(14, 16, 19, 0.9)';
    ctx.fillRect(x - width / 2 - 6, y - 9, width + 12, 18);
    ctx.fillStyle = color;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(text, x, y);
  }

  drawSelection(level, editorState, catalog, defaultCeiling) {
    const ctx = this.ctx;
    const ids = editorState.selectedIds;
    if (!ids || ids.size === 0) return;

    for (const id of ids) {
      const bounds = LiminalOps.objectBounds2D(level, id, catalog);
      if (!bounds) continue;
      const s = this.worldToScreen(bounds.x, bounds.z);
      const w = this.worldDistToScreen(bounds.width);
      const h = this.worldDistToScreen(bounds.depth);

      ctx.lineWidth = 1.5;
      ctx.strokeStyle = '#4cc2ff';
      ctx.setLineDash([6, 3]);
      ctx.strokeRect(s.x - 2, s.y - 2, w + 4, h + 4);
      ctx.setLineDash([]);

      if (ids.size === 1 && editorState.dragMode !== 'marquee') {
        this.drawHandles(s.x, s.y, w, h);
      }
    }
    void defaultCeiling;
  }

  drawHandles(x, y, w, h) {
    const ctx = this.ctx;
    const size = 6;
    const half = size / 2;
    const points = [
      [x, y], [x + w / 2, y], [x + w, y],
      [x + w, y + h / 2], [x + w, y + h],
      [x + w / 2, y + h], [x, y + h], [x, y + h / 2]
    ];
    ctx.fillStyle = '#f2f6fa';
    ctx.strokeStyle = '#1d6f9c';
    ctx.lineWidth = 1;
    for (const [px, py] of points) {
      ctx.fillRect(px - half, py - half, size, size);
      ctx.strokeRect(px - half + 0.5, py - half + 0.5, size - 1, size - 1);
    }
  }

  drawMarquee(box) {
    const ctx = this.ctx;
    const x = Math.min(box.x1, box.x2);
    const y = Math.min(box.y1, box.y2);
    const w = Math.abs(box.x2 - box.x1);
    const h = Math.abs(box.y2 - box.y1);
    ctx.fillStyle = 'rgba(76, 194, 255, 0.1)';
    ctx.fillRect(x, y, w, h);
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 3]);
    ctx.strokeStyle = '#4cc2ff';
    ctx.strokeRect(x + 0.5, y + 0.5, w, h);
    ctx.setLineDash([]);
  }
}
