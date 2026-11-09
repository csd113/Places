#!/usr/bin/env python3
"""Render the guide and verify its structural contract; inspect PNGs separately."""
from pathlib import Path
import hashlib
import json
import sys
import pymupdf

root = Path(__file__).resolve().parents[1]
pdf = root.parent / 'Beginners_Guide_to_Your_First_Places_Space.pdf'
out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('/tmp/places-guide-review')
out.mkdir(parents=True, exist_ok=True)
pages = json.loads((root / 'source/pages.json').read_text())
doc = pymupdf.open(pdf)
issues = []
if len(doc) != len(pages):
    issues.append('PDF/source page count mismatch')
if len(doc.get_toc()) != len(pages):
    issues.append('Missing page bookmarks')
for i, page in enumerate(doc):
    page.get_pixmap(matrix=pymupdf.Matrix(1.5, 1.5)).save(out / f'page-{i + 1:02}.png')
    if len(page.get_text()) < 200:
        issues.append(f'Page {i + 1}: insufficient extractable text')
    for block in page.get_text('dict')['blocks']:
        for line in block.get('lines', []):
            for span in line['spans']:
                x0, y0, x1, y1 = span['bbox']
                if x0 < 0 or y0 < 0 or x1 > page.rect.width + .1 or y1 > page.rect.height + .1:
                    issues.append(f'Page {i + 1}: text outside page: {span["text"]}')
text = '\n'.join(page.get_text() for page in doc)
for term in ['Doorway opening', 'core:desk', 'core:chair', 'core:bed', 'home:sink', 'pool:pool_bench', 'core:rubber_duck', 'GLOBAL surface region', 'Link selected', 'Final build']:
    if term not in text:
        issues.append(f'Missing required content: {term}')
for name, expected in json.loads((root / 'verification/screenshot-sha256.json').read_text()).items():
    if hashlib.sha256((root / 'screenshots' / name).read_bytes()).hexdigest() != expected:
        issues.append(f'Screenshot changed: {name}')
report = {'pages': len(doc), 'bookmarks': len(doc.get_toc()), 'page_size_points': list(doc[0].rect), 'pdf_bytes': pdf.stat().st_size, 'pdf_sha256': hashlib.sha256(pdf.read_bytes()).hexdigest(), 'issues': issues, 'renders': str(out), 'visual_review': 'Required separately; rendering is not visual approval.'}
(out / 'validation.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
raise SystemExit(bool(issues))
