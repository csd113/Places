// properties.js - Inspector panel UI management for Liminal Level Editor

class PropertiesPanel {
  constructor(container, app) {
    this.container = container;
    this.app = app;
    this.activeTab = 'selection'; // 'selection', 'level', 'textures'
    this.init();
  }

  init() {
    this.render();
  }

  setTab(tab) {
    this.activeTab = tab;
    this.render();
  }

  render() {
    const selectedIds = this.app.editor.selectedIds;
    const level = this.app.level;

    // Header tabs
    let html = `
      <div class="panel-tabs">
        <button class="tab-btn ${this.activeTab === 'selection' ? 'active' : ''}" data-tab="selection">
          Selection ${selectedIds.size > 0 ? `(${selectedIds.size})` : ''}
        </button>
        <button class="tab-btn ${this.activeTab === 'level' ? 'active' : ''}" data-tab="level">
          Level Settings
        </button>
        <button class="tab-btn ${this.activeTab === 'textures' ? 'active' : ''}" data-tab="textures">
          Textures
        </button>
      </div>
      <div class="panel-content">
    `;

    if (this.activeTab === 'selection') {
      if (selectedIds.size === 0) {
        html += this.renderEmptySelection();
      } else if (selectedIds.size === 1) {
        const id = Array.from(selectedIds)[0];
        html += this.renderSingleSelection(id);
      } else {
        html += this.renderMultiSelection(selectedIds);
      }
    } else if (this.activeTab === 'level') {
      html += this.renderLevelSettings(level);
    } else if (this.activeTab === 'textures') {
      html += this.renderTextureManager(level);
    }

    html += `</div>`;
    this.container.innerHTML = html;
    this.bindEvents();
  }

  renderEmptySelection() {
    return `
      <div class="empty-state">
        <div class="empty-icon">⬚</div>
        <p>No object selected</p>
        <p class="subtext">Click on geometry to inspect properties, or switch to tools to draw new walls, columns, lights, and spawn.</p>
        <div class="quick-nav-box">
          <button class="btn btn-secondary btn-block" id="btn-goto-level-settings">Edit Level Settings</button>
        </div>
      </div>
    `;
  }

  renderSingleSelection(id) {
    const level = this.app.level;
    const defaultCeiling = level.getCeilingHeight();

    if (id === 'spawn') {
      const s = level.spawn;
      return `
        <div class="prop-section">
          <div class="section-title">
            <span class="type-badge spawn-badge">● Player Spawn</span>
          </div>

          <div class="form-group-row">
            <div class="form-group">
              <label>Position X (m)</label>
              <input type="number" step="0.1" class="prop-input" data-obj="spawn" data-prop="x" value="${s.x.toFixed(2)}">
            </div>
            <div class="form-group">
              <label>Position Z (m)</label>
              <input type="number" step="0.1" class="prop-input" data-obj="spawn" data-prop="z" value="${s.z.toFixed(2)}">
            </div>
          </div>

          <div class="form-group">
            <label>Yaw Orientation (Degrees)</label>
            <div class="input-with-addons">
              <input type="number" step="5" min="0" max="360" class="prop-input" data-obj="spawn" data-prop="yaw_degrees" value="${s.yaw_degrees.toFixed(1)}">
              <span class="unit-addon">°</span>
            </div>
          </div>

          <div class="quick-presets-row">
            <button class="btn btn-xs btn-preset" data-preset="yaw:0">N (0°)</button>
            <button class="btn btn-xs btn-preset" data-preset="yaw:90">E (90°)</button>
            <button class="btn btn-xs btn-preset" data-preset="yaw:180">S (180°)</button>
            <button class="btn btn-xs btn-preset" data-preset="yaw:270">W (270°)</button>
          </div>
        </div>
      `;
    }

    const wall = level.walls.find(w => w.id === id);
    if (wall) {
      const isFull = wall.height === null || wall.height === undefined;
      const curH = wall.getResolvedHeight(defaultCeiling);

      return `
        <div class="prop-section">
          <div class="section-title">
            <span class="type-badge wall-badge">■ Wall</span>
            <span class="prop-id">#${wall.id.slice(0, 10)}</span>
          </div>

          <div class="form-group-row">
            <div class="form-group">
              <label>X Position (m)</label>
              <input type="number" step="0.05" class="prop-input" data-obj="wall" data-id="${wall.id}" data-prop="x" value="${wall.x.toFixed(2)}">
            </div>
            <div class="form-group">
              <label>Z Position (m)</label>
              <input type="number" step="0.05" class="prop-input" data-obj="wall" data-id="${wall.id}" data-prop="z" value="${wall.z.toFixed(2)}">
            </div>
          </div>

          <div class="form-group-row">
            <div class="form-group">
              <label>Width (m)</label>
              <input type="number" step="0.05" min="0.05" class="prop-input" data-obj="wall" data-id="${wall.id}" data-prop="width" value="${wall.width.toFixed(2)}">
            </div>
            <div class="form-group">
              <label>Depth (m)</label>
              <input type="number" step="0.05" min="0.05" class="prop-input" data-obj="wall" data-id="${wall.id}" data-prop="depth" value="${wall.depth.toFixed(2)}">
            </div>
          </div>

          <div class="section-divider"></div>
          <div class="section-subtitle">Elevation & Height</div>

          <div class="form-group">
            <label>Base Elevation Y (Floor = 0.0m)</label>
            <input type="number" step="0.1" min="0" class="prop-input" data-obj="wall" data-id="${wall.id}" data-prop="y" value="${wall.y.toFixed(2)}">
            <div class="helper-text">${wall.y > 0 ? 'Raised wall segment (e.g. door header / high window)' : 'Floor-level wall segment'}</div>
          </div>

          <div class="form-group">
            <label class="checkbox-label">
              <input type="checkbox" id="chk-wall-full-height" data-id="${wall.id}" ${isFull ? 'checked' : ''}>
              <span>Full Ceiling Height (${defaultCeiling.toFixed(1)}m)</span>
            </label>
          </div>

          ${!isFull ? `
            <div class="form-group">
              <label>Explicit Wall Height (m)</label>
              <input type="number" step="0.1" min="0.1" class="prop-input" data-obj="wall" data-id="${wall.id}" data-prop="height" value="${curH.toFixed(2)}">
            </div>
          ` : ''}

          <div class="quick-presets-row">
            <span class="preset-label">Presets:</span>
            <button class="btn btn-xs btn-preset" data-preset="wall-full:${wall.id}">Full Height</button>
            <button class="btn btn-xs btn-preset" data-preset="wall-header:${wall.id}">Door Header (Y:2.2, H:1.3)</button>
            <button class="btn btn-xs btn-preset" data-preset="wall-sill:${wall.id}">Window Sill (Y:0, H:1.0)</button>
          </div>

          <div class="section-divider"></div>
          <div class="section-subtitle">Materials</div>

          <div class="form-group">
            <label>Wall Material</label>
            ${this.renderMaterialDropdown('wall', wall.material || level.defaults.wall, `wall-mat:${wall.id}`)}
          </div>

          <details class="advanced-faces-details">
            <summary>Per-Face Materials (Optional)</summary>
            <div class="faces-grid">
              <div class="form-group">
                <label>North Face</label>
                ${this.renderMaterialDropdown('wall', wall.faces.north || '', `face-north:${wall.id}`, true)}
              </div>
              <div class="form-group">
                <label>South Face</label>
                ${this.renderMaterialDropdown('wall', wall.faces.south || '', `face-south:${wall.id}`, true)}
              </div>
              <div class="form-group">
                <label>East Face</label>
                ${this.renderMaterialDropdown('wall', wall.faces.east || '', `face-east:${wall.id}`, true)}
              </div>
              <div class="form-group">
                <label>West Face</label>
                ${this.renderMaterialDropdown('wall', wall.faces.west || '', `face-west:${wall.id}`, true)}
              </div>
            </div>
          </details>

          <div class="action-btn-row">
            <button class="btn btn-secondary btn-sm" id="btn-prop-duplicate" title="Duplicate (Ctrl+D)">Duplicate</button>
            <button class="btn btn-danger btn-sm" id="btn-prop-delete" title="Delete (Del)">Delete</button>
          </div>
        </div>
      `;
    }

    const light = level.ceiling_lights.find(l => l.id === id);
    if (light) {
      return `
        <div class="prop-section">
          <div class="section-title">
            <span class="type-badge light-badge">☼ Ceiling Light</span>
            <span class="prop-id">#${light.id.slice(0, 10)}</span>
          </div>

          <div class="form-group-row">
            <div class="form-group">
              <label>X Position (m)</label>
              <input type="number" step="0.1" class="prop-input" data-obj="light" data-id="${light.id}" data-prop="x" value="${light.x.toFixed(2)}">
            </div>
            <div class="form-group">
              <label>Z Position (m)</label>
              <input type="number" step="0.1" class="prop-input" data-obj="light" data-id="${light.id}" data-prop="z" value="${light.z.toFixed(2)}">
            </div>
          </div>

          <div class="form-group">
            <label>Rotation (Degrees)</label>
            <div class="input-with-addons">
              <input type="number" step="90" min="0" max="360" class="prop-input" data-obj="light" data-id="${light.id}" data-prop="rotation_degrees" value="${light.rotation_degrees.toFixed(0)}">
              <span class="unit-addon">°</span>
            </div>
          </div>

          <div class="quick-presets-row">
            <button class="btn btn-xs btn-preset" data-preset="light-rot:${light.id}:0">0° (Horiz)</button>
            <button class="btn btn-xs btn-preset" data-preset="light-rot:${light.id}:90">90° (Vert)</button>
            <button class="btn btn-xs btn-preset" data-preset="light-rot:${light.id}:180">180°</button>
            <button class="btn btn-xs btn-preset" data-preset="light-rot:${light.id}:270">270°</button>
          </div>

          <div class="form-group">
            <label>Brightness</label>
            <input type="number" step="0.1" min="0.1" max="5.0" class="prop-input" data-obj="light" data-id="${light.id}" data-prop="brightness" value="${(light.brightness || 1.0).toFixed(1)}">
          </div>

          <div class="form-group">
            <label>Fixture Material</label>
            ${this.renderMaterialDropdown('fixture', light.fixture, `light-fixture:${light.id}`)}
          </div>

          <div class="action-btn-row">
            <button class="btn btn-secondary btn-sm" id="btn-prop-duplicate" title="Duplicate (Ctrl+D)">Duplicate</button>
            <button class="btn btn-danger btn-sm" id="btn-prop-delete" title="Delete (Del)">Delete</button>
          </div>
        </div>
      `;
    }

    return this.renderEmptySelection();
  }

  renderMultiSelection(selectedIds) {
    const level = this.app.level;
    const walls = level.walls.filter(w => selectedIds.has(w.id));
    const lights = level.ceiling_lights.filter(l => selectedIds.has(l.id));
    const hasSpawn = selectedIds.has('spawn');

    return `
      <div class="prop-section">
        <div class="section-title">
          <span class="type-badge multi-badge">❖ Multi-Selection</span>
        </div>
        <p class="multi-summary">
          <strong>${selectedIds.size} objects selected:</strong><br>
          ${walls.length > 0 ? `• ${walls.length} Wall(s)<br>` : ''}
          ${lights.length > 0 ? `• ${lights.length} Ceiling Light(s)<br>` : ''}
          ${hasSpawn ? `• Player Spawn<br>` : ''}
        </p>

        ${walls.length > 0 ? `
          <div class="section-divider"></div>
          <div class="section-subtitle">Batch Wall Edits (${walls.length} walls)</div>

          <div class="form-group">
            <label>Set Elevation Y for all selected walls (m)</label>
            <div class="input-with-action">
              <input type="number" step="0.1" id="batch-wall-y" value="0.0">
              <button class="btn btn-xs btn-primary" id="btn-apply-batch-y">Apply</button>
            </div>
          </div>

          <div class="form-group">
            <label>Set Height for all selected walls (m)</label>
            <div class="input-with-action">
              <input type="number" step="0.1" id="batch-wall-h" value="3.5">
              <button class="btn btn-xs btn-primary" id="btn-apply-batch-h">Apply</button>
            </div>
          </div>

          <div class="form-group">
            <label>Set Material for all selected walls</label>
            ${this.renderMaterialDropdown('wall', level.defaults.wall, 'batch-wall-mat')}
            <button class="btn btn-xs btn-primary btn-block mt-1" id="btn-apply-batch-mat">Apply Material to Selected</button>
          </div>
        ` : ''}

        <div class="action-btn-row">
          <button class="btn btn-secondary btn-sm" id="btn-prop-duplicate">Duplicate All (${selectedIds.size})</button>
          <button class="btn btn-danger btn-sm" id="btn-prop-delete">Delete All (${selectedIds.size})</button>
        </div>
      </div>
    `;
  }

  renderLevelSettings(level) {
    const room = level.rooms[0] || { x: -10, z: -10, width: 20, depth: 20, height: 3.5 };

    return `
      <div class="prop-section">
        <div class="section-title">
          <span class="type-badge level-badge">⚙ Level Settings</span>
        </div>

        <div class="form-group">
          <label>Level Name</label>
          <input type="text" class="prop-input" data-level="name" value="${level.name}">
        </div>

        <div class="form-group">
          <label>Level ID</label>
          <input type="text" class="prop-input" data-level="id" value="${level.id}" placeholder="e.g. level_1">
          <div class="helper-text">Unique alphanumeric identifier used by the game loader.</div>
        </div>

        <div class="form-group">
          <label>Author</label>
          <input type="text" class="prop-input" data-level="author" value="${level.author}">
        </div>

        <div class="form-group">
          <label>Format Version</label>
          <input type="number" class="prop-input" value="${level.format_version}" disabled>
          <div class="helper-text">Format version 1 required by liminal-rust.</div>
        </div>

        <div class="section-divider"></div>
        <div class="section-subtitle">Room Bounds & Ceiling</div>

        <div class="form-group">
          <label>Ceiling Height (m)</label>
          <input type="number" step="0.1" min="1.0" max="20.0" class="prop-input" data-room="height" value="${room.height.toFixed(1)}">
          <div class="helper-text">Default ceiling height for room and full-height walls (3.5m standard).</div>
        </div>

        <div class="form-group-row">
          <div class="form-group">
            <label>Room X (m)</label>
            <input type="number" step="1" class="prop-input" data-room="x" value="${room.x.toFixed(1)}">
          </div>
          <div class="form-group">
            <label>Room Z (m)</label>
            <input type="number" step="1" class="prop-input" data-room="z" value="${room.z.toFixed(1)}">
          </div>
        </div>

        <div class="form-group-row">
          <div class="form-group">
            <label>Room Width (m)</label>
            <input type="number" step="1" min="1" class="prop-input" data-room="width" value="${room.width.toFixed(1)}">
          </div>
          <div class="form-group">
            <label>Room Depth (m)</label>
            <input type="number" step="1" min="1" class="prop-input" data-room="depth" value="${room.depth.toFixed(1)}">
          </div>
        </div>

        <div class="section-divider"></div>
        <div class="section-subtitle">Default Materials</div>

        <div class="form-group">
          <label>Default Wall Material</label>
          ${this.renderMaterialDropdown('wall', level.defaults.wall, 'level-default:wall')}
        </div>

        <div class="form-group">
          <label>Default Floor Material</label>
          ${this.renderMaterialDropdown('floor', level.defaults.floor, 'level-default:floor')}
        </div>

        <div class="form-group">
          <label>Default Ceiling Material</label>
          ${this.renderMaterialDropdown('ceiling', level.defaults.ceiling, 'level-default:ceiling')}
        </div>
      </div>
    `;
  }

  renderTextureManager(level) {
    const customList = Object.entries(level.custom_textures || {});

    let html = `
      <div class="prop-section">
        <div class="section-title">
          <span class="type-badge tex-badge">🎨 Texture Pack Manager</span>
        </div>
        <p class="subtext">
          Import PNG textures to package with this level. Uses the <code>pack:*</code> namespace.
        </p>

        <div class="upload-box">
          <input type="file" id="tex-file-input" accept="image/png,image/jpeg,image/webp" style="display:none">
          <button class="btn btn-primary btn-block" id="btn-import-texture">
            + Import Custom Texture PNG
          </button>
        </div>

        <div class="section-divider"></div>
        <div class="section-subtitle">Pack Textures (${customList.length})</div>
    `;

    if (customList.length === 0) {
      html += `
        <div class="empty-state-sm">
          <p>No custom textures imported.</p>
          <p class="subtext">Level currently uses built-in core textures.</p>
        </div>
      `;
    } else {
      html += `<div class="tex-list">`;
      customList.forEach(([id, tex]) => {
        const isOversized = tex.width > 1024 || tex.height > 1024;
        html += `
          <div class="tex-card ${isOversized ? 'tex-oversized' : ''}">
            <img src="${tex.dataUrl}" class="tex-thumb" alt="${id}">
            <div class="tex-meta">
              <div class="tex-id">${id}</div>
              <div class="tex-dims">${tex.width}×${tex.height}px • ${tex.filename}</div>
              ${isOversized ? '<div class="tex-warning">⚠ Exceeds 1024x1024 limit</div>' : ''}
            </div>
            <button class="btn btn-xs btn-danger btn-delete-tex" data-tex-id="${id}" title="Delete Texture">✕</button>
          </div>
        `;
      });
      html += `</div>`;
    }

    html += `
        <div class="section-divider"></div>
        <div class="section-subtitle">Core Built-in Materials</div>
        <div class="tex-list">
    `;

    for (const [id, mat] of Object.entries(CORE_MATERIALS)) {
      const thumb = CORE_THUMBNAILS[id] || '';
      html += `
        <div class="tex-card core-card">
          <img src="${thumb}" class="tex-thumb" alt="${mat.name}">
          <div class="tex-meta">
            <div class="tex-name">${mat.name}</div>
            <div class="tex-id"><code>${id}</code></div>
          </div>
        </div>
      `;
    }

    html += `
        </div>
      </div>
    `;

    return html;
  }

  renderMaterialDropdown(category, currentVal, elementId, allowDefault = false) {
    const level = this.app.level;
    const thumb = this.getThumbnailForMaterial(currentVal);

    let html = `
      <div class="mat-picker-row">
        <img src="${thumb}" class="mat-preview-thumb" id="thumb-${elementId}" alt="Preview">
        <select class="mat-select" data-mat-target="${elementId}">
    `;

    if (allowDefault) {
      html += `<option value="" ${!currentVal ? 'selected' : ''}>[Inherit Default]</option>`;
    }

    html += `<optgroup label="Core Built-in Materials">`;
    for (const [id, mat] of Object.entries(CORE_MATERIALS)) {
      if (!category || mat.category === category || (category === 'wall' && mat.category === 'wall')) {
        const sel = currentVal === id ? 'selected' : '';
        html += `<option value="${id}" ${sel}>${mat.name} (${id})</option>`;
      }
    }
    html += `</optgroup>`;

    if (level.custom_textures && Object.keys(level.custom_textures).length > 0) {
      html += `<optgroup label="Custom Pack Materials">`;
      for (const [id, tex] of Object.entries(level.custom_textures)) {
        const sel = currentVal === id ? 'selected' : '';
        html += `<option value="${id}" ${sel}>${id} (${tex.filename})</option>`;
      }
      html += `</optgroup>`;
    }

    html += `
        </select>
      </div>
    `;
    return html;
  }

  getThumbnailForMaterial(matId) {
    if (!matId) return CORE_THUMBNAILS['core:wallpaper_yellow_01'];
    if (CORE_THUMBNAILS[matId]) return CORE_THUMBNAILS[matId];
    if (this.app.level.custom_textures && this.app.level.custom_textures[matId]) {
      return this.app.level.custom_textures[matId].dataUrl;
    }
    return CORE_THUMBNAILS['core:wallpaper_yellow_01'];
  }

  bindEvents() {
    // Tabs click
    this.container.querySelectorAll('.tab-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        this.setTab(btn.dataset.tab);
      });
    });

    // Quick nav button
    const gotoLevelBtn = this.container.querySelector('#btn-goto-level-settings');
    if (gotoLevelBtn) {
      gotoLevelBtn.addEventListener('click', () => this.setTab('level'));
    }

    // Material dropdown change
    this.container.querySelectorAll('.mat-select').forEach(sel => {
      sel.addEventListener('change', (e) => {
        const target = sel.dataset.matTarget;
        const val = sel.value;
        this.handleMaterialChange(target, val);
      });
    });

    // Generic property input change
    this.container.querySelectorAll('.prop-input').forEach(input => {
      input.addEventListener('input', (e) => {
        this.handlePropChange(input);
      });
      input.addEventListener('change', (e) => {
        this.handlePropCommit(input);
      });
    });

    // Wall Full Height toggle
    const chkFull = this.container.querySelector('#chk-wall-full-height');
    if (chkFull) {
      chkFull.addEventListener('change', (e) => {
        const wallId = chkFull.dataset.id;
        const wall = this.app.level.walls.find(w => w.id === wallId);
        if (wall) {
          this.app.history.pushState(this.app.level, 'Toggle Full Height');
          wall.height = chkFull.checked ? null : 3.5;
          this.render();
          this.app.requestRender();
        }
      });
    }

    // Presets buttons
    this.container.querySelectorAll('.btn-preset').forEach(btn => {
      btn.addEventListener('click', () => {
        this.handlePresetClick(btn.dataset.preset);
      });
    });

    // Actions: Duplicate, Delete
    const btnDup = this.container.querySelector('#btn-prop-duplicate');
    if (btnDup) {
      btnDup.addEventListener('click', () => this.app.duplicateSelection());
    }

    const btnDel = this.container.querySelector('#btn-prop-delete');
    if (btnDel) {
      btnDel.addEventListener('click', () => this.app.deleteSelection());
    }

    // Batch wall buttons
    const btnBatchY = this.container.querySelector('#btn-apply-batch-y');
    if (btnBatchY) {
      btnBatchY.addEventListener('click', () => {
        const val = parseFloat(this.container.querySelector('#batch-wall-y').value) || 0;
        this.app.history.pushState(this.app.level, 'Batch Set Wall Y');
        this.app.editor.selectedIds.forEach(id => {
          const w = this.app.level.walls.find(x => x.id === id);
          if (w) w.y = val;
        });
        this.render();
        this.app.requestRender();
      });
    }

    const btnBatchH = this.container.querySelector('#btn-apply-batch-h');
    if (btnBatchH) {
      btnBatchH.addEventListener('click', () => {
        const val = parseFloat(this.container.querySelector('#batch-wall-h').value) || 3.5;
        this.app.history.pushState(this.app.level, 'Batch Set Wall Height');
        this.app.editor.selectedIds.forEach(id => {
          const w = this.app.level.walls.find(x => x.id === id);
          if (w) w.height = val;
        });
        this.render();
        this.app.requestRender();
      });
    }

    const btnBatchMat = this.container.querySelector('#btn-apply-batch-mat');
    if (btnBatchMat) {
      btnBatchMat.addEventListener('click', () => {
        const sel = this.container.querySelector('[data-mat-target="batch-wall-mat"]');
        if (sel) {
          const mat = sel.value;
          this.app.history.pushState(this.app.level, 'Batch Set Wall Material');
          this.app.editor.selectedIds.forEach(id => {
            const w = this.app.level.walls.find(x => x.id === id);
            if (w) w.material = mat;
          });
          this.render();
          this.app.requestRender();
        }
      });
    }

    // Texture upload button
    const btnImportTex = this.container.querySelector('#btn-import-texture');
    const fileInput = this.container.querySelector('#tex-file-input');
    if (btnImportTex && fileInput) {
      btnImportTex.addEventListener('click', () => fileInput.click());
      fileInput.addEventListener('change', (e) => {
        if (e.target.files.length > 0) {
          this.app.importCustomTextureFile(e.target.files[0]);
          fileInput.value = '';
        }
      });
    }

    // Delete texture button
    this.container.querySelectorAll('.btn-delete-tex').forEach(btn => {
      btn.addEventListener('click', () => {
        const id = btn.dataset.texId;
        if (confirm(`Remove texture "${id}"?`)) {
          this.app.deleteCustomTexture(id);
        }
      });
    });
  }

  handlePropChange(input) {
    const val = input.value;
    const num = parseFloat(val);

    if (input.dataset.obj === 'wall') {
      const wall = this.app.level.walls.find(w => w.id === input.dataset.id);
      if (wall) {
        const prop = input.dataset.prop;
        if (prop === 'height') {
          wall.height = isNaN(num) ? 3.5 : Math.max(0.05, num);
        } else if (prop === 'width' || prop === 'depth') {
          wall[prop] = isNaN(num) ? 0.1 : Math.max(0.05, num);
        } else if (prop === 'x' || prop === 'z' || prop === 'y') {
          wall[prop] = isNaN(num) ? 0 : num;
        }
        this.app.requestRender();
      }
    } else if (input.dataset.obj === 'spawn') {
      const prop = input.dataset.prop;
      if (prop === 'x' || prop === 'z') {
        this.app.level.spawn[prop] = isNaN(num) ? 0 : num;
      } else if (prop === 'yaw_degrees') {
        this.app.level.spawn.yaw_degrees = isNaN(num) ? 0 : (num % 360 + 360) % 360;
      }
      this.app.requestRender();
    } else if (input.dataset.obj === 'light') {
      const light = this.app.level.ceiling_lights.find(l => l.id === input.dataset.id);
      if (light) {
        const prop = input.dataset.prop;
        if (prop === 'x' || prop === 'z') {
          light[prop] = isNaN(num) ? 0 : num;
        } else if (prop === 'rotation_degrees') {
          light.rotation_degrees = isNaN(num) ? 0 : num;
        } else if (prop === 'brightness') {
          light.brightness = isNaN(num) ? 1.0 : Math.max(0.1, num);
        }
        this.app.requestRender();
      }
    } else if (input.dataset.level) {
      const prop = input.dataset.level;
      this.app.level[prop] = val;
    } else if (input.dataset.room) {
      const prop = input.dataset.room;
      const room = this.app.level.rooms[0];
      if (room) {
        room[prop] = isNaN(num) ? 1.0 : num;
        this.app.requestRender();
      }
    }
  }

  handlePropCommit(input) {
    this.app.history.pushState(this.app.level, 'Edit Property');
    this.app.updateStatus(`Updated ${input.dataset.prop || input.dataset.level || 'property'}`);
  }

  handleMaterialChange(target, val) {
    this.app.history.pushState(this.app.level, 'Change Material');

    if (target.startsWith('wall-mat:')) {
      const wallId = target.split(':')[1];
      const wall = this.app.level.walls.find(w => w.id === wallId);
      if (wall) wall.material = val;
    } else if (target.startsWith('face-')) {
      const [faceType, wallId] = target.split(':');
      const face = faceType.replace('face-', '');
      const wall = this.app.level.walls.find(w => w.id === wallId);
      if (wall) {
        if (!val) {
          delete wall.faces[face];
        } else {
          wall.faces[face] = val;
        }
      }
    } else if (target.startsWith('light-fixture:')) {
      const lightId = target.split(':')[1];
      const light = this.app.level.ceiling_lights.find(l => l.id === lightId);
      if (light) light.fixture = val;
    } else if (target.startsWith('level-default:')) {
      const defProp = target.split(':')[1];
      this.app.level.defaults[defProp] = val;
    }

    this.render();
    this.app.requestRender();
  }

  handlePresetClick(preset) {
    this.app.history.pushState(this.app.level, 'Apply Preset');

    if (preset.startsWith('yaw:')) {
      const deg = parseFloat(preset.split(':')[1]);
      this.app.level.spawn.yaw_degrees = deg;
    } else if (preset.startsWith('wall-full:')) {
      const id = preset.split(':')[1];
      const wall = this.app.level.walls.find(w => w.id === id);
      if (wall) {
        wall.y = 0.0;
        wall.height = null;
      }
    } else if (preset.startsWith('wall-header:')) {
      const id = preset.split(':')[1];
      const wall = this.app.level.walls.find(w => w.id === id);
      if (wall) {
        wall.y = 2.2;
        wall.height = 1.3;
      }
    } else if (preset.startsWith('wall-sill:')) {
      const id = preset.split(':')[1];
      const wall = this.app.level.walls.find(w => w.id === id);
      if (wall) {
        wall.y = 0.0;
        wall.height = 1.0;
      }
    } else if (preset.startsWith('light-rot:')) {
      const [, id, deg] = preset.split(':');
      const light = this.app.level.ceiling_lights.find(l => l.id === id);
      if (light) light.rotation_degrees = parseFloat(deg);
    }

    this.render();
    this.app.requestRender();
  }
}
