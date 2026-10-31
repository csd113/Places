"""Tiny receipt refusal regression: filenames once falsely qualified idle as float.

No scene, assets, GPU, files or baking; existing quality checks cannot see clips.
"""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('character_capture', ROOT / 'tools/bench/check_character_capture.py')
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class CharacterCaptureTests(unittest.TestCase):
    def test_actual_state_and_series_fail_closed(self):
        expected = dict(level='real', drawable=[960, 640], captures=[
            dict(name='early', frame=60, actors=[dict(id='ghost', clip='float_forward', paused=False, opacity=[.5, .5])]),
            dict(name='wrapped', frame=252, actors=[dict(id='ghost', clip='float_forward', paused=False, opacity=[0, 0])])],
            series=[dict(id='ghost', captures=['early', 'wrapped'], pose_changed=True, clock_wrap=True)])
        actor = dict(id='ghost', cue=dict(clip='float_forward', time=3.5, duration=4, paused=False),
                     opacity=.5, mesh_visible=True, light_position=[0, 1, 0], origin=[0, 0, 0],
                     posed_bounds=dict(min=[-.5, .1, -.4], max=[.5, 1.8, .4]))
        states = [dict(capture_path='early.png', ready_frame=60, character_animations=[actor]),
                  dict(capture_path='wrapped.png', ready_frame=252, character_animations=[copy.deepcopy(actor)])]
        hidden = states[1]['character_animations'][0]
        hidden.update(opacity=0, mesh_visible=True, light_position=None)
        hidden['cue']['time'] = .7
        hidden['posed_bounds']['max'][1] = 1.85
        draw = dict(event='capture', mode='final', level='real',
                    frame=dict(drawable=[960, 640], diagnostic_identity_post=False),
                    capture_submission=dict(draw_calls=1, submitted_indices=3))
        def run(values):
            log = '\n'.join('[lighting-capture-state] ' + json.dumps(state) + '\n[visual-diagnostic] ' + json.dumps(draw) for state in values)
            return CHECK.inspect(log, expected)
        self.assertEqual(len(run(states)), 2)
        mutations = [
            lambda a: a['cue'].update(clip='idle'),
            lambda a: a['cue'].update(paused=True),
            lambda a: a['cue'].update(time=float('nan')),
            lambda a: a.update(opacity=.00001),
            lambda a: a.pop('mesh_visible'),
            lambda a: a.update(light_position=[0, 1, 0]),
            lambda a: a.pop('light_position'),
            lambda a: a.update(posed_bounds=copy.deepcopy(actor['posed_bounds'])),
        ]
        for mutate in mutations:
            with self.subTest(mutation=mutate):
                values = copy.deepcopy(states); mutate(values[1]['character_animations'][0])
                with self.assertRaises(ValueError): run(values)
        values = copy.deepcopy(states); values[1]['character_animations'] = []
        with self.assertRaises(ValueError): run(values)
        with self.assertRaises(ValueError): CHECK.inspect('', expected)
