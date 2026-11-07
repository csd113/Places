"""Tiny deterministic regressions for per-file rates and fail-closed import.

Value/cost: prevents silent 44.1 kHz restoration of compact loops, decoded-cap
publication, partial writes after renderer failure, and unknown original source
fallthrough. Uses only 64-frame PCM fixtures and metadata, no maps or devices.
"""
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import wave

SPEC = importlib.util.spec_from_file_location('places_audio_build', Path(__file__).resolve().parents[1] / 'tools/audio/build.py')
AUDIO = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIO)


def pcm(rate=16000):
    result = io.BytesIO()
    with wave.open(result, 'wb') as wav:
        wav.setparams((1, 2, rate, 0, 'NONE', 'not compressed'))
        wav.writeframes(b'\xe8\x03' * 64)
    return result.getvalue()


def clip(name='tone.wav', **fields):
    return dict(file=name, source='fixture', source_file='tone', **fields)


class AudioImportTests(unittest.TestCase):
    def test_explicit_compact_rate_and_pinned_bytes_cannot_silently_default(self):
        data = pcm()
        record = AUDIO.wav_record(data, clip(sample_rate=16000), data)
        self.assertEqual((record['sample_rate'], record['frames'], record['sample_bits']), (16000, 64, 16))
        with self.assertRaisesRegex(ValueError, 'format/duration'):
            AUDIO.wav_record(data, clip(), data)
        with self.assertRaisesRegex(ValueError, 'pinned rendered'):
            AUDIO.wav_record(data, clip(sample_rate=16000, expected_sha256='changed'), data)
        for rate in (True, 7999, 48001):
            with self.subTest(rate=rate), self.assertRaisesRegex(ValueError, 'sample rate'):
                AUDIO.clip_rate(clip(sample_rate=rate))

    def test_decoded_bank_limit_is_checked_before_publication(self):
        records = [dict(file=f'{i}.wav', frames=1440000) for i in range(5)]
        records.append(dict(file='last.wav', frames=AUDIO.MAX_DECODED_BYTES // 8 - 7200000))
        AUDIO.preflight_bank(records)
        records[-1]['frames'] += 1
        with self.assertRaisesRegex(ValueError, 'decoded budget'):
            AUDIO.preflight_bank(records)
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            AUDIO.preflight_bank([records[0], records[0]])

    def test_failed_later_render_does_not_replace_earlier_asset_or_provenance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = pcm()
            (root/'one.wav').write_bytes(original)
            (root/'provenance.json').write_text('preserve provenance')
            authoring = dict(sources=[dict(id='fixture', archive='original')], clips=[clip('one.wav', sample_rate=16000), clip('two.wav', sample_rate=16000)])
            with patch.object(AUDIO, 'AUDIO', root), patch.object(AUDIO, 'original_audio', side_effect=[(original, original), ValueError('second renderer refused')]):
                with self.assertRaisesRegex(ValueError, 'second renderer'):
                    AUDIO.import_audio(authoring, root, False)
            self.assertEqual((root/'one.wav').read_bytes(), original)
            self.assertEqual((root/'provenance.json').read_text(), 'preserve provenance')
            self.assertFalse((root/'two.wav').exists())

    def test_unknown_original_source_cannot_fall_through_to_legacy_renderer(self):
        with self.assertRaisesRegex(ValueError, 'unknown original audio source'):
            AUDIO.original_audio(clip())
