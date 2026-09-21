// history.js - Undo / Redo history manager for Liminal Level Editor

class HistoryManager {
  constructor(maxStates = 50) {
    this.maxStates = maxStates;
    this.undoStack = [];
    this.redoStack = [];
    this.lastAction = 'Initial State';
    this.listeners = [];
  }

  onChange(callback) {
    this.listeners.push(callback);
  }

  notify() {
    this.listeners.forEach(cb => cb(this.canUndo(), this.canRedo(), this.lastAction));
  }

  pushState(level, actionName = 'Edit') {
    // Clone level deeply
    const snapshot = level.clone();
    this.undoStack.push({
      level: snapshot,
      action: actionName
    });

    if (this.undoStack.length > this.maxStates) {
      this.undoStack.shift();
    }

    // Any new action clears redo stack
    this.redoStack = [];
    this.lastAction = actionName;
    this.notify();
  }

  undo(currentLevel) {
    if (!this.canUndo()) return null;

    // Save current state to redo stack
    const currentSnapshot = currentLevel.clone();
    this.redoStack.push({
      level: currentSnapshot,
      action: this.lastAction
    });

    const entry = this.undoStack.pop();
    this.lastAction = entry.action;
    this.notify();
    return entry.level.clone();
  }

  redo(currentLevel) {
    if (!this.canRedo()) return null;

    // Save current state to undo stack
    const currentSnapshot = currentLevel.clone();
    this.undoStack.push({
      level: currentSnapshot,
      action: this.lastAction
    });

    const entry = this.redoStack.pop();
    this.lastAction = entry.action;
    this.notify();
    return entry.level.clone();
  }

  canUndo() {
    return this.undoStack.length > 0;
  }

  canRedo() {
    return this.redoStack.length > 0;
  }

  clear() {
    this.undoStack = [];
    this.redoStack = [];
    this.lastAction = 'Clean';
    this.notify();
  }
}
