#!/usr/bin/env python3
"""Protect Luau's foreign-unwind boundary in the actual production executable.

Cargo's test harness uses unwind even when release binaries use abort. This
unique regression runs tiny deliberately invalid modules through the direct
release compiler, before any geometry/asset bake. The optional finished-project
checks validate their actual source ownership and initialization without baking.
All adversarial inputs stay in one temporary local project.
"""
import argparse
import hashlib
import json
import os
import subprocess
import tempfile
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=Path, default=Path('target/release/places-compile'))
    parser.add_argument('--asset-root', type=Path, default=Path('assets'))
    parser.add_argument('--project', type=Path, action='append', default=[])
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    compiler, assets = args.compiler.resolve(), args.asset_root.resolve()
    environment = os.environ.copy()
    environment.pop('PLACES_ASSET_ROOT', None)
    cases = [
        ('runaway', 'while true do end', 'execution budget exhausted'),
        ('string_memory', "local huge = string.rep('x', 4000000); error('String memory limit unexpectedly absent')", 'not enough memory'),
        ('table_memory', "local huge = table.create(1000000); error('Table memory limit unexpectedly absent')", 'not enough memory'),
        ('ordinary_error', "error('release refusal fixture')", 'release refusal fixture'),
        ('error_object', 'error(setmetatable({}, {__tostring=function() while true do end end}))', None),
        ('readonly_library', "table.insert = function() end; error('Mutable library unexpectedly exposed')", 'readonly'),
        ('native_format_width', "string.format('%2147483647s', 'x'); error('Wide format unexpectedly allowed')", 'width or precision too long'),
        ('forbidden_os', "local capability = os.execute; error('OS capability unexpectedly exposed')", 'nil'),
        ('dynamic_loading', "loadstring('return {}'); error('Dynamic loading capability unexpectedly exposed')", 'nil'),
        ('native_pattern', "string.find('fixture', '.*'); error('Native pattern capability unexpectedly exposed')", 'nil'),
        ('sparse_table_move', "table.move({},1,10000000,-10000000); error('Sparse move unexpectedly allowed')", 'range exceeds 8192'),
    ]
    report = {
        'compiler': str(compiler),
        'compiler_sha256': hashlib.sha256(compiler.read_bytes()).hexdigest(),
        'asset_root': str(assets),
        'scope': 'Source initialization and production exception boundary; handler query cost is covered by the owning Rust regression.',
        'projects': [], 'refusals': [],
    }
    for requested in args.project:
        selected = requested.resolve()
        before = {str(path.relative_to(selected)): hashlib.sha256(path.read_bytes()).hexdigest()
                  for path in selected.rglob('*') if path.is_file()}
        started = time.monotonic()
        result = subprocess.run([
            str(compiler), 'check-project', str(selected), '--asset-root', str(assets),
        ], cwd=tempfile.gettempdir(), env=environment, capture_output=True, text=True, timeout=30, check=False)
        after = {str(path.relative_to(selected)): hashlib.sha256(path.read_bytes()).hexdigest()
                 for path in selected.rglob('*') if path.is_file()}
        passed = result.returncode == 0 and before == after
        report['projects'].append({'project': str(selected), 'passed': passed,
                                   'exit': result.returncode, 'seconds': time.monotonic() - started,
                                   'source_hashes': before, 'unchanged': before == after,
                                   'output': (result.stdout + result.stderr)[:4096]})
        if not passed:
            if args.report:
                args.report.write_text(json.dumps(report, indent=2) + '\n')
            raise SystemExit(f'Finished-project Luau check failed: {selected}\n{result.stdout}{result.stderr}')
    with tempfile.TemporaryDirectory(prefix='places-luau-release-') as directory:
        project = Path(directory)
        (project / 'levels').mkdir()
        (project / 'scripts').mkdir()
        (project / 'places.project.json').write_text(json.dumps({
            'schema': 'places-project', 'project_format': 1, 'id': 'release_guard',
            'name': 'Release guard', 'entry_level': 'entry',
            'levels': [{'id': 'entry', 'source': 'levels/entry.json'}],
        }))
        (project / 'levels/entry.json').write_text(json.dumps({
            'schema': 'places-level', 'format_version': 1, 'id': 'entry',
            'name': 'Release guard', 'spawn': {'x': 0, 'z': 0},
            'scripts': ['scripts/guard.luau'],
        }))
        output = project / 'levels/entry.placesmap'
        for name, source, expected in cases:
            (project / 'scripts/guard.luau').write_text(source)
            started = time.monotonic()
            result = subprocess.run([
                str(compiler), 'build', str(project), '--asset-root', str(assets),
                '--progress', 'off', '--json',
            ], cwd=directory, env=environment, capture_output=True, text=True, timeout=15, check=False)
            combined = result.stdout + result.stderr
            passed = result.returncode == 1 and (expected is None or expected in combined) and not output.exists()
            report['refusals'].append({'case': name, 'source': source, 'expected': expected,
                                      'passed': passed, 'exit': result.returncode,
                                      'seconds': time.monotonic() - started,
                                      'package_created': output.exists(), 'output': combined[:4096]})
            if not passed:
                if args.report:
                    args.report.write_text(json.dumps(report, indent=2) + '\n')
                raise SystemExit(f'Luau release refusal failed: exit={result.returncode}, expected={expected!r}\n{combined[:4096]}')
    report['passed'] = True
    if args.report:
        args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(f'Luau production error boundary: {len(cases)} bounded refusals and {len(args.project)} finished-project checks passed; no map baked')


if __name__ == '__main__':
    main()
