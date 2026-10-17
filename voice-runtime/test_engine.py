import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("voice_engine", Path(__file__).with_name("engine.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ContractTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.engine = module.Engine(self.root / "model", root=self.root)

    def tearDown(self):
        self.temporary.cleanup()

    def request(self, **overrides):
        reference = self.root / "voice-profiles/reference.wav"
        reference.parent.mkdir(exist_ok=True)
        reference.touch()
        return {"id": "one", "op": "synthesize", "text": "A suggestion.",
                "reference_text": "A reference.", "reference_audio": str(reference),
                "output": str(self.root / "voice-output/reply.wav"), **overrides}

    def test_status_does_not_load_model(self):
        self.assertEqual(self.engine.handle({"op": "status"}), {"loaded": False})

    def test_incomplete_model_fails_before_import(self):
        with self.assertRaisesRegex(ValueError, "missing or incomplete"):
            self.engine.handle(self.request())

    def test_output_cannot_escape_private_directory(self):
        for path in ("/tmp/out.wav", str(self.root / "voice-output/../../out.wav")):
            with self.assertRaisesRegex(ValueError, "outside"):
                self.engine.handle(self.request(output=path))

    def test_reference_cannot_escape_via_symlink(self):
        other = self.root / "other.wav"
        other.touch()
        self.request()
        link = self.root / "voice-profiles/link.wav"
        link.symlink_to(other)
        with self.assertRaisesRegex(ValueError, "outside"):
            self.engine.handle(self.request(reference_audio=str(link)))

    def test_reject_missing_or_unbounded_text(self):
        for value in ("", " " * 3, "x" * 2001, 3, None):
            with self.assertRaises(ValueError):
                self.engine.handle(self.request(text=value))

    def test_invalid_requests_do_not_kill_protocol(self):
        incoming = io.StringIO('invalid\n[]\n{"id":2,"op":"status"}\n')
        outgoing = io.StringIO()
        module.serve(self.engine, incoming, outgoing)
        replies = [json.loads(line) for line in outgoing.getvalue().splitlines()]
        self.assertEqual([r["ok"] for r in replies], [False, False, True])
        self.assertEqual(replies[-1]["id"], 2)

    def test_test_root_is_explicit(self):
        engine = module.Engine(self.root / "model", test_root=self.root)
        self.assertEqual(engine.reference_root, self.root)
        self.assertEqual(engine.output_root, self.root)

    def test_offline_environment(self):
        self.assertEqual(module.os.environ["HF_HUB_OFFLINE"], "1")
        self.assertEqual(module.os.environ["TRANSFORMERS_OFFLINE"], "1")


if __name__ == "__main__":
    unittest.main()
