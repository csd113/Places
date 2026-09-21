// renderer.js - HTML5 Canvas 2D Top-Down Rendering Engine for Liminal Level Editor

class Renderer {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');

    // Camera view transform
    this.cameraX = 0; // World X at center of view (meters)
    this.cameraZ = 0; // World Z at center of view (meters)
    this.zoom = 28;   // Pixels per meter (default: 28px = 1m)
    this.minZoom = 2;
    this.maxZoom = 200;

    // Display options
    this.showGrid = true;
    this.showHeightBadges = true;
    this.showRoomBounds = true;
    this.showLightGlow = true;

    // Cached hatch pattern for raised walls
    this.hatchPattern = this.createHatchPattern();

    // Resize canvas to match display size
    this.resize();
  }

  resize() {
    const rect = this.canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    this.canvas.width = Math.max(100, Math.floor(rect.width * dpr));
    this.canvas.height = Math.max(100, Math.floor(rect.height * dpr));
    this.dpr = dpr;
  }

  createHatchPattern() {
    const pCanvas = document.createElement('canvas');
    pCanvas.width = 16;
    pCanvas.height = 16;
    const pCtx = pCanvas.getContext('2d');
    pCtx.strokeStyle = 'rgba(234, 179, 8, 0.35)'; // Amber hatch
    pCtx.lineWidth = 2;
    pCtx.beginPath();
    pCtx.moveTo(0, 16);
    pCtx.lineTo(16, 0);
    pCtx.moveTo(-8, 8);
    pCtx.lineTo(8, -8);
    pCtx.moveTo(8, 24);
    pCtx.lineTo(24, 8);
    pCtx.stroke();
    return this.ctx.createPattern(pCanvas, 'repeat');
  }

  // Coordinate conversions
  worldToScreen(wx, wz) {
    const cx = this.canvas.width / (2 * this.dpr);
    const cy = this.canvas.height / (2 * this.dpr);
    return {
      x: cx + (wx - this.cameraX) * this.zoom,
      y: cy + (wz - this.cameraZ) * this.zoom
    };
  }

  screenToWorld(sx, sy) {
    const cx = this.canvas.width / (2 * this.dpr);
    const cy = this.canvas.height / (2 * this.dpr);
    return {
      x: this.cameraX + (sx - cx) / this.zoom,
      z: this.cameraZ + (sy - cy) / this.zoom
    };
  }

  worldDistToScreen(d) {
    return d * this.zoom;
  }

  screenDistToWorld(d) {
    return d / this.zoom;
  }

  // Focus view to enclose all geometry
  fitToGeometry(level) {
    let minX = -10, maxX = 10, minZ = -10, maxZ = 10;
    let found = false;

    const expand = (x, z, w, d) => {
      const x0 = Math.min(x, x + w);
      const x1 = Math.max(x, x + w);
      const z0 = Math.min(z, z + d);
      const z1 = Math.max(z, z + d);
      if (!found) {
        minX = x0; maxX = x1; minZ = z0; maxZ = z1;
        found = true;
      } else {
        minX = Math.min(minX, x0); maxX = Math.max(maxX, x1);
        minZ = Math.min(minZ, z0); maxZ = Math.max(maxZ, z1);
      }
    };

    if (level.rooms.length > 0) {
      level.rooms.forEach(r => expand(r.x, r.z, r.width, r.depth));
    }
    level.walls.forEach(w => expand(w.x, w.z, w.width, w.depth));
    level.ceiling_lights.forEach(l => expand(l.x - 0.6, l.z - 0.6, 1.2, 1.2));
    if (level.spawn) expand(level.spawn.x - 1, level.spawn.z - 1, 2, 2);

    const pad = 2; // meters padding
    minX -= pad; maxX += pad; minZ -= pad; maxZ += pad;

    this.cameraX = (minX + maxX) / 2;
    this.cameraZ = (minZ + maxZ) / 2;

    const viewW = this.canvas.width / this.dpr;
    const viewH = this.canvas.height / this.dpr;
    const zoomX = viewW / (maxX - minX || 1);
    const zoomZ = viewH / (maxZ - minZ || 1);
    this.zoom = Math.max(this.minZoom, Math.min(60, Math.min(zoomX, zoomZ)));
  }

  // Main render pass
  render(level, editorState) {
    const ctx = this.ctx;
    ctx.save();
    ctx.scale(this.dpr, this.dpr);

    const width = this.canvas.width / this.dpr;
    const height = this.canvas.height / this.dpr;

    // 1. Clear background (deep dark slate void)
    ctx.fillStyle = '#141417';
    ctx.fillRect(0, 0, width, height);

    // 2. Render Grid & Axes
    if (this.showGrid) {
      this.renderGrid(width, height);
    }

    // 3. Render Room floor slabs
    if (this.showRoomBounds) {
      this.renderRooms(level, editorState);
    }

    // 4. Render Floor Patches (e.g. damp carpet)
    this.renderFloorPatches(level);

    // 5. Render Ceiling Lights (under walls or over walls? Lights on ceiling, draw glow & fixtures)
    this.renderLights(level, editorState);

    // 6. Render Walls (with height & elevation styling)
    this.renderWalls(level, editorState);

    // 7. Render Player Spawn
    this.renderSpawn(level, editorState);

    // 8. Render Active Drawing Previews (drawing wall or placing objects)
    this.renderDrawingPreview(editorState);

    // 9. Render Selection Highlights & Resize Handles
    this.renderSelection(level, editorState);

    // 10. Render Marquee Box
    if (editorState.marqueeBox) {
      this.renderMarquee(editorState.marqueeBox);
    }

    // 11. Render On-Canvas Coordinate & Orientation Overlay (top-left)
    this.renderCompassOverlay(width, height);

    ctx.restore();
  }

  renderGrid(width, height) {
    const ctx = this.ctx;
    const topLeft = this.screenToWorld(0, 0);
    const bottomRight = this.screenToWorld(width, height);

    // Determine grid step based on zoom
    let minorStep = 1.0;
    let majorStep = 5.0;

    if (this.zoom > 50) {
      minorStep = 0.25;
      majorStep = 1.0;
    } else if (this.zoom > 25) {
      minorStep = 0.5;
      majorStep = 2.5;
    } else if (this.zoom > 10) {
      minorStep = 1.0;
      majorStep = 5.0;
    } else if (this.zoom > 4) {
      minorStep = 5.0;
      majorStep = 20.0;
    } else {
      minorStep = 10.0;
      majorStep = 50.0;
    }

    const startX = Math.floor(topLeft.x / minorStep) * minorStep;
    const endX = Math.ceil(bottomRight.x / minorStep) * minorStep;
    const startZ = Math.floor(topLeft.z / minorStep) * minorStep;
    const endZ = Math.ceil(bottomRight.z / minorStep) * minorStep;

    // Minor grid lines
    ctx.lineWidth = 1;
    ctx.strokeStyle = '#222328';
    ctx.beginPath();
    for (let x = startX; x <= endX; x += minorStep) {
      const sx = Math.round(this.worldToScreen(x, 0).x);
      ctx.moveTo(sx, 0);
      ctx.lineTo(sx, height);
    }
    for (let z = startZ; z <= endZ; z += minorStep) {
      const sy = Math.round(this.worldToScreen(0, z).y);
      ctx.moveTo(0, sy);
      ctx.lineTo(width, sy);
    }
    ctx.stroke();

    // Major grid lines
    ctx.lineWidth = 1;
    ctx.strokeStyle = '#2d2f36';
    ctx.beginPath();
    const startMajorX = Math.floor(topLeft.x / majorStep) * majorStep;
    const startMajorZ = Math.floor(topLeft.z / majorStep) * majorStep;
    for (let x = startMajorX; x <= endX; x += majorStep) {
      const sx = Math.round(this.worldToScreen(x, 0).x);
      ctx.moveTo(sx, 0);
      ctx.lineTo(sx, height);
    }
    for (let z = startMajorZ; z <= endZ; z += majorStep) {
      const sy = Math.round(this.worldToScreen(0, z).y);
      ctx.moveTo(0, sy);
      ctx.lineTo(width, sy);
    }
    ctx.stroke();

    // Origin Axes (X=0 Red-ish, Z=0 Blue-ish)
    const origin = this.worldToScreen(0, 0);
    ctx.lineWidth = 1.5;
    
    // Z-axis (Vertical on screen, Z=0 line is X-axis)
    // Actually: X=0 line is vertical, Z=0 line is horizontal
    if (origin.x >= 0 && origin.x <= width) {
      ctx.strokeStyle = 'rgba(78, 140, 255, 0.45)'; // Z axis (North/South)
      ctx.beginPath();
      ctx.moveTo(origin.x, 0);
      ctx.lineTo(origin.x, height);
      ctx.stroke();
    }

    if (origin.y >= 0 && origin.y <= height) {
      ctx.strokeStyle = 'rgba(255, 99, 71, 0.45)'; // X axis (West/East)
      ctx.beginPath();
      ctx.moveTo(0, origin.y);
      ctx.lineTo(width, origin.y);
      ctx.stroke();
    }

    // Grid coordinates markers along edges
    if (this.zoom >= 15) {
      ctx.font = '10px monospace';
      ctx.fillStyle = '#6b7280';
      ctx.textAlign = 'left';
      ctx.textBaseline = 'top';
      for (let x = startMajorX; x <= endX; x += majorStep) {
        const sx = this.worldToScreen(x, 0).x;
        if (sx >= 10 && sx <= width - 40) {
          ctx.fillText(`${x}m`, sx + 3, 4);
        }
      }
      for (let z = startMajorZ; z <= endZ; z += majorStep) {
        const sy = this.worldToScreen(0, z).y;
        if (sy >= 15 && sy <= height - 20) {
          ctx.fillText(`${z}m`, 4, sy + 3);
        }
      }
    }
  }

  renderRooms(level, editorState) {
    const ctx = this.ctx;
    level.rooms.forEach((r, idx) => {
      const isSelected = editorState && editorState.selectedIds && editorState.selectedIds.has(r.id);
      const s = this.worldToScreen(r.x, r.z);
      const w = this.worldDistToScreen(r.width);
      const h = this.worldDistToScreen(r.depth);

      // Floor slab tone (deep warm beige carpet floor)
      ctx.fillStyle = isSelected ? 'rgba(65, 55, 42, 0.85)' : 'rgba(42, 38, 32, 0.75)';
      ctx.fillRect(s.x, s.y, w, h);

      // Ceiling boundary / perimeter indicator (light neutral dashed tint)
      ctx.lineWidth = 1;
      ctx.setLineDash([6, 4]);
      ctx.strokeStyle = isSelected ? '#38bdf8' : '#6b5e4a';
      ctx.strokeRect(s.x, s.y, w, h);
      ctx.setLineDash([]);

      // Subtle corner accents to indicate ceiling slab corners
      const cLen = Math.min(12, Math.min(w, h) * 0.2);
      ctx.lineWidth = 2;
      ctx.strokeStyle = isSelected ? '#38bdf8' : 'rgba(213, 213, 206, 0.4)';
      // NW corner
      ctx.beginPath();
      ctx.moveTo(s.x, s.y + cLen); ctx.lineTo(s.x, s.y); ctx.lineTo(s.x + cLen, s.y);
      // NE corner
      ctx.moveTo(s.x + w - cLen, s.y); ctx.lineTo(s.x + w, s.y); ctx.lineTo(s.x + w, s.y + cLen);
      // SE corner
      ctx.moveTo(s.x + w, s.y + h - cLen); ctx.lineTo(s.x + w, s.y + h); ctx.lineTo(s.x + w - cLen, s.y + h);
      // SW corner
      ctx.moveTo(s.x + cLen, s.y + h); ctx.lineTo(s.x, s.y + h); ctx.lineTo(s.x, s.y + h - cLen);
      ctx.stroke();

      // Room information badge
      if (this.zoom >= 16) {
        ctx.font = '10px monospace';
        ctx.fillStyle = isSelected ? '#38bdf8' : '#9c8e76';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        const label = `Floor/Ceiling ${idx + 1} [${r.width.toFixed(1)}m × ${r.depth.toFixed(1)}m | H:${r.height.toFixed(1)}m]`;
        ctx.fillText(label, s.x + 8, s.y + 8);
      }
    });
  }

  renderFloorPatches(level) {
    const ctx = this.ctx;
    level.floor_patches.forEach(p => {
      const s = this.worldToScreen(p.x, p.z);
      const w = this.worldDistToScreen(p.width);
      const h = this.worldDistToScreen(p.depth);

      ctx.fillStyle = 'rgba(28, 25, 20, 0.85)'; // Dark damp carpet patch
      ctx.fillRect(s.x, s.y, w, h);

      ctx.lineWidth = 1.5;
      ctx.strokeStyle = '#423a2e';
      ctx.strokeRect(s.x, s.y, w, h);

      if (this.zoom >= 22) {
        ctx.font = '9px monospace';
        ctx.fillStyle = '#7a705e';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('damp carpet', s.x + w / 2, s.y + h / 2);
      }
    });
  }

  renderWalls(level, editorState) {
    const ctx = this.ctx;
    const defaultCeiling = level.getCeilingHeight();

    level.walls.forEach(w => {
      const isSelected = editorState.selectedIds.has(w.id);
      const s = this.worldToScreen(w.x, w.z);
      const sw = this.worldDistToScreen(w.width);
      const sd = this.worldDistToScreen(w.depth);

      const wallHeight = w.getResolvedHeight(defaultCeiling);
      const isRaised = w.isRaised();
      const isHalf = w.isHalfHeight(defaultCeiling);
      const isFull = w.isFullHeight(defaultCeiling);

      // Determine fill style based on height & elevation
      if (isRaised) {
        // Raised wall segment (e.g. door header, high window top)
        ctx.fillStyle = 'rgba(40, 36, 28, 0.85)';
        ctx.fillRect(s.x, s.y, sw, sd);
        ctx.fillStyle = this.hatchPattern;
        ctx.fillRect(s.x, s.y, sw, sd);

        ctx.lineWidth = 1.5;
        ctx.setLineDash([3, 3]);
        ctx.strokeStyle = '#eab308'; // Amber dashed
        ctx.strokeRect(s.x, s.y, sw, sd);
        ctx.setLineDash([]);
      } else if (isHalf) {
        // Half-height wall (e.g. window sill, low partition)
        ctx.fillStyle = '#3a4449'; // Slate/teal tint
        ctx.fillRect(s.x, s.y, sw, sd);

        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#67e8f9'; // Cyan border
        ctx.strokeRect(s.x, s.y, sw, sd);
      } else {
        // Standard full-height wall
        ctx.fillStyle = '#262420'; // Solid dark slate/charcoal
        ctx.fillRect(s.x, s.y, sw, sd);

        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#c4a65c'; // Warm wallpaper amber outline
        ctx.strokeRect(s.x, s.y, sw, sd);
      }

      // Height / Elevation label badge
      if (this.showHeightBadges && this.zoom >= 16) {
        let badgeText = '';
        let badgeColor = '#c4a65c';

        if (isRaised) {
          badgeText = `▲ Y:${w.y.toFixed(1)} H:${wallHeight.toFixed(1)}`;
          badgeColor = '#eab308';
        } else if (isHalf) {
          badgeText = `H:${wallHeight.toFixed(1)}m`;
          badgeColor = '#67e8f9';
        } else if (wallHeight !== 3.5 && this.zoom >= 25) {
          badgeText = `H:${wallHeight.toFixed(1)}m`;
        }

        if (badgeText) {
          ctx.font = '10px monospace';
          ctx.fillStyle = badgeColor;
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          // Draw text only if space permits or offset slightly
          if (sw > 30 && sd > 14) {
            ctx.fillText(badgeText, s.x + sw / 2, s.y + sd / 2);
          } else if (sw > 30) {
            ctx.fillText(badgeText, s.x + sw / 2, s.y - 7);
          } else if (sd > 30) {
            ctx.save();
            ctx.translate(s.x + sw / 2, s.y + sd / 2);
            ctx.rotate(-Math.PI / 2);
            ctx.fillText(badgeText, 0, 0);
            ctx.restore();
          }
        }
      }
    });
  }

  renderLights(level, editorState) {
    const ctx = this.ctx;
    level.ceiling_lights.forEach(l => {
      const isSelected = editorState.selectedIds.has(l.id);
      const s = this.worldToScreen(l.x, l.z);

      // Light dimensions in liminal-rust
      // if rot % 180 != 0 => (0.6, 1.2), else (1.2, 0.6)
      const rot = Math.round(l.rotation_degrees) % 360;
      const isRotated = (rot % 180 !== 0);
      const halfW = isRotated ? 0.30 : 0.60;
      const halfD = isRotated ? 0.60 : 0.30;

      const sw = this.worldDistToScreen(halfW * 2);
      const sd = this.worldDistToScreen(halfD * 2);
      const lx = s.x - sw / 2;
      const ly = s.y - sd / 2;

      // Glow circle
      if (this.showLightGlow) {
        const glowRad = Math.max(16, this.worldDistToScreen(3.0));
        const gradient = ctx.createRadialGradient(s.x, s.y, 2, s.x, s.y, glowRad);
        gradient.addColorStop(0, 'rgba(255, 253, 232, 0.22)');
        gradient.addColorStop(0.5, 'rgba(254, 240, 138, 0.08)');
        gradient.addColorStop(1, 'rgba(254, 240, 138, 0)');
        ctx.fillStyle = gradient;
        ctx.beginPath();
        ctx.arc(s.x, s.y, glowRad, 0, Math.PI * 2);
        ctx.fill();
      }

      // Fixture bezel
      ctx.fillStyle = '#2b2a26';
      ctx.fillRect(lx, ly, sw, sd);

      // Fixture tube / fluorescent panel
      ctx.fillStyle = '#fffde8';
      const pad = Math.max(1, Math.min(sw, sd) * 0.15);
      ctx.fillRect(lx + pad, ly + pad, sw - pad * 2, sd - pad * 2);

      // Fixture border
      ctx.lineWidth = 1;
      ctx.strokeStyle = '#eab308';
      ctx.strokeRect(lx, ly, sw, sd);

      // Center cross / orientation indicator
      if (sw > 16 && sd > 16) {
        ctx.fillStyle = '#d97706';
        ctx.fillRect(s.x - 1, s.y - 1, 2, 2);
      }
    });
  }

  renderSpawn(level, editorState) {
    if (!level.spawn) return;
    const ctx = this.ctx;
    const s = this.worldToScreen(level.spawn.x, level.spawn.z);
    const isSelected = editorState.selectedIds.has('spawn');

    const radius = Math.max(10, Math.min(22, this.worldDistToScreen(0.45)));

    // Outer glow / halo
    ctx.beginPath();
    ctx.arc(s.x, s.y, radius + 4, 0, Math.PI * 2);
    ctx.fillStyle = isSelected ? 'rgba(56, 189, 248, 0.35)' : 'rgba(74, 222, 128, 0.25)';
    ctx.fill();

    // Main circular body (vivid green)
    ctx.beginPath();
    ctx.arc(s.x, s.y, radius, 0, Math.PI * 2);
    ctx.fillStyle = isSelected ? '#38bdf8' : '#22c55e';
    ctx.fill();
    ctx.lineWidth = 2;
    ctx.strokeStyle = '#ffffff';
    ctx.stroke();

    // Direction arrow (indicates player yaw)
    // Game orientation: yaw = 0 points towards -Z (North / Up on 2D map)
    // Yaw = 90 points towards +X (East / Right)
    // Angle in screen coordinates: -PI/2 is Up, 0 is Right
    const angleRad = (level.spawn.yaw_degrees - 90) * Math.PI / 180;

    const arrowLen = radius + 8;
    const ax = s.x + Math.cos(angleRad) * arrowLen;
    const ay = s.y + Math.sin(angleRad) * arrowLen;

    ctx.lineWidth = 3;
    ctx.strokeStyle = '#ffffff';
    ctx.beginPath();
    ctx.moveTo(s.x, s.y);
    ctx.lineTo(ax, ay);
    ctx.stroke();

    // Arrowhead
    const headLen = 6;
    const headAngle = Math.PI / 6;
    ctx.fillStyle = '#ffffff';
    ctx.beginPath();
    ctx.moveTo(ax, ay);
    ctx.lineTo(ax - headLen * Math.cos(angleRad - headAngle), ay - headLen * Math.sin(angleRad - headAngle));
    ctx.lineTo(ax - headLen * Math.cos(angleRad + headAngle), ay - headLen * Math.sin(angleRad + headAngle));
    ctx.closePath();
    ctx.fill();

    // Label
    if (this.zoom >= 18) {
      ctx.font = 'bold 10px monospace';
      ctx.fillStyle = '#4ade80';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'top';
      ctx.fillText(`SPAWN (${level.spawn.yaw_degrees.toFixed(0)}°)`, s.x, s.y + radius + 6);
    }
  }

  renderSelection(level, editorState) {
    const ctx = this.ctx;
    const selectedIds = editorState.selectedIds;
    if (selectedIds.size === 0) return;

    const defaultCeiling = level.getCeilingHeight();

    // Collect bounds of all selected items for multi-selection
    selectedIds.forEach(id => {
      let rect = null;

      if (id === 'spawn') {
        const s = this.worldToScreen(level.spawn.x, level.spawn.z);
        const rad = Math.max(12, this.worldDistToScreen(0.55));
        rect = { x: s.x - rad, y: s.y - rad, w: rad * 2, h: rad * 2 };
      } else {
        const wall = level.walls.find(w => w.id === id);
        if (wall) {
          const s = this.worldToScreen(wall.x, wall.z);
          rect = { x: s.x, y: s.y, w: this.worldDistToScreen(wall.width), h: this.worldDistToScreen(wall.depth) };
        } else {
          const light = level.ceiling_lights.find(l => l.id === id);
          if (light) {
            const s = this.worldToScreen(light.x, light.z);
            const isRot = (Math.round(light.rotation_degrees) % 180 !== 0);
            const hw = isRot ? 0.3 : 0.6;
            const hd = isRot ? 0.6 : 0.3;
            rect = {
              x: s.x - this.worldDistToScreen(hw),
              y: s.y - this.worldDistToScreen(hd),
              w: this.worldDistToScreen(hw * 2),
              h: this.worldDistToScreen(hd * 2)
            };
          } else {
            const room = level.rooms.find(r => r.id === id);
            if (room) {
              const s = this.worldToScreen(room.x, room.z);
              rect = { x: s.x, y: s.y, w: this.worldDistToScreen(room.width), h: this.worldDistToScreen(room.depth) };
            }
          }
        }
      }

      if (rect) {
        // Selection outline
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#38bdf8'; // Luminous Cyan
        ctx.strokeRect(rect.x - 2, rect.y - 2, rect.w + 4, rect.h + 4);

        // If only 1 rectangular object is selected, draw 8 resize handles
        if (selectedIds.size === 1 && id !== 'spawn') {
          this.renderResizeHandles(rect);
        }
      }
    });
  }

  renderResizeHandles(rect) {
    const ctx = this.ctx;
    const handleSize = 7;
    const half = handleSize / 2;

    const handles = [
      { x: rect.x, y: rect.y },                            // NW
      { x: rect.x + rect.w / 2, y: rect.y },              // N
      { x: rect.x + rect.w, y: rect.y },                  // NE
      { x: rect.x + rect.w, y: rect.y + rect.h / 2 },      // E
      { x: rect.x + rect.w, y: rect.y + rect.h },          // SE
      { x: rect.x + rect.w / 2, y: rect.y + rect.h },      // S
      { x: rect.x, y: rect.y + rect.h },                  // SW
      { x: rect.x, y: rect.y + rect.h / 2 }               // W
    ];

    ctx.fillStyle = '#ffffff';
    ctx.strokeStyle = '#0284c7';
    ctx.lineWidth = 1.5;

    handles.forEach(h => {
      ctx.fillRect(h.x - half, h.y - half, handleSize, handleSize);
      ctx.strokeRect(h.x - half, h.y - half, handleSize, handleSize);
    });
  }

  renderDrawingPreview(editorState) {
    if (!editorState.isDrawing || !editorState.drawingStart || !editorState.drawingCurrent) {
      return;
    }

    const ctx = this.ctx;
    const p1 = editorState.drawingStart;
    const p2 = editorState.drawingCurrent;

    const minX = Math.min(p1.x, p2.x);
    const maxX = Math.max(p1.x, p2.x);
    const minZ = Math.min(p1.z, p2.z);
    const maxZ = Math.max(p1.z, p2.z);

    const widthM = maxX - minX;
    const depthM = maxZ - minZ;

    const s = this.worldToScreen(minX, minZ);
    const sw = this.worldDistToScreen(widthM);
    const sd = this.worldDistToScreen(depthM);

    const tool = editorState.currentTool;
    let toolLabel = 'Wall';
    let fillColor = 'rgba(56, 189, 248, 0.15)';
    let strokeColor = '#38bdf8';

    if (tool === 'floor') {
      toolLabel = 'Floor Slab';
      fillColor = 'rgba(234, 179, 8, 0.20)';
      strokeColor = '#facc15';
    } else if (tool === 'ceiling') {
      toolLabel = 'Ceiling Section';
      fillColor = 'rgba(148, 163, 184, 0.22)';
      strokeColor = '#cbd5e1';
    } else if (tool === 'column') {
      toolLabel = 'Column';
    }

    // Fill with dashed accent
    ctx.fillStyle = fillColor;
    ctx.fillRect(s.x, s.y, sw, sd);

    ctx.lineWidth = 2;
    ctx.setLineDash([4, 4]);
    ctx.strokeStyle = strokeColor;
    ctx.strokeRect(s.x, s.y, sw, sd);
    ctx.setLineDash([]);

    // Measurement badge
    const badge = `${toolLabel}: ${widthM.toFixed(2)}m × ${depthM.toFixed(2)}m`;
    ctx.font = 'bold 11px monospace';
    const textW = ctx.measureText(badge).width;

    ctx.fillStyle = '#0f172a';
    ctx.fillRect(s.x + sw / 2 - textW / 2 - 6, s.y + sd + 6, textW + 12, 20);
    ctx.strokeStyle = strokeColor;
    ctx.lineWidth = 1;
    ctx.strokeRect(s.x + sw / 2 - textW / 2 - 6, s.y + sd + 6, textW + 12, 20);

    ctx.fillStyle = strokeColor;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(badge, s.x + sw / 2, s.y + sd + 16);
  }

  renderMarquee(box) {
    const ctx = this.ctx;
    const minX = Math.min(box.x1, box.x2);
    const maxX = Math.max(box.x1, box.x2);
    const minY = Math.min(box.y1, box.y2);
    const maxY = Math.max(box.y1, box.y2);

    ctx.fillStyle = 'rgba(56, 189, 248, 0.12)';
    ctx.fillRect(minX, minY, maxX - minX, maxY - minY);

    ctx.lineWidth = 1;
    ctx.setLineDash([4, 3]);
    ctx.strokeStyle = '#38bdf8';
    ctx.strokeRect(minX, minY, maxX - minX, maxY - minY);
    ctx.setLineDash([]);
  }

  renderCompassOverlay(width, height) {
    const ctx = this.ctx;
    const cx = width - 40;
    const cy = 40;

    // Small compass rose showing world axes (+X East, -Z North)
    ctx.save();
    ctx.beginPath();
    ctx.arc(cx, cy, 22, 0, Math.PI * 2);
    ctx.fillStyle = 'rgba(20, 20, 24, 0.75)';
    ctx.fill();
    ctx.lineWidth = 1;
    ctx.strokeStyle = '#374151';
    ctx.stroke();

    // North arrow (-Z)
    ctx.fillStyle = '#ef4444'; // Red for North
    ctx.beginPath();
    ctx.moveTo(cx, cy - 16);
    ctx.lineTo(cx - 4, cy - 2);
    ctx.lineTo(cx + 4, cy - 2);
    ctx.closePath();
    ctx.fill();

    // East arrow (+X)
    ctx.fillStyle = '#60a5fa'; // Blue for East
    ctx.beginPath();
    ctx.moveTo(cx + 16, cy);
    ctx.lineTo(cx + 2, cy - 4);
    ctx.lineTo(cx + 2, cy + 4);
    ctx.closePath();
    ctx.fill();

    ctx.font = 'bold 9px monospace';
    ctx.fillStyle = '#ef4444';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'bottom';
    ctx.fillText('N', cx, cy - 17);

    ctx.fillStyle = '#60a5fa';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    ctx.fillText('E', cx + 18, cy);

    ctx.restore();
  }
}
