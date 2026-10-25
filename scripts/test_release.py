"""Offline release contracts: real Git trees and a subprocess vx fixture.

The fixture deliberately mutates vx.lock on setup AND execution. No release is
uploaded, and no Rust build or tool installation is needed by this suite.
"""

import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock
import urllib.error

import yaml

import release


REPO = Path(__file__).resolve().parents[1]
REAL_RUN = subprocess.run
FAKE_VX = '''
import json, os, pathlib, sys
cwd = pathlib.Path.cwd()
args = sys.argv[1:]
with open(os.environ["VX_CALLS"], "a") as log:
    log.write(json.dumps({"args": args, "cwd": str(cwd)}) + "\\n")
lock = cwd / "vx.lock"
text = lock.read_text().replace('original', 'generated')
if os.environ.get("VX_DRIFT"):
    text = text.replace('1.58.0', '9.99.0')
lock.write_text(text)
if args == ["setup"] and os.environ.get("DIRTY_SOURCE"):
    pathlib.Path(os.environ["DIRTY_SOURCE"]).write_text("dirty source")
if args[0] == "cargo":
    assert "--locked" in args
    assert "--allow-dirty" not in args
    source = pathlib.Path(args[args.index("--manifest-path") + 1]).parent
    assert source != cwd and not cwd.is_relative_to(source)
if os.environ.get("VX_FAIL"):
    sys.exit(17)
'''


class ReleaseFixture(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="msvc-release-test-")
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.root = self.base / "source with spaces"
        self.root.mkdir()
        files = {
            "Cargo.toml": '[package]\nname = "msvc-kit"\nversion = "1.2.3"\nedition = "2021"\n',
            "Cargo.lock": 'version = 3\n[[package]]\nname = "msvc-kit"\nversion = "1.2.3"\n',
            ".release-please-manifest.json": '{".": "1.2.3"}\n',
            "vx.toml": '[tools]\njust = "latest"\n',
            "vx.lock": '[metadata]\ngenerated_at = "original"\n[tools.just]\nversion = "1.58.0"\nsource = "original"\n',
            "src/lib.rs": "pub fn answer() -> u8 { 42 }\n",
            ".gitignore": "/target/\n",
            "CHANGELOG.md": '# Changelog\n\n## [1.2.3](https://example.org/compare/v1.2.2...v1.2.3) (2026-10-13)\n\n### Bug Fixes\n\n* Real fix\n\n## [1.2.2](https://example.org/compare/v1.2.1...v1.2.2)\n\n* Old fix\n',
        }
        for filename, content in files.items():
            path = self.root / filename
            path.parent.mkdir(exist_ok=True)
            path.write_text(content, encoding="utf-8")
        self.git("init", "-q")
        self.git("config", "user.name", "Release fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.commit()
        self.git("tag", "v1.2.3")
        self.fake = self.base / "fake_vx.py"
        self.fake.write_text(FAKE_VX, encoding="utf-8")
        self.calls = self.base / "calls.jsonl"
        self.addCleanup(mock.patch.stopall)
        mock.patch.dict(os.environ, {"VX_CALLS": str(self.calls), "RUNNER_TEMP": str(self.base)}).start()

    def git(self, *args):
        return release.git(self.root, *args)

    def commit(self):
        self.git("add", "--all")
        self.git("commit", "-qm", "fixture")

    def retag(self):
        self.commit()
        self.git("tag", "-f", "v1.2.3")

    def run_vx(self, args, **kwargs):
        if args[0] == "vx":
            args = [sys.executable, str(self.fake), *args[1:]]
        return REAL_RUN(args, **kwargs)

    def run_release(self, command="build"):
        with mock.patch.object(release.subprocess, "run", side_effect=self.run_vx):
            release.run_cargo(self.root, "v1.2.3", command)

    def call_log(self):
        return [json.loads(line) for line in self.calls.read_text().splitlines()]

    def test_setup_and_execution_mutations_stay_outside_source(self):
        before = (self.root / "vx.lock").read_bytes()
        self.run_release()
        self.assertEqual((self.root / "vx.lock").read_bytes(), before)
        self.assertEqual(self.git("status", "--porcelain"), "")
        calls = self.call_log()
        self.assertEqual(calls[0]["args"], ["setup"])
        self.assertEqual(calls[1]["args"], ["cargo", "build", "--locked", "--manifest-path", str(self.root / "Cargo.toml"), "--release"])
        self.assertEqual(calls[0]["cwd"], calls[1]["cwd"])
        self.assertFalse(Path(calls[0]["cwd"]).exists(), "temporary tool state must be cleaned up")

    @unittest.skipUnless(os.environ.get("MSVC_KIT_REAL_RELEASE_SMOKE") == "1",
                         "real vx/Cargo smoke runs in the Windows x64 CI job")
    def test_real_vx_cargo_build_and_clean_package(self):
        # A tiny, dependency-free crate exercises the actual release helper;
        # this never publishes or recompiles msvc-kit itself.
        self.assertIsNotNone(shutil.which("vx"))
        self.assertIsNotNone(shutil.which("cargo"))
        for filename in ("vx.toml", "vx.lock"):
            shutil.copyfile(REPO / filename, self.root / filename)
        self.retag()
        expected_head = self.git("rev-parse", "HEAD")
        expected_lock = (self.root / "vx.lock").read_bytes()
        release.run_cargo(self.root, "v1.2.3", "build")
        self.assertTrue((self.root / "target/release/libmsvc_kit.rlib").is_file())
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual((self.root / "vx.lock").read_bytes(), expected_lock)
        package_args = ["cargo", "package", "--locked", "--offline", "--no-verify",
                        "--manifest-path", str(self.root / "Cargo.toml")]
        REAL_RUN(package_args, cwd=self.base, check=True)
        crate = self.root / "target/package/msvc-kit-1.2.3.crate"
        with tarfile.open(crate) as archive:
            vcs = json.load(archive.extractfile("msvc-kit-1.2.3/.cargo_vcs_info.json"))
            packed_lock = archive.extractfile("msvc-kit-1.2.3/vx.lock").read()
        self.assertEqual(vcs["git"]["sha1"], expected_head)
        self.assertFalse(vcs["git"].get("dirty", False))
        self.assertEqual(packed_lock, expected_lock)
        (self.root / "src/lib.rs").write_text("pub fn answer() -> u8 { 43 }\n")
        dirty = REAL_RUN(package_args, cwd=self.base, capture_output=True, text=True)
        self.assertNotEqual(dirty.returncode, 0)
        self.assertIn("uncommitted", dirty.stderr.lower())
        self.assertIn("43", (self.root / "src/lib.rs").read_text())

    def test_publish_is_locked_and_does_not_allow_dirty(self):
        with mock.patch.object(release, "version_exists", return_value=False):
            self.run_release("publish")
        self.assertEqual(self.call_log()[1]["args"], ["cargo", "publish", "--locked", "--manifest-path", str(self.root / "Cargo.toml")])

    def test_dirty_tracked_source_is_rejected_without_reset(self):
        source = self.root / "src/lib.rs"
        source.write_text("changed source")
        with self.assertRaisesRegex(RuntimeError, "dirty"):
            self.run_release()
        self.assertEqual(source.read_text(), "changed source")
        self.assertFalse(self.calls.exists())

    def test_staged_source_is_rejected(self):
        (self.root / "src/lib.rs").write_text("staged source")
        self.git("add", "src/lib.rs")
        with self.assertRaisesRegex(RuntimeError, "dirty"):
            self.run_release()

    def test_untracked_source_is_rejected(self):
        (self.root / "src/untracked.rs").write_text("new source")
        with self.assertRaisesRegex(RuntimeError, "dirty"):
            self.run_release()

    def test_action_stage_lock_mutation_is_rejected(self):
        with (self.root / "vx.lock").open("a") as stream:
            stream.write("\n# changed by an earlier action\n")
        with self.assertRaisesRegex(RuntimeError, "dirty"):
            self.run_release()

    def test_setup_cannot_dirty_source_then_publish(self):
        with mock.patch.dict(os.environ, {"DIRTY_SOURCE": str(self.root / "src/lib.rs")}), \
                mock.patch.object(release, "version_exists", return_value=False):
            with self.assertRaisesRegex(RuntimeError, "dirty"):
                self.run_release("publish")
        self.assertEqual(len(self.call_log()), 1)

    def test_setup_cannot_change_locked_tool_versions(self):
        with mock.patch.dict(os.environ, {"VX_DRIFT": "1"}):
            with self.assertRaisesRegex(RuntimeError, "locked tool versions"):
                self.run_release()
        self.assertEqual(len(self.call_log()), 1)

    def test_setup_failure_propagates(self):
        with mock.patch.dict(os.environ, {"VX_FAIL": "1"}):
            with self.assertRaises(subprocess.CalledProcessError):
                self.run_release()
        self.assertEqual(len(self.call_log()), 1)

    def test_temp_directory_inside_source_is_rejected(self):
        with mock.patch.dict(os.environ, {"RUNNER_TEMP": str(self.root)}):
            with self.assertRaisesRegex(RuntimeError, "outside"):
                self.run_release()
        self.assertFalse(self.calls.exists())
        self.assertEqual(self.git("status", "--porcelain"), "")

    def test_tag_version_mismatch_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "does not match"):
            release.release_version(self.root, "v1.2.4")

    def test_checkout_must_be_exact_tag_commit(self):
        (self.root / "src/lib.rs").write_text("later change")
        self.commit()
        with self.assertRaisesRegex(RuntimeError, "HEAD is not"):
            self.run_release()

    def test_lock_and_release_manifest_versions_must_agree(self):
        for filename in ("Cargo.lock", ".release-please-manifest.json"):
            with self.subTest(filename=filename):
                path = self.root / filename
                original = path.read_text()
                path.write_text(original.replace("1.2.3", "1.2.4"))
                self.retag()
                with self.assertRaisesRegex(RuntimeError, "versions disagree"):
                    self.run_release()
                path.write_text(original)
                self.retag()

    def test_existing_version_never_runs_setup_or_publish(self):
        with mock.patch.object(release, "version_exists", return_value=True):
            self.run_release("publish")
            self.run_release("publish")
        self.assertFalse(self.calls.exists())

    def test_registry_error_never_runs_setup_or_publish(self):
        with mock.patch.object(release, "version_exists", side_effect=RuntimeError("registry unavailable")):
            with self.assertRaisesRegex(RuntimeError, "registry unavailable"):
                self.run_release("publish")
        self.assertFalse(self.calls.exists())

    def test_notes_use_only_the_committed_matching_section(self):
        notes = release.release_notes(self.root, "v1.2.3")
        self.assertIn("v1.2.2...v1.2.3", notes)
        self.assertIn("Real fix", notes)
        self.assertNotIn("Old fix", notes)

    def test_downloaded_asset_and_notes_stay_outside_clean_checkout(self):
        # Reproduce the release job's ordering, even without an *.exe ignore.
        artifacts = self.base / "artifacts"
        artifacts.mkdir()
        (artifacts / "msvc-kit-x86_64-windows.exe").write_bytes(b"fixture binary")
        output = self.base / "release-notes.md"
        result = REAL_RUN(
            [sys.executable, str(REPO / "scripts/release.py"), "notes", "--root", str(self.root),
             "--tag", "v1.2.3", "--output", str(output)],
            text=True, capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Real fix", output.read_text())
        self.assertEqual(self.git("status", "--porcelain"), "")

    def test_notes_reject_missing_duplicate_and_self_comparison(self):
        path = self.root / "CHANGELOG.md"
        original = path.read_text()
        for text in ("# Empty\n", original + original, original.replace("v1.2.2...v1.2.3", "v1.2.3...v1.2.3")):
            with self.subTest(text=text):
                path.write_text(text)
                self.retag()
                with self.assertRaises(RuntimeError):
                    release.release_notes(self.root, "v1.2.3")


class RegistryTests(unittest.TestCase):
    def test_exact_version_endpoint_even_when_newer_releases_exist(self):
        with mock.patch.object(release.urllib.request, "urlopen", return_value=io.BytesIO(b'{"version":{"num":"1.2.3"}}')) as urlopen:
            self.assertTrue(release.version_exists("msvc-kit", "1.2.3"))
        self.assertEqual(urlopen.call_args.args[0].full_url, "https://crates.io/api/v1/crates/msvc-kit/1.2.3")

    def test_only_404_permits_publication(self):
        for code in (404, 403, 429, 500):
            with self.subTest(code=code), mock.patch.object(release.urllib.request, "urlopen", side_effect=urllib.error.HTTPError("url", code, "error", {}, None)):
                if code == 404:
                    self.assertFalse(release.version_exists("msvc-kit", "1.2.3"))
                else:
                    with self.assertRaises(urllib.error.HTTPError):
                        release.version_exists("msvc-kit", "1.2.3")

    def test_unexpected_response_fails_closed(self):
        with mock.patch.object(release.urllib.request, "urlopen", return_value=io.BytesIO(b'{"version":{"num":"1.2.4"}}')):
            with self.assertRaisesRegex(RuntimeError, "Unexpected"):
                release.version_exists("msvc-kit", "1.2.3")


# Floor declared by scripts/release.py ("Requires Python 3.11+"). A floor is not an
# exact-version assertion: Renovate only ever raises the pin, so this stays green
# across bumps while still rejecting a pin the release script cannot run on.
MIN_PYTHON = (3, 11)


def setup_python_pins(workflow_file):
    """Map every `actions/setup-python` pin in a workflow to {job: version}."""

    workflow = yaml.safe_load((REPO / ".github/workflows" / workflow_file).read_text())
    return {
        job_name: step["with"]["python-version"]
        for job_name, job in workflow["jobs"].items()
        for step in job.get("steps") or []
        if str(step.get("uses", "")).startswith("actions/setup-python@")
    }


class WorkflowTests(unittest.TestCase):
    def assertPythonPin(self, version, context):
        """Assert a `python-version` pin is a well-formed `X.Y` string.

        The value itself is never asserted literally: Renovate raises the pinned
        interpreter on its own schedule, so a literal here is a time bomb that
        only moves the failure to the next bump. Check the shape of the pin and
        let the cross-workflow comparison below pin down the value.
        """

        self.assertIsInstance(version, str, f"{context}: quote python-version so YAML keeps it a string")
        self.assertRegex(version, r"^\d+\.\d+$", f"{context}: expected a concrete X.Y pin")
        major, minor = (int(part) for part in version.split("."))
        self.assertGreaterEqual((major, minor), MIN_PYTHON, f"{context}: below the Python {MIN_PYTHON[0]}.{MIN_PYTHON[1]}+ floor in scripts/release.py")

    def test_workflow_uses_isolated_commands_and_preserves_notes(self):
        workflow = yaml.safe_load((REPO / ".github/workflows/release.yml").read_text())
        jobs = workflow["jobs"]
        for name, command in (("crates-publish", "publish"), ("build-release-binary", "build")):
            steps = jobs[name]["steps"]
            checkout = next(s for s in steps if s.get("uses", "").startswith("actions/checkout@"))
            self.assertIn("needs.release-please.outputs.tag_name || needs.check-tag.outputs.tag_name", checkout["with"]["ref"])
            setup = next(s for s in steps if s.get("uses", "").startswith("loonghao/vx@"))
            self.assertEqual(setup["with"]["setup"], "false")
            self.assertEqual(setup["with"]["tools"], "")
            self.assertFalse(setup["with"]["cache"])
            runs = "\n".join(s.get("run", "") for s in steps)
            self.assertIn(f'python scripts/release.py {command} --tag "$env:RELEASE_TAG"', runs)
            self.assertNotIn("vx setup", runs)
            self.assertNotIn("vx just", runs)
        # The CI workflow owns the pinned interpreter and release.yml has to follow it.
        # Renovate bumps every `actions/setup-python` step in one PR, so derive the
        # expectation from pr-checks.yml instead of restating a version that drifts.
        ci_pins = set(setup_python_pins("pr-checks.yml").values())
        self.assertEqual(len(ci_pins), 1, "pr-checks.yml must pin exactly one Python version")
        ci_python_version = ci_pins.pop()
        self.assertPythonPin(ci_python_version, "pr-checks.yml python-version")
        release_pins = set()
        for name in ("crates-publish", "build-release-binary", "github-release"):
            setup_python = next(s for s in jobs[name]["steps"] if s.get("uses", "").startswith("actions/setup-python@"))
            release_pins.add(setup_python["with"]["python-version"])
            self.assertPythonPin(setup_python["with"]["python-version"], f"release.yml:{name} python-version")
            self.assertEqual(setup_python["with"]["python-version"], ci_python_version)
        self.assertEqual(len(release_pins), 1, "release.yml must pin one Python version")
        steps = jobs["github-release"]["steps"]
        self.assertFalse(any("changelog-generator" in s.get("uses", "") for s in steps))
        action = next(s for s in steps if s.get("uses", "").startswith("ncipollo/release-action@"))["with"]
        self.assertTrue(action["omitBodyDuringUpdate"])
        self.assertTrue(action["omitNameDuringUpdate"])
        self.assertEqual(action["bodyFile"], "${{ runner.temp }}/release-notes.md")
        self.assertEqual(action["artifacts"], "${{ runner.temp }}/artifacts/msvc-kit-x86_64-windows.exe")
        download = next(s for s in steps if s.get("uses", "").startswith("actions/download-artifact@"))
        self.assertEqual(download["with"]["path"], "${{ runner.temp }}/artifacts/")
        self.assertNotIn("body", action)
        self.assertEqual(workflow["concurrency"], {"group": "release-${{ github.workflow }}", "cancel-in-progress": False})
        self.assertIn("gh release view", jobs["check-tag"]["steps"][0]["run"])
        recipe = (REPO / "justfile").read_text().split("publish-ci:\n", 1)[1].split("\n\n", 1)[0]
        self.assertIn("--locked", recipe)
        self.assertNotIn("--allow-dirty", recipe)


if __name__ == "__main__":
    unittest.main()
