// app.js - Main Controller and Application Orchestration for Liminal Level Editor

class App {
  constructor() {
    this.canvas = document.getElementById('editor-canvas');
    this.statusTool = document.getElementById('status-tool');
    this.statusCoords = document.getElementById('status-coords');
    this.statusZoom = document.getElementById('status-zoom');
    this.statusSnap = document.getElementById('status-snap');
    this.statusSelection = document.getElementById('status-selection');
    this.statusMsg = document.getElementById('status-msg');

    // Create default level
    this.level = this.createDefaultLevel();

    // Subsystems
    this.renderer = new Renderer(this.canvas);
    this.history = new HistoryManager();
    this.editor = new Editor(this.canvas, this);
    this.propertiesPanel = new PropertiesPanel(document.getElementById('properties-container'), this);
    this.io = new LevelIO(this);

    // Initial view fit
    this.renderer.fitToGeometry(this.level);

    // Initial history state
    this.history.pushState(this.level, 'Initial Level');

    this.bindToolbar();
    this.bindKeyboardShortcuts();
    this.io.setupDragAndDrop();

    // Listen to history changes to update undo/redo buttons
    this.history.onChange((canUndo, canRedo, lastAction) => {
      const btnUndo = document.getElementById('btn-undo');
      const btnRedo = document.getElementById('btn-redo');
      if (btnUndo) btnUndo.disabled = !canUndo;
      if (btnRedo) btnRedo.disabled = !canRedo;
    });

    this.updateStatus('Editor ready. Draw walls or import existing level.');
    this.requestRender();
  }

  createDefaultLevel() {
    const level = new Level({
      format_version: 1,
      id: 'custom_room_01',
      name: 'Custom Room',
      author: 'Creator',
      spawn: { x: 0.0, z: 0.0, yaw_degrees: 0.0 },
      defaults: {
        wall: 'core:wallpaper_yellow_01',
        floor: 'core:carpet_beige_01',
        ceiling: 'core:ceiling_panel_01'
      },
      room: {
        x: -10.0,
        z: -10.0,
        width: 20.0,
        depth: 20.0,
        height: 3.5
      },
      walls: [
        // Perimeter North wall with doorway
        { x: -10.0, z: -10.0, width: 8.5, depth: 0.35, height: 3.5 },
        // Doorway overhead header (raised wall segment at y: 2.2, height: 1.3)
        { x: -1.5, z: -10.0, width: 3.0, depth: 0.35, y: 2.2, height: 1.3 },
        { x: 1.5, z: -10.0, width: 8.5, depth: 0.35, height: 3.5 },

        // Perimeter South wall
        { x: -10.0, z: 9.65, width: 20.0, depth: 0.35, height: 3.5 },

        // Perimeter West wall
        { x: -10.0, z: -10.0, width: 0.35, depth: 20.0, height: 3.5 },

        // Perimeter East wall with window opening (sill + header)
        { x: 9.65, z: -10.0, width: 0.35, depth: 8.0, height: 3.5 },
        // Window sill (y: 0.0, height: 1.0)
        { x: 9.65, z: -2.0, width: 0.35, depth: 4.0, y: 0.0, height: 1.0 },
        // Window header (y: 2.5, height: 1.0)
        { x: 9.65, z: -2.0, width: 0.35, depth: 4.0, y: 2.5, height: 1.0 },
        { x: 9.65, z: 2.0, width: 0.35, depth: 8.0, height: 3.5 },

        // Interior Column pillars
        { x: -4.5, z: -4.5, width: 1.0, depth: 1.0, height: 3.5 },
        { x: 3.5, z: -4.5, width: 1.0, depth: 1.0, height: 3.5 },
        { x: -4.5, z: 3.5, width: 1.0, depth: 1.0, height: 3.5 },
        { x: 3.5, z: 3.5, width: 1.0, depth: 1.0, height: 3.5 }
      ],
      ceiling_lights: [
        { fixture: 'core:fluorescent_panel_01', x: 0.0, z: -4.0, rotation_degrees: 0.0, brightness: 1.0 },
        { fixture: 'core:fluorescent_panel_01', x: 0.0, z: 4.0, rotation_degrees: 0.0, brightness: 1.0 }
      ]
    });
    return level;
  }

  setLevel(newLevel, actionName = 'Set Level') {
    this.level = newLevel;
    this.editor.selectedIds.clear();
    this.history.clear();
    this.history.pushState(this.level, actionName);
    this.propertiesPanel.render();
    this.requestRender();
  }

  requestRender() {
    if (!this.renderRequested) {
      this.renderRequested = true;
      requestAnimationFrame(() => {
        this.renderRequested = false;
        this.renderer.render(this.level, this.editor);
      });
    }
  }

  onSelectionChanged() {
    this.propertiesPanel.render();
    const count = this.editor.selectedIds.size;
    this.statusSelection.textContent = count === 0 ? 'None' : `${count} selected`;
  }

  updateCursorCoords(wx, wz) {
    this.statusCoords.textContent = `X: ${wx.toFixed(2)}m, Z: ${wz.toFixed(2)}m`;
  }

  updateZoomLabel() {
    const pct = Math.round((this.renderer.zoom / 28) * 100);
    this.statusZoom.textContent = `${pct}%`;
    const zoomDisplay = document.getElementById('zoom-display');
    if (zoomDisplay) zoomDisplay.textContent = `${pct}%`;
  }

  updateStatus(msg) {
    this.statusMsg.textContent = msg;
    if (this.statusTimeout) clearTimeout(this.statusTimeout);
    this.statusTimeout = setTimeout(() => {
      this.statusMsg.textContent = 'Ready';
    }, 4500);
  }

  duplicateSelection() {
    const selectedIds = this.editor.selectedIds;
    if (selectedIds.size === 0) return;

    this.history.pushState(this.level, 'Duplicate Selection');
    const newSelectedIds = new Set();
    const offset = this.editor.snapEnabled ? this.editor.snapStep || 0.5 : 0.5;

    selectedIds.forEach(id => {
      if (id === 'spawn') {
        // Spawn is singleton in liminal-rust, just offset it
        this.level.spawn.x += offset;
        this.level.spawn.z += offset;
        newSelectedIds.add('spawn');
      } else {
        const wall = this.level.walls.find(w => w.id === id);
        if (wall) {
          const dup = wall.clone();
          dup.x += offset;
          dup.z += offset;
          this.level.walls.push(dup);
          newSelectedIds.add(dup.id);
        } else {
          const light = this.level.ceiling_lights.find(l => l.id === id);
          if (light) {
            const dup = light.clone();
            dup.x += offset;
            dup.z += offset;
            this.level.ceiling_lights.push(dup);
            newSelectedIds.add(dup.id);
          } else {
            const room = this.level.rooms.find(r => r.id === id);
            if (room) {
              const dup = room.clone();
              dup.x += offset;
              dup.z += offset;
              this.level.rooms.push(dup);
              newSelectedIds.add(dup.id);
            }
          }
        }
      }
    });

    this.editor.selectedIds = newSelectedIds;
    this.onSelectionChanged();
    this.requestRender();
    this.updateStatus(`Duplicated ${newSelectedIds.size} object(s)`);
  }

  deleteSelection() {
    const selectedIds = this.editor.selectedIds;
    if (selectedIds.size === 0) return;

    this.history.pushState(this.level, 'Delete Selection');
    const count = selectedIds.size;

    this.level.walls = this.level.walls.filter(w => !selectedIds.has(w.id));
    this.level.ceiling_lights = this.level.ceiling_lights.filter(l => !selectedIds.has(l.id));

    // Handle room deletion: keep at least 1 room for valid level
    const remainingRooms = this.level.rooms.filter(r => !selectedIds.has(r.id));
    if (remainingRooms.length > 0) {
      this.level.rooms = remainingRooms;
    } else if (this.level.rooms.some(r => selectedIds.has(r.id))) {
      // If deleting the last room, reset it to default rather than 0 rooms
      this.level.rooms = [new Room({ x: -10, z: -10, width: 20, depth: 20, height: 3.5 })];
    }

    // Notice: Player spawn is required by liminal-rust, do not delete it, reset to (0,0) if deleted
    if (selectedIds.has('spawn')) {
      this.level.spawn.x = 0;
      this.level.spawn.z = 0;
    }

    this.editor.selectedIds.clear();
    this.onSelectionChanged();
    this.requestRender();
    this.updateStatus(`Deleted ${count} object(s)`);
  }

  undo() {
    const prev = this.history.undo(this.level);
    if (prev) {
      this.level = prev;
      this.editor.selectedIds.clear();
      this.onSelectionChanged();
      this.requestRender();
      this.updateStatus(`Undo: ${this.history.lastAction}`);
    }
  }

  redo() {
    const next = this.history.redo(this.level);
    if (next) {
      this.level = next;
      this.editor.selectedIds.clear();
      this.onSelectionChanged();
      this.requestRender();
      this.updateStatus(`Redo: ${this.history.lastAction}`);
    }
  }

  async importCustomTextureFile(file) {
    const reader = new FileReader();
    reader.onload = async (e) => {
      const dataUrl = e.target.result;
      const dims = await this.io.getImageDimensions(dataUrl);

      // Validate dimensions
      if (dims.width > 1024 || dims.height > 1024) {
        alert(`Warning: Texture dimensions (${dims.width}x${dims.height}) exceed the game loader limit of 1024x1024.`);
      }

      // Convert image to PNG bytes for game loader compatibility
      const canvas = document.createElement('canvas');
      canvas.width = dims.width;
      canvas.height = dims.height;
      const ctx = canvas.getContext('2d');
      const img = new Image();
      img.onload = () => {
        ctx.drawImage(img, 0, 0);
        canvas.toBlob(async (blob) => {
          const arrayBuffer = await blob.arrayBuffer();
          const bytes = new Uint8Array(arrayBuffer);
          const cleanName = file.name.replace(/\.[^/.]+$/, "").replace(/[^a-zA-Z0-9_-]/g, '_').toLowerCase();
          const matId = `pack:${cleanName}`;

          this.history.pushState(this.level, `Import Texture ${matId}`);
          this.level.custom_textures[matId] = {
            filename: `textures/${cleanName}.png`,
            dataUrl: canvas.toDataURL('image/png'),
            width: dims.width,
            height: dims.height,
            bytes
          };

          this.propertiesPanel.render();
          this.requestRender();
          this.updateStatus(`Imported texture "${matId}" (${dims.width}x${dims.height}px)`);
        }, 'image/png');
      };
      img.src = dataUrl;
    };
    reader.readAsDataURL(file);
  }

  deleteCustomTexture(matId) {
    this.history.pushState(this.level, `Delete Texture ${matId}`);
    delete this.level.custom_textures[matId];
    this.propertiesPanel.render();
    this.requestRender();
    this.updateStatus(`Removed custom texture "${matId}"`);
  }

  showValidationModal() {
    const res = validateLevel(this.level);
    const modal = document.getElementById('validation-modal');
    const content = document.getElementById('validation-results');

    let html = '';
    if (res.valid && res.warnings.length === 0) {
      html += `
        <div class="valid-banner success">
          <div class="icon">✓</div>
          <div>
            <strong>Level is fully valid!</strong>
            <p>All schema, dimension, spawn, and material checks passed. Ready to play in liminal-rust.</p>
          </div>
        </div>
      `;
    } else if (res.valid) {
      html += `
        <div class="valid-banner warning">
          <div class="icon">⚠</div>
          <div>
            <strong>Level is exportable with warnings.</strong>
          </div>
        </div>
      `;
    } else {
      html += `
        <div class="valid-banner danger">
          <div class="icon">✕</div>
          <div>
            <strong>Validation Errors Found (${res.errors.length})</strong>
            <p>The liminal-rust game loader will reject this level until errors are resolved.</p>
          </div>
        </div>
      `;
    }

    if (res.errors.length > 0) {
      html += `<h4>Errors:</h4><ul class="val-list error-list">`;
      res.errors.forEach(err => html += `<li>✕ ${err}</li>`);
      html += `</ul>`;
    }

    if (res.warnings.length > 0) {
      html += `<h4>Warnings:</h4><ul class="val-list warn-list">`;
      res.warnings.forEach(warn => html += `<li>⚠ ${warn}</li>`);
      html += `</ul>`;
    }

    html += `
      <div class="val-summary">
        <div>Total Walls: <strong>${this.level.walls.length}</strong> / 5000 max</div>
        <div>Ceiling Lights: <strong>${this.level.ceiling_lights.length}</strong> / 5000 max</div>
        <div>Room Sections: <strong>${this.level.rooms.length}</strong> / 500 max</div>
        <div>Pack Textures: <strong>${Object.keys(this.level.custom_textures).length}</strong></div>
      </div>
    `;

    content.innerHTML = html;
    modal.classList.add('open');
  }

  bindToolbar() {
    // Tools buttons
    document.querySelectorAll('.tool-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.tool-btn').forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        this.editor.setTool(btn.dataset.tool);
      });
    });

    // File action buttons
    document.getElementById('btn-new').addEventListener('click', () => {
      if (confirm("Create a new level? Any unsaved changes will be lost.")) {
        this.setLevel(this.createDefaultLevel(), 'New Level');
        this.renderer.fitToGeometry(this.level);
        this.updateStatus("Created new level");
      }
    });

    const fileInput = document.getElementById('file-import-input');
    document.getElementById('btn-open').addEventListener('click', () => {
      fileInput.click();
    });
    fileInput.addEventListener('change', (e) => {
      if (e.target.files.length > 0) {
        this.io.importFile(e.target.files[0]);
        fileInput.value = '';
      }
    });

    document.getElementById('btn-export-json').addEventListener('click', () => {
      this.io.exportJSON(this.level);
    });

    document.getElementById('btn-export-zip').addEventListener('click', () => {
      this.io.exportZIP(this.level);
    });

    document.getElementById('btn-validate').addEventListener('click', () => {
      this.showValidationModal();
    });

    // Edit actions
    document.getElementById('btn-undo').addEventListener('click', () => this.undo());
    document.getElementById('btn-redo').addEventListener('click', () => this.redo());
    document.getElementById('btn-duplicate').addEventListener('click', () => this.duplicateSelection());
    document.getElementById('btn-delete').addEventListener('click', () => this.deleteSelection());

    // Snapping controls
    const chkSnap = document.getElementById('chk-snap');
    const selectSnap = document.getElementById('select-snap-step');

    chkSnap.addEventListener('change', () => {
      this.editor.snapEnabled = chkSnap.checked;
      this.statusSnap.textContent = chkSnap.checked ? `${this.editor.snapStep}m` : 'Off';
      this.updateStatus(`Snap to grid: ${chkSnap.checked ? 'ON' : 'OFF'}`);
    });

    selectSnap.addEventListener('change', () => {
      this.editor.snapStep = parseFloat(selectSnap.value);
      this.statusSnap.textContent = chkSnap.checked ? `${this.editor.snapStep}m` : 'Off';
      this.requestRender();
    });

    // Zoom controls
    document.getElementById('btn-zoom-in').addEventListener('click', () => {
      this.renderer.zoom = Math.min(this.renderer.maxZoom, this.renderer.zoom * 1.25);
      this.updateZoomLabel();
      this.requestRender();
    });

    document.getElementById('btn-zoom-out').addEventListener('click', () => {
      this.renderer.zoom = Math.max(this.renderer.minZoom, this.renderer.zoom * 0.8);
      this.updateZoomLabel();
      this.requestRender();
    });

    document.getElementById('btn-zoom-reset').addEventListener('click', () => {
      this.renderer.zoom = 28;
      this.updateZoomLabel();
      this.requestRender();
    });

    document.getElementById('btn-fit-view').addEventListener('click', () => {
      this.renderer.fitToGeometry(this.level);
      this.updateZoomLabel();
      this.requestRender();
    });

    // View options toggles
    document.getElementById('chk-show-heights').addEventListener('change', (e) => {
      this.renderer.showHeightBadges = e.target.checked;
      this.requestRender();
    });

    // Modal close
    document.querySelectorAll('.modal-close').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.modal').forEach(m => m.classList.remove('open'));
      });
    });

    // Shortcuts help button
    const btnShortcuts = document.getElementById('btn-shortcuts-help');
    if (btnShortcuts) {
      btnShortcuts.addEventListener('click', () => {
        document.getElementById('shortcuts-modal').classList.add('open');
      });
    }
  }

  bindKeyboardShortcuts() {
    window.addEventListener('keydown', (e) => {
      // Don't intercept if user is typing into input field
      const tag = (e.target || {}).tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') {
        return;
      }

      const ctrlOrCmd = e.ctrlKey || e.metaKey;

      if (ctrlOrCmd && e.key === 'z') {
        e.preventDefault();
        if (e.shiftKey) {
          this.redo();
        } else {
          this.undo();
        }
        return;
      }

      if (ctrlOrCmd && e.key === 'y') {
        e.preventDefault();
        this.redo();
        return;
      }

      if (ctrlOrCmd && e.key === 'd') {
        e.preventDefault();
        this.duplicateSelection();
        return;
      }

      if (ctrlOrCmd && e.key === 'a') {
        e.preventDefault();
        this.editor.selectAll();
        return;
      }

      if (ctrlOrCmd && e.key === 's') {
        e.preventDefault();
        this.io.exportJSON(this.level);
        return;
      }

      if (e.key === 'Delete' || e.key === 'Backspace') {
        e.preventDefault();
        this.deleteSelection();
        return;
      }

      if (e.key === 'Escape') {
        e.preventDefault();
        this.editor.clearSelection();
        const selToolBtn = document.querySelector('.tool-btn[data-tool="select"]');
        if (selToolBtn) selToolBtn.click();
        return;
      }

      // Single-key tool switches
      switch (e.key.toLowerCase()) {
        case 'v':
          document.querySelector('.tool-btn[data-tool="select"]')?.click();
          break;
        case 'f':
          document.querySelector('.tool-btn[data-tool="floor"]')?.click();
          break;
        case 'u':
          document.querySelector('.tool-btn[data-tool="ceiling"]')?.click();
          break;
        case 'w':
          document.querySelector('.tool-btn[data-tool="wall"]')?.click();
          break;
        case 'c':
        case 'b':
          document.querySelector('.tool-btn[data-tool="column"]')?.click();
          break;
        case 'l':
          document.querySelector('.tool-btn[data-tool="light"]')?.click();
          break;
        case 'p':
          document.querySelector('.tool-btn[data-tool="spawn"]')?.click();
          break;
        case 'g': {
          const chk = document.getElementById('chk-snap');
          if (chk) {
            chk.checked = !chk.checked;
            chk.dispatchEvent(new Event('change'));
          }
          break;
        }
      }
    });
  }
}

// Boot application when DOM is ready
window.addEventListener('DOMContentLoaded', () => {
  window.app = new App();
});
