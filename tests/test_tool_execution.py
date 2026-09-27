"""Fast known-answer and spawn regression checks; no production files written."""
import multiprocessing
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import signal
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
from execution import ordered_map, worker_count, atomic_write


def square(value):
    return value, value * value


def broken(value):
    raise ValueError(f'bad job {value}')


def abrupt(value):
    os._exit(7)


def slow_or_broken(value):
    if value == 0:
        time.sleep(.2)
        raise ValueError("early failure")
    time.sleep(20)
    return value


def slow(value):
    time.sleep(20)
    return value


def bad_initializer():
    raise ValueError("initializer failed")


class ExecutionTests(unittest.TestCase):
    def test_order_coverage_empty_tiny_uneven_and_repetition(self):
        for workers in (1, 2, 12):
            for jobs in ([], [4], list(range(17)), [2, 2, 1]):
                self.assertEqual(ordered_map(square, jobs, workers), [square(x) for x in jobs])
        self.assertFalse(multiprocessing.active_children())

    def test_budget_and_invalid_counts(self):
        previous = os.environ.get('PLACES_TOOL_WORKERS')
        try:
            os.environ['PLACES_TOOL_WORKERS'] = '2'
            self.assertLessEqual(worker_count(12), 2)
            for value in (0, -1, 'bad'):
                with self.assertRaises(ValueError):
                    worker_count(value)
        finally:
            if previous is None:
                os.environ.pop('PLACES_TOOL_WORKERS', None)
            else:
                os.environ['PLACES_TOOL_WORKERS'] = previous

    def test_worker_error_and_abrupt_exit_cleanup(self):
        with self.assertRaisesRegex(ValueError, 'bad job'):
            ordered_map(broken, [1, 2], 2)
        from concurrent.futures.process import BrokenProcessPool
        with self.assertRaises(BrokenProcessPool):
            ordered_map(abrupt, [1, 2], 2)
        self.assertFalse(multiprocessing.active_children())

    def test_failure_does_not_wait_for_other_running_jobs(self):
        start = time.monotonic()
        with self.assertRaisesRegex(ValueError, "early failure"):
            ordered_map(slow_or_broken, [1, 0], 2)
        self.assertLess(time.monotonic() - start, 5)
        self.assertFalse(multiprocessing.active_children())

    def test_failed_initializer_and_cancellation(self):
        from concurrent.futures.process import BrokenProcessPool
        with self.assertRaises(BrokenProcessPool):
            ordered_map(square, [1, 2], 2, initializer=bad_initializer)
        process = subprocess.Popen([sys.executable, '-m', 'tests.test_tool_execution', '--interrupt-probe'],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=ROOT)
        try:
            self.assertEqual(process.stdout.readline().strip(), 'ready')
            time.sleep(.4)
            process.send_signal(signal.SIGINT)
            stdout, _ = process.communicate(timeout=5)
            self.assertIn('children=0', stdout)
            self.assertNotEqual(process.returncode, 0)
        finally:
            if process.poll() is None:
                process.kill()
            process.wait()

    def test_lattice_cache_preserves_hash_values(self):
        from tools.textures.artkit import hash01, lattice_hash01
        for seed in (-1, 0, 7, 65537):
            for x in range(-3, 19):
                for y in range(-3, 19):
                    self.assertEqual(lattice_hash01(x, y, seed), hash01(x, y, seed))
        before = lattice_hash01.cache_info().hits
        lattice_hash01(0, 0, 7)
        self.assertGreater(lattice_hash01.cache_info().hits, before)

    def test_prop_builder_rejects_escaping_catalog_paths(self):
        from tools.props import build
        with self.assertRaises(SystemExit):
            build.model_path({'id': 'bad', 'model': '../../outside.glb'})
        self.assertTrue(Path(build.model_path({'id':'safe', 'model':'core/props/safe.glb'})).is_relative_to(ROOT/'assets'))

    def test_atomic_replace_has_no_leftover(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'result'
            path.write_bytes(b'old')
            atomic_write(path, b'new')
            self.assertEqual(path.read_bytes(), b'new')
            self.assertEqual(list(Path(directory).iterdir()), [path])

    def test_downsample_known_answer_and_parallel(self):
        from tools.props.resize_embedded_textures import box_downsample
        source = bytearray([0, 4, 8, 255, 4, 8, 12, 255, 8, 12, 16, 255, 12, 16, 20, 255])
        self.assertEqual(box_downsample(2, 2, source, 1), (1, 1, bytearray([6, 10, 14, 255])))
        source = bytearray((i * 19) % 256 for i in range(17 * 13 * 4))
        expected = box_downsample(17, 13, source, 7, 1)
        for workers in (2, 12):
            self.assertEqual(box_downsample(17, 13, source, 7, workers), expected)
        with self.assertRaises(ValueError):
            box_downsample(0, 1, bytearray(), 2)

    def test_image_metrics_known_answers(self):
        from tools.props.resize_embedded_textures import encode_png
        from tools.bench.check_holes import near_black_fraction
        from tools.bench.compare_captures import compare
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory) / 'a.png', Path(directory) / 'b.png'
            a.write_bytes(encode_png(2, 2, bytes([0,0,0,255, 255,255,255,255] * 2)))
            self.assertEqual(near_black_fraction(str(a), step=1), (.5, 2, 4))
            a.write_bytes(encode_png(1, 1, bytes([0,0,0,255])))
            b.write_bytes(encode_png(1, 1, bytes([9,3,1,255])))
            self.assertEqual(compare(a, b, 8), (9.0, 100.0, 9, 1))

    def test_contact_failure_does_not_publish_partial_sheets(self):
        from unittest.mock import patch
        from tools.entities import render_contact_sheets as contact
        asset = ROOT/'assets/entities/rat/model/rat.glb'
        result = {'asset': str(asset), 'ok': False, 'error': 'worker failed'}
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(contact, 'render_asset', return_value=result), patch.object(contact, 'compose_sheet') as publish:
                self.assertEqual(contact.main(['--glb', str(asset), '--workers', '1',
                                               '--blender-threads', '1', '--out', directory]), 1)
                publish.assert_not_called()
                self.assertEqual(list(Path(directory).iterdir()), [])

    def test_visual_cli_keeps_same_named_build_captures_separate(self):
        from tools.props.resize_embedded_textures import encode_png
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = []
            for side, color in [('before', 0), ('after', 255)]:
                binary = root / side / 'places'
                binary.parent.mkdir()
                png = encode_png(1, 1, bytes([color, 0, 0, 255]))
                binary.write_text(f'#!{sys.executable}\nimport os\nfrom pathlib import Path\nassert os.environ["PLACES_BENCH"] == "1"\nPath(os.environ["PLACES_CAPTURE"]).write_bytes({png!r})\n')
                binary.chmod(0o755)
                binaries.append(binary)
            command = [sys.executable, str(ROOT/'tools/bench/visual_check.py'),
                       '--baseline', str(binaries[0]), '--current', str(binaries[1]),
                       '--out', str(root/'captures'), '--workers', '2', '--strict']
            for workers in (1, 4, 12):
                command[-2] = str(workers)
                result = subprocess.run(command, capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertEqual(result.stdout.count('REGRESSION'), 11)
            command[command.index('--current') + 1] = str(binaries[0])
            result = subprocess.run(command, capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(len(list((root/'captures'/'baseline').glob('*.png'))), 11)
            self.assertEqual(len(list((root/'captures'/'current').glob('*.png'))), 11)

    def test_capture_rejects_stale_outputs_and_failed_processes(self):
        from unittest.mock import patch
        from tools.bench.visual_check import capture
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stale = root / 'places__shot.png'
            stale.write_bytes(b'stale')
            with patch('tools.bench.visual_check.subprocess.run',
                       return_value=subprocess.CompletedProcess([], 1, '', 'failed')):
                with self.assertRaises(SystemExit):
                    capture(Path('/example/places'), ('shot', 'demo', {}), root, root)
            self.assertFalse(stale.exists())

    def test_seam_batch_error_publishes_nothing(self):
        from tools.props.resize_embedded_textures import encode_png
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory) / 'valid.png', Path(directory) / 'invalid.png'
            original = encode_png(8, 8, bytes([60,70,80,123] * 64))
            a.write_bytes(original); b.write_bytes(b'invalid')
            result = subprocess.run([sys.executable, str(ROOT/'tools/textures/seam_repair.py'),
                                     '--workers', '2', '--repair', str(a), str(b)], capture_output=True, timeout=20)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(a.read_bytes(), original)
            self.assertEqual(b.read_bytes(), b'invalid')


if __name__ == '__main__':
    if '--interrupt-probe' in sys.argv:
        print('ready', flush=True)
        try:
            ordered_map(slow, [1, 2], 2)
        except KeyboardInterrupt:
            print(f'children={len(multiprocessing.active_children())}', flush=True)
            raise SystemExit(130)
    else:
        unittest.main()
