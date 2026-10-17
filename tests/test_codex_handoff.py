import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('handoff', Path(__file__).parents[1] / 'scripts/codex-handoff.py')
handoff = importlib.util.module_from_spec(spec)
spec.loader.exec_module(handoff)


class HandoffTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / 'project'
        self.root.mkdir()
        (self.root / 'notes.md').write_text('My selected context.')

    def test_only_selected_files(self):
        (self.root / 'private.txt').write_text('not selected')
        result = handoff.prepare(self.root, 'Prepare a boundary conversation.', ['notes.md'])
        self.assertEqual(result['status'], 'draft_not_sent')
        self.assertEqual(result['context'], [{'path': 'notes.md', 'text': 'My selected context.'}])

    def test_path_escape(self):
        for name in ['../outside.txt', '/etc/hosts']:
            with self.assertRaises(ValueError):
                handoff.prepare(self.root, 'goal', [name])

    def test_symlink_escape_and_hidden_secret(self):
        outside = self.root.parent / 'outside.txt'
        outside.write_text('private')
        (self.root / 'alias.txt').symlink_to(outside)
        (self.root / '.env').write_text('SECRET=value')
        (self.root / 'secret-alias.txt').symlink_to(self.root / '.env')
        for name in ['alias.txt', '.env', 'secret-alias.txt']:
            with self.assertRaises(ValueError):
                handoff.prepare(self.root, 'goal', [name])

    def test_size_limit(self):
        (self.root / 'large.txt').write_text('x' * (handoff.MAX_FILE_BYTES + 1))
        with self.assertRaises(ValueError):
            handoff.prepare(self.root, 'goal', ['large.txt'])

    def test_binary_rejected(self):
        (self.root / 'binary.txt').write_bytes(b'zero\x00byte')
        with self.assertRaises(ValueError):
            handoff.prepare(self.root, 'goal', ['binary.txt'])

    def test_goal_is_data_not_executable(self):
        goal = '$(touch SHOULD_NOT_EXIST); explain my next step'
        self.assertEqual(handoff.prepare(self.root, goal)['goal'], goal)
        self.assertFalse((self.root / 'SHOULD_NOT_EXIST').exists())


if __name__ == '__main__':
    unittest.main()
