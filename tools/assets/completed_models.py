"""Lazy accepted-export ledger snapshot for one offline run or worker.

Keep snapshots local to an invocation: later commands must see any ledger
edits, and inspecting an unrelated scratch model must not require this file.
Callers retain their own source-path, hash and reporting policies.
"""
import json
from pathlib import Path


class CompletedModels:
    def __init__(self, asset_root):
        self.path = Path(asset_root) / "completed-models.json"
        self._records = None

    def get(self, model, default=None):
        if self._records is None:
            self._records = json.loads(self.path.read_text())
        return self._records.get(model, default)
