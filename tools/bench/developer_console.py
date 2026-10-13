#!/usr/bin/env python3
"""Opt-in SDL/Metal console acceptance; reuses existing packages, never bakes maps.

Actions pass through the player's normal native key/text event handler. Output
is isolated under --out; screenshots capture actual presented frames. Timing
comparisons use tools/bench/frame_time.py separately after builds are idle.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]

def plan(mode: str) -> list[dict]:
    steps = []
    anchor = 'ready:office_demo'
    def action(ms, kind, **fields):
        steps.append(dict(after=anchor, delay_ms=ms, action=dict(kind=kind, **fields)))
    def key(ms, name, pressed=True):
        action(ms, 'key', key=name, pressed=pressed)
    def command(ms, text):
        action(ms, 'text', text=text, submit=True)
    if mode == 'settings':
        action(0, 'escape'); key(100, 'Down'); key(200, 'Return')
        key(300, 'Down'); key(400, 'Down'); key(500, 'Return')
        for i in range(17): key(600 + i * 50, 'Down')
        action(1550, 'capture', name='setting-off')
        key(1700, 'Return'); action(1950, 'capture', name='setting-on')
        action(2100, 'escape'); action(2200, 'escape'); action(2300, 'escape')
        key(2450, '`'); command(2550, '/help'); action(2850, 'capture', name='help')
        action(3100, 'escape'); key(3250, 'W'); key(3550, 'W', False)
        key(3700, '`'); action(3850, 'console_enabled', enabled=False)
        key(4000, '`'); action(4300, 'quit')
    elif mode == 'input':
        key(0, '`'); action(100, 'console_enabled', enabled=True)
        key(200, 'W'); key(500, '`')
        command(650, '/help'); key(750, 'W'); key(850, 'Space')
        key(950, 'E'); action(1100, 'text', text='wasd e /noclip')
        action(1400, 'capture', name='typing')
        for i in range(15): key(1450 + 5 * i, 'Backspace')
        command(1600, '/give core:rubber_duck'); action(1900, 'capture', name='give')
        command(2000, '/give core:not_an_item'); command(2100, '/give core:tex_carpet_beige_01')
        command(2200, '/give spooner-man'); command(2300, '/give ../../file')
        key(2400, 'Up'); key(2500, 'Down'); action(2700, 'escape')
        # Held gameplay presses typed into the console must have been cleared.
        action(3100, 'capture', name='walking-restored')
        key(3200, 'W'); key(3600, 'W', False)
        key(3750, '`'); command(3900, '/clear'); action(4150, 'capture', name='clear')
        action(4300, 'console_enabled', enabled=False); action(4500, 'quit')
    elif mode == 'preview':
        action(0, 'console_enabled', enabled=True); key(100, '`')
        command(200, '/give core:rubber_duck'); action(350, 'escape')
        key(450, 'Down'); key(1000, 'Down', False)
        action(1250, 'capture', name='prop-visible')
        key(1400, '`'); command(1500, '/give core:rubber_duck')
        action(1700, 'capture', name='overlap-refused'); action(1850, 'escape')
        key(1950, 'A'); key(2250, 'A', False); key(2400, '`')
        command(2500, '/give core:rubber_duck'); action(2700, 'escape')
        action(2950, 'capture', name='two-props')
        action(3100, 'console_enabled', enabled=False); action(3350, 'capture', name='previews-cleared')
        action(3550, 'quit')
    elif mode == 'recovery':
        action(0, 'console_enabled', enabled=True); key(100, '`'); command(200, '/noclip on')
        action(300, 'escape'); key(400, 'Space'); key(1050, 'Space', False)
        key(1150, '`'); action(1300, 'capture', name='inside-ceiling')
        command(1450, '/noclip off'); action(1700, 'capture', name='safe-return')
        command(1850, '/noclip'); action(1950, 'escape'); key(2050, 'D')
        key(9000, 'D', False); key(9100, '`'); command(9200, '/noclip off')
        action(9450, 'capture', name='void-return')
        command(9600, '/noclip on'); action(9750, 'console_enabled', enabled=False)
        action(10000, 'quit')
    elif mode == 'overflow':
        action(0, 'console_enabled', enabled=True); key(100, '`')
        for i in range(70): command(200 + 20*i, '/unknown_' + str(i) + '_' + 'x'*110)
        key(1800, 'PageUp'); action(2050, 'capture', name='scrolled')
        key(2150, 'Up'); action(2400, 'capture', name='history')
        command(2500, ''); action(2600, 'console_enabled', enabled=False); action(2800, 'quit')
    elif mode == 'transitions':
        action(0, 'console_enabled', enabled=True); key(100, '`'); command(200, '/noclip')
        action(350, 'focus', focused=False); action(500, 'focus', focused=True)
        key(650, '`'); action(750, 'load', level='retro_arcade_demo')
        anchor = 'ready:retro_arcade_demo'
        key(100, '`'); command(200, '/help'); action(400, 'escape'); action(500, 'escape')
        key(650, 'Down'); key(750, 'Down'); key(850, 'Return')
        key(1000, '`'); action(1150, 'load', level='office_demo')
        # The ready anchor refers to its first occurrence, so schedule reload
        # checks from the second map and allow bounded native preparation time.
        anchor = 'ready:retro_arcade_demo'
        key(4000, '`'); command(4200, '/noclip off'); action(4500, 'capture', name='reloaded')
        action(4800, 'quit')
    else:
        raise ValueError(mode)
    return steps

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release/places')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--mode', choices=['settings','input','preview','recovery','overflow','transitions'], required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    out = args.out.resolve(); out.mkdir(parents=True, exist_ok=False)
    settings = json.loads((ROOT/'tests/fixtures/native/retro-arcade-manifest.json').read_text())['settings']
    settings.update(window_width=1280, window_height=720, vsync=True, developer_console=False,
                    quality='high', texture_filtering='high', lightmaps='full', reflections='full')
    (out/'settings.json').write_text(json.dumps(settings, indent=2)+'\n')
    (out/'actions.json').write_text(json.dumps(plan(args.mode), indent=2)+'\n')
    env = {k:v for k,v in os.environ.items() if not k.startswith('PLACES_') and k != 'SDL_VIDEODRIVER'}
    env.update(PLACES_ASSET_ROOT=str(ROOT), PLACES_STATE_ROOT=str(out), PLACES_LEVEL='office_demo',
        PLACES_SPAWN='21,1.6,10,0', PLACES_BENCH='1', PLACES_VERBOSE='1',
        PLACES_BENCH_ACTIONS=str(out/'actions.json'), PLACES_LOAD_TRACE=str(out/'trace.jsonl'),
        PLACES_STATE_LOG=str(out/'player.csv'), PLACES_BENCH_OUT=str(out/'frames.csv'),
        PLACES_BENCH_FRAMES='3600', PLACES_BENCH_WARMUP='120')
    result = subprocess.run([str(binary)], cwd=ROOT, env=env, capture_output=True, text=True, timeout=120)
    (out/'native.log').write_text(result.stdout+result.stderr)
    events = [json.loads(x) for x in (out/'trace.jsonl').read_text().splitlines()]
    states = [dict(event=x['event'], elapsed_ms=x['elapsed_ms'], **json.loads(x['detail']))
              for x in events if x['event'] in ('action_result','console_command','world_committed')]
    receipt = dict(mode=args.mode, exit_code=result.returncode,
        binary_sha256=hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),
        screenshots=[x.name for x in out.glob('*.png')], states=states)
    (out/'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
    if result.returncode: raise RuntimeError(f'Native run exited {result.returncode}: {out}/native.log')
    if not states: raise RuntimeError('No native action evidence')
    print(f'{args.mode}: native exit 0, {len(states)} states, {len(receipt["screenshots"])} captures')

if __name__ == '__main__': main()
