"""Derive the explicit Winter severe-weather control from maintained source.

Shared by source checks and opt-in native acceptance; never rebuilds a map.
"""
from __future__ import annotations

import copy
from pathlib import Path

SEVERE_WEATHER = Path(__file__).resolve().parent / 'config/winter_severe_weather.json'


def build_severe_review(level: dict) -> dict:
    """Derive an isolated severe control without maintaining another village."""
    severe = copy.deepcopy(level)
    severe.update(id='blizzard_review', name='Winter Blizzard Review',
                  weather=copy.deepcopy(level['weather_alternate']),
                  weather_alternate=copy.deepcopy(level['weather']))
    button = next(prop for prop in severe['props'] if prop['id'] == 'winter_blizzard_button')
    next(c for c in button['components'] if c['component'] == 'interactable')['prompt'] = 'Stop blizzard'
    next(c for c in button['components'] if c['component'] == 'state')['value'] = True
    # The severe review reverses the endpoint pair; preserve each control's
    # meaning rather than treating strength one as severe in that fixture.
    severe['weather_cycle'] = {
        **level['weather_cycle'],
        'min_strength': round(1 - level['weather_cycle']['max_strength'], 4),
        'max_strength': round(1 - level['weather_cycle']['min_strength'], 4),
    }
    for placed in severe['props']:
        for binding in placed.get('bindings', []):
            for action in binding['actions']:
                if action['action'] == 'set_weather_strength':
                    action['strength'] = round(1 - action['strength'], 4)
    return severe
