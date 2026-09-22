import base64
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "mirror", Path(__file__).resolve().parents[1] / "deploy/updater/mirror.py"
)
mirror = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mirror)


class FakeGitHub:
    repo = "hatsune-miku/anydrop"

    def __init__(self, version="0.1.2-rc.10.1"):
        self.version = version
        self.tag = "rc-v" + version
        self.names = ["AnyDrop_0.1.2_x64-setup.exe", "AnyDrop.app.tar.gz"]
        self.signature = base64.b64encode(b"test signature").decode()
        self.data = {"version": version, "notes": "test", "platforms": {}}
        for target in mirror.TARGETS:
            name = self.names[0 if target.startswith("windows") else 1]
            self.data["platforms"][target] = {
                "url": f"https://github.com/{self.repo}/releases/download/{self.tag}/{name}",
                "signature": self.signature,
            }
        self.release = {
            "tag_name": self.tag, "draft": False, "prerelease": True,
            "assets": [{"name": name} for name in ["latest.json", *self.names]],
        }
        self.calls = []
        self.fail = None

    def releases(self):
        return [self.release]

    def download(self, tag, name, destination):
        self.calls.append(name)
        if name == self.fail:
            raise OSError("download interrupted")
        (destination / name).write_bytes(
            json.dumps(self.data).encode() if name == "latest.json" else name.encode()
        )


class MirrorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name)
        self.github = FakeGitHub()
        self.verified = []

    def verify(self, file, signature, key):
        self.assertEqual(file.read_bytes(), file.name.encode())
        self.assertEqual(signature, self.github.signature)
        self.verified.append(file.name)

    def run_mirror(self, verifier=None):
        return mirror.mirror(self.output, "https://updates.example.test", Path("public.key"),
                             self.github, verifier or self.verify)

    def test_verified_complete_release_is_published_and_mac_download_is_reused(self):
        self.assertIn("Published verified RC", self.run_mirror())
        latest = json.loads((self.output / "rc/latest.json").read_text())
        self.assertEqual(set(latest["platforms"]), mirror.TARGETS)
        self.assertEqual(sorted(self.verified), sorted(self.github.names))
        self.assertEqual(len(self.github.calls), 3)
        for info in latest["platforms"].values():
            self.assertTrue(info["url"].startswith("https://updates.example.test/rc/0.1.2-rc.10.1/"))
        self.assertTrue((self.output / "rc/0.1.2-rc.10.1/sha256sums.txt").exists())
        self.assertIn("Already serving", self.run_mirror())
        self.assertEqual(len(self.github.calls), 3)

    def test_failed_download_or_signature_preserves_current_manifest(self):
        for failure in ["download", "signature"]:
            with self.subTest(failure=failure):
                channel = self.output / "rc"
                channel.mkdir(exist_ok=True)
                original = b'{"version":"0.1.2-rc.9.1"}'
                (channel / "latest.json").write_bytes(original)
                self.github.fail = self.github.names[1] if failure == "download" else None

                def invalid_signature(*args):
                    raise ValueError("invalid signature")

                with self.assertRaises((OSError, ValueError)):
                    self.run_mirror(invalid_signature if failure == "signature" else None)
                self.assertEqual((channel / "latest.json").read_bytes(), original)
                self.assertFalse((channel / self.github.version).exists())
                self.assertFalse(list(channel.glob(".staging-*")))

    def test_older_release_never_rolls_back_current_version(self):
        (self.output / "rc").mkdir()
        original = b'{"version":"0.1.2-rc.11.1"}'
        (self.output / "rc/latest.json").write_bytes(original)
        self.assertIn("Already serving", self.run_mirror())
        self.assertEqual(self.github.calls, [])
        self.assertEqual((self.output / "rc/latest.json").read_bytes(), original)

    def test_missing_platform_and_untrusted_artifact_url_fail_before_download(self):
        original = copy.deepcopy(self.github.data)
        mutations = [
            lambda data: data["platforms"].pop("darwin-x86_64"),
            lambda data: data["platforms"]["windows-x86_64"].update(url="https://other.example/file.exe"),
            lambda data: data.update(version="0.1.2-rc.12.1"),
            lambda data: data["platforms"]["windows-x86_64"].update(signature="invalid base64"),
        ]
        for mutate in mutations:
            self.github.data = copy.deepcopy(original)
            self.github.calls = []
            mutate(self.github.data)
            with self.assertRaises(ValueError):
                self.run_mirror()
            self.assertEqual(self.github.calls, ["latest.json"])
            self.assertFalse((self.output / "rc/latest.json").exists())

    def test_unsigned_legacy_releases_are_ignored(self):
        self.github.release["assets"] = []
        self.assertIn("No signed RC", self.run_mirror())
        self.assertEqual(self.github.calls, [])

    def test_existing_immutable_version_cannot_be_replaced(self):
        destination = self.output / "rc" / self.github.version
        destination.mkdir(parents=True)
        (destination / self.github.names[0]).write_bytes(b"existing artifact")
        with self.assertRaisesRegex(ValueError, "immutable"):
            self.run_mirror()
        self.assertFalse((self.output / "rc/latest.json").exists())
        self.assertEqual((destination / self.github.names[0]).read_bytes(), b"existing artifact")


if __name__ == "__main__":
    unittest.main()
