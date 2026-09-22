// history.js - Undo / Redo history manager for Liminal Level Editor
//
// Convention: `pushState(level, action)` records the state *after* a change, so one
// user-visible action - including an entire drag - produces exactly one entry and a
// single Ctrl+Z reverses it.
//
// `entries` holds every recorded state oldest-first (entries[0] is usually the state
// the level was loaded with); `redoEntries` holds states that were undone.

class HistoryManager {
  constructor(maxStates = 50) {
    this.maxStates = maxStates;
    this.entries = [];
    this.redoEntries = [];
    this.lastAction = 'Initial State';
    this.listeners = [];
  }

  onChange(callback) {
    this.listeners.push(callback);
  }

  notify() {
    this.listeners.forEach(cb => cb(this.canUndo(), this.canRedo(), this.lastAction));
  }

  /** Records the current level state. Call this once per user-visible action. */
  pushState(level, actionName = 'Edit') {
    this.entries.push({ level: level.clone(), action: actionName });
    if (this.entries.length > this.maxStates) {
      this.entries.shift();
    }
    // Any new action invalidates the redo history.
    this.redoEntries = [];
    this.lastAction = actionName;
    this.notify();
  }

  /** Returns the previous recorded state, or null when there is nothing to undo. */
  undo(currentLevel) {
    if (!this.canUndo()) return null;
    const leaving = this.entries.pop();
    this.redoEntries.push({ level: currentLevel.clone(), action: leaving.action });
    this.lastAction = leaving.action;
    const target = this.entries[this.entries.length - 1];
    this.notify();
    return target.level.clone();
  }

  /** Returns the next recorded state, or null when there is nothing to redo. */
  redo() {
    if (!this.canRedo()) return null;
    const entry = this.redoEntries.pop();
    // The redone state becomes the current state, so it belongs on the main stack.
    this.entries.push(entry);
    this.lastAction = entry.action;
    this.notify();
    return entry.level.clone();
  }

  canUndo() {
    return this.entries.length > 1;
  }

  canRedo() {
    return this.redoEntries.length > 0;
  }

  /** Number of recorded states (1 means "only the loaded level"). */
  depth() {
    return this.entries.length;
  }

  clear() {
    this.entries = [];
    this.redoEntries = [];
    this.lastAction = 'Clean';
    this.notify();
  }
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = { HistoryManager };
}
