// io.js - Level import/export, level packs and the "prepare for the game" helper.

class LevelIO {
  constructor(app) {
    this.app = app;
  }

  // ------------------------------------------------------------------ output

  /** Serialized level JSON with a trailing newline. */
  levelJSON(level) {
    return JSON.stringify(level.toJSON(), null, 2) + '\n';
  }

  downloadBlob(blob, filename) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    setTimeout(() => {
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    }, 100);
  }

  async saveLevelFile(level) {
    const json = this.levelJSON(level);
    const filename = `${(level.id || 'level').replace(/[^a-zA-Z0-9_-]/g, '_')}.json`;

    // When the browser supports the File System Access API, let the user drop the
    // level straight into the game's import folder.
    if (typeof window !== 'undefined' && typeof window.showSaveFilePicker === 'function') {
      try {
        const handle = await window.showSaveFilePicker({
          suggestedName: filename,
          types: [{ description: 'Liminal level', accept: { 'application/json': ['.json'] } }]
        });
        const writable = await handle.createWritable();
        await writable.write(json);
        await writable.close();
        this.app.updateStatus(`Saved ${filename}`);
        return { filename, saved: true };
      } catch (err) {
        if (err && err.name === 'AbortError') return { filename, saved: false, cancelled: true };
        // Fall through to a normal download.
      }
    }

    this.downloadBlob(new Blob([json], { type: 'application/json' }), filename);
    this.app.updateStatus(`Exported ${filename}`);
    return { filename, saved: true, downloaded: true };
  }

  exportJSON(level) {
    const validation = validateLevel(level);
    if (!validation.valid) {
      const message = 'This level has validation errors:\n\n' + validation.errors.join('\n') + '\n\nSave anyway?';
      if (!confirm(message)) return false;
    }
    this.saveLevelFile(level);
    return true;
  }

  async exportZIP(level) {
    if (typeof JSZip === 'undefined') {
      alert('JSZip is not loaded, so a level pack cannot be created.');
      return false;
    }
    const validation = validateLevel(level);
    if (!validation.valid) {
      const message = 'This level has validation errors:\n\n' + validation.errors.join('\n') + '\n\nExport anyway?';
      if (!confirm(message)) return false;
    }

    this.app.updateStatus('Building level pack…');
    const zip = new JSZip();
    zip.file('level.json', this.levelJSON(level));

    const customTextures = level.custom_textures || {};
    const entries = Object.entries(customTextures);
    if (entries.length > 0) {
      const materials = { materials: {} };
      const textures = zip.folder('textures');
      for (const [id, texture] of entries) {
        const filename = String(texture.filename || '').replace(/^textures\//, '') || `${id.replace(/^pack:/, '')}.png`;
        materials.materials[id] = { texture: `textures/${filename}` };
        if (texture.bytes) textures.file(filename, texture.bytes);
        else if (texture.dataUrl) textures.file(filename, this.dataUrlToUint8Array(texture.dataUrl));
      }
      zip.file('materials.json', JSON.stringify(materials, null, 2));
    }

    try {
      const blob = await zip.generateAsync({ type: 'blob' });
      const filename = `${level.id || 'level'}.zip`;
      this.downloadBlob(blob, filename);
      this.app.updateStatus(`Exported level pack ${filename}`);
      return true;
    } catch (err) {
      console.error('ZIP generation failed:', err);
      alert(`Could not build the level pack: ${err.message}`);
      this.app.updateStatus('Level pack export failed');
      return false;
    }
  }

  // ------------------------------------------------------------------- input

  async importFile(file) {
    const name = file.name.toLowerCase();
    this.app.updateStatus(`Opening ${file.name}…`);
    if (name.endsWith('.json')) {
      return this.importJSONString(await file.text(), file.name);
    }
    if (name.endsWith('.zip')) {
      return this.importZIPFile(file);
    }
    alert('Unsupported file. Open a .json level or a .zip level pack.');
    return false;
  }

  importJSONString(json, sourceName = 'level.json', quiet = false) {
    try {
      const data = JSON.parse(json);
      if (data.format_version && data.format_version !== 1) {
        this.app.updateStatus(`Heads up: format_version ${data.format_version} (the game expects 1)`);
      }
      const level = new Level(data);
      this.app.setLevel(level, `Open ${sourceName}`);
      const stats = LiminalGeometry.levelStats(level);
      this.app.updateStatus(`Opened ${sourceName} — ${stats.rooms} room(s), ${stats.walls} wall(s), ${stats.openings} opening(s), ${stats.props} prop(s)`);
      return true;
    } catch (err) {
      console.error('Failed to parse level JSON:', err);
      if (!quiet) alert(`That file is not a valid level:\n${err.message}`);
      this.app.updateStatus('Could not open the level');
      return false;
    }
  }

  async importZIPFile(file) {
    if (typeof JSZip === 'undefined') {
      alert('JSZip is not loaded, so level packs cannot be opened.');
      return false;
    }
    try {
      const zip = await JSZip.loadAsync(file);
      let levelEntry = null;
      let materialsEntry = null;
      const textureEntries = [];

      zip.forEach((relativePath, entry) => {
        if (entry.dir) return;
        const normalized = relativePath.toLowerCase().replace(/\\/g, '/');
        const filename = normalized.split('/').pop();
        if (filename === 'level.json') levelEntry = entry;
        else if (filename === 'materials.json') materialsEntry = entry;
        else if (normalized.includes('textures/') || normalized.endsWith('.png')) textureEntries.push({ relativePath, entry, filename });
      });

      if (!levelEntry) {
        alert('That pack does not contain a level.json file.');
        return false;
      }

      const level = new Level(JSON.parse(await levelEntry.async('text')));
      const materialMap = {};
      if (materialsEntry) {
        try {
          const parsed = JSON.parse(await materialsEntry.async('text'));
          for (const [id, value] of Object.entries(parsed.materials || parsed)) {
            if (typeof value === 'string') materialMap[id] = value;
            else if (value && value.texture) materialMap[id] = value.texture;
          }
        } catch (err) {
          console.warn('Ignoring unreadable materials.json:', err);
        }
      }

      for (const texture of textureEntries) {
        const bytes = await texture.entry.async('uint8array');
        const dataUrl = this.bytesToPngDataUrl(bytes);
        const dimensions = await this.getImageDimensions(dataUrl);
        let materialId = null;
        for (const [id, target] of Object.entries(materialMap)) {
          const normalized = target.toLowerCase().replace(/\\/g, '/');
          if (normalized === texture.relativePath.toLowerCase() || normalized.split('/').pop() === texture.filename) {
            materialId = id;
            break;
          }
        }
        if (!materialId) materialId = `pack:${texture.filename.replace(/\.[^/.]+$/, '')}`;
        level.custom_textures[materialId] = {
          filename: `textures/${texture.filename}`,
          dataUrl,
          width: dimensions.width,
          height: dimensions.height,
          bytes
        };
      }

      this.app.setLevel(level, `Open pack ${file.name}`);
      const stats = LiminalGeometry.levelStats(level);
      this.app.updateStatus(`Opened ${file.name} — ${stats.rooms} room(s), ${stats.walls} wall(s), ${stats.props} prop(s)`);
      return true;
    } catch (err) {
      console.error('Failed to read the level pack:', err);
      alert(`Could not read that level pack:\n${err.message}`);
      return false;
    }
  }

  // ------------------------------------------------------------------ helpers

  dataUrlToUint8Array(dataUrl) {
    const binary = atob(dataUrl.split(',')[1]);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes;
  }

  bytesToPngDataUrl(bytes) {
    let binary = '';
    for (let i = 0; i < bytes.byteLength; i++) binary += String.fromCharCode(bytes[i]);
    return 'data:image/png;base64,' + btoa(binary);
  }

  getImageDimensions(dataUrl) {
    return new Promise((resolve) => {
      const image = new Image();
      image.onload = () => resolve({ width: image.naturalWidth || 64, height: image.naturalHeight || 64 });
      image.onerror = () => resolve({ width: 64, height: 64 });
      image.src = dataUrl;
    });
  }

  setupDragAndDrop() {
    window.addEventListener('dragover', (e) => {
      e.preventDefault();
      e.dataTransfer.dropEffect = 'copy';
    });
    window.addEventListener('drop', (e) => {
      e.preventDefault();
      if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
        this.importFile(e.dataTransfer.files[0]);
      }
    });
  }
}
