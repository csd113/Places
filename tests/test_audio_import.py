"""Guard explicit WAV formats and the decoded-bank cap used by the shipped audit.

Uses only a 64-frame PCM fixture and metadata, no maps, devices or generation.
"""
import io
import unittest
import wave

from assets.inspection import audio as AUDIO


def pcm(rate=16000):
    result = io.BytesIO()
    with wave.open(result, 'wb') as wav:
        wav.setparams((1, 2, rate, 0, 'NONE', 'not compressed'))
        wav.writeframes(b'\xe8\x03' * 64)
    return result.getvalue()


def clip(name='tone.wav', **fields):
    return dict(file=name, source='fixture', source_file='tone', **fields)


class AudioBankInspectionTests(unittest.TestCase):
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

    def test_decoded_bank_limit_allows_exact_cap_and_rejects_overflow(self):
        records = [dict(file=f'{i}.wav', frames=1440000) for i in range(5)]
        records.append(dict(file='last.wav', frames=AUDIO.MAX_DECODED_BYTES // 8 - 7200000))
        AUDIO.preflight_bank(records)
        records[-1]['frames'] += 1
        with self.assertRaisesRegex(ValueError, 'decoded budget'):
            AUDIO.preflight_bank(records)
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            AUDIO.preflight_bank([records[0], records[0]])

if __name__ == "__main__":
    unittest.main()
