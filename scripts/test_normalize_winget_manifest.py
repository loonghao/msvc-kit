"""Unit tests for ``normalize-winget-manifest.py``.

The script rewrites a manifest on a winget-pkgs pull request branch, so the
pure transformations it applies are covered here: header splitting, the
collapse rule, dependency parsing, the post-collapse validation, the license
URL rewrite and the ``--fork`` guard.

Run from the repository root, no third-party packages required::

    python3 -m unittest discover -s scripts -p "test_*.py"
"""

from __future__ import annotations

import base64
import contextlib
import importlib.util
import io
import os
import pathlib
import sys
import unittest
import unittest.mock
import urllib.error
import urllib.parse

SCRIPT_PATH = pathlib.Path(__file__).with_name("normalize-winget-manifest.py")


def _load_module():
    # The script is a standalone executable with a dashed name, so it cannot be
    # imported the regular way.
    spec = importlib.util.spec_from_file_location(
        "normalize_winget_manifest", SCRIPT_PATH
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


normalize = _load_module()

SCHEMA_HINT = (
    "# yaml-language-server: "
    "$schema=https://aka.ms/winget-manifest.installer.1.10.0.schema.json\n"
)
URL = "https://github.com/loonghao/msvc-kit/releases/download/v0.2.16/msvc-kit-x86_64-windows.exe"
SHA = "61F1BCFA8A296E73D41B3316EA03FA0E9AE84D6ACE96666FD00B62FBD0F64D8A"


def installer(architecture: str, dependency: str | None = None, **extra) -> dict:
    entry = {"Architecture": architecture, "InstallerUrl": URL, "InstallerSha256": SHA}
    if dependency is not None:
        entry["Dependencies"] = {
            "PackageDependencies": [{"PackageIdentifier": dependency}]
        }
    entry.update(extra)
    return entry


class SplitHeaderTest(unittest.TestCase):
    def test_keeps_the_schema_hint_komac_writes(self):
        content = f"{SCHEMA_HINT}\nPackageIdentifier: loonghao.msvc-kit\n"
        header, body = normalize.split_header(content)
        self.assertEqual(f"{SCHEMA_HINT}\n", header)
        self.assertEqual("PackageIdentifier: loonghao.msvc-kit\n", body)

    def test_header_and_body_reassemble_the_original(self):
        content = f"{SCHEMA_HINT}\nPackageIdentifier: loonghao.msvc-kit\nInstallers: []\n"
        header, body = normalize.split_header(content)
        self.assertEqual(content, header + body)

    def test_content_without_a_header(self):
        content = "PackageIdentifier: loonghao.msvc-kit\n"
        self.assertEqual(("", content), normalize.split_header(content))

    def test_comments_after_the_body_stay_in_the_body(self):
        content = "# hint\nPackageIdentifier: loonghao.msvc-kit\n# trailing note\n"
        header, body = normalize.split_header(content)
        self.assertEqual("# hint\n", header)
        self.assertEqual("PackageIdentifier: loonghao.msvc-kit\n# trailing note\n", body)

    def test_blank_line_between_header_and_body_belongs_to_the_header(self):
        content = f"{SCHEMA_HINT}\n\nPackageIdentifier: loonghao.msvc-kit\n"
        header, body = normalize.split_header(content)
        self.assertEqual(f"{SCHEMA_HINT}\n\n", header)
        self.assertEqual("PackageIdentifier: loonghao.msvc-kit\n", body)

    def test_crlf_line_endings_are_preserved(self):
        content = f"{SCHEMA_HINT[:-1]}\r\n\r\nPackageIdentifier: loonghao.msvc-kit\r\n"
        header, body = normalize.split_header(content)
        self.assertEqual(f"{SCHEMA_HINT[:-1]}\r\n\r\n", header)
        self.assertEqual("PackageIdentifier: loonghao.msvc-kit\r\n", body)
        self.assertEqual(content, header + body)

    def test_empty_content(self):
        self.assertEqual(("", ""), normalize.split_header(""))


class InstallersKeyTest(unittest.TestCase):
    def test_entries_with_the_same_url_and_hash_share_a_key(self):
        self.assertEqual(
            normalize.installers_key(installer("x64")),
            normalize.installers_key(installer("x64")),
        )

    def test_architecture_is_part_of_the_key(self):
        self.assertNotEqual(
            normalize.installers_key(installer("x64")),
            normalize.installers_key(installer("arm64")),
        )

    def test_url_is_part_of_the_key(self):
        self.assertNotEqual(
            normalize.installers_key(installer("x64")),
            normalize.installers_key(installer("x64", InstallerUrl="other")),
        )

    def test_hash_is_part_of_the_key(self):
        self.assertNotEqual(
            normalize.installers_key(installer("x64")),
            normalize.installers_key(installer("x64", InstallerSha256="other")),
        )

    def test_dependencies_are_not_part_of_the_key(self):
        self.assertEqual(
            normalize.installers_key(installer("x64", "Microsoft.VCRedist.2015+.x64")),
            normalize.installers_key(installer("x64", "Microsoft.VCRedist.2015+.x86")),
        )

    def test_missing_fields_are_tolerated(self):
        self.assertEqual((None, None, None), normalize.installers_key({}))


class DependencyArchitecturesTest(unittest.TestCase):
    def test_reads_the_vcredist_architecture(self):
        self.assertEqual(
            {"x64"},
            normalize.dependency_architectures(
                installer("x64", "Microsoft.VCRedist.2015+.x64")
            ),
        )

    def test_architecture_is_case_insensitive(self):
        self.assertEqual(
            {"x64"},
            normalize.dependency_architectures(
                installer("x64", "Microsoft.VCRedist.2015+.X64")
            ),
        )

    def test_non_vcredist_dependencies_are_ignored(self):
        self.assertEqual(
            set(),
            normalize.dependency_architectures(
                installer("x64", "Microsoft.WindowsAppRuntime.1.5")
            ),
        )

    def test_entry_without_dependencies(self):
        self.assertEqual(set(), normalize.dependency_architectures(installer("x64")))

    def test_dependencies_without_package_dependencies(self):
        self.assertEqual(set(), normalize.dependency_architectures({"Dependencies": {}}))


class CollapseTest(unittest.TestCase):
    def test_entries_without_duplicates_are_untouched(self):
        installers = [
            installer("x64", "Microsoft.VCRedist.2015+.x64"),
            installer("x86", "Microsoft.VCRedist.2015+.x86", InstallerUrl="other"),
        ]
        collapsed, changed = normalize.collapse(installers)
        self.assertFalse(changed)
        self.assertEqual(installers, collapsed)

    def test_collapses_the_release_that_winget_rejected(self):
        # komac paired one asset with the three installer entries of the
        # previously published manifest, producing three x64 entries that only
        # differ by their inherited VCRedist dependency.
        installers = [
            installer("x64", "Microsoft.VCRedist.2015+.x86"),
            installer("x64", "Microsoft.VCRedist.2015+.arm64"),
            installer("x64", "Microsoft.VCRedist.2015+.x64"),
        ]
        collapsed, changed = normalize.collapse(installers)
        self.assertTrue(changed)
        self.assertEqual([installers[2]], collapsed)

    def test_keeps_the_first_entry_when_no_dependency_matches(self):
        # Documented behaviour: with no architecture match the first entry wins.
        installers = [
            installer("x64", "Microsoft.VCRedist.2015+.x86"),
            installer("x64", "Microsoft.VCRedist.2015+.arm64"),
        ]
        collapsed, changed = normalize.collapse(installers)
        self.assertTrue(changed)
        self.assertEqual([installers[0]], collapsed)

    def test_a_matching_entry_replaces_an_earlier_one(self):
        installers = [
            installer("x64"),
            installer("x64", "Microsoft.VCRedist.2015+.x64"),
        ]
        collapsed, changed = normalize.collapse(installers)
        self.assertTrue(changed)
        self.assertEqual([installers[1]], collapsed)

    def test_preserves_the_order_of_the_surviving_entries(self):
        installers = [
            installer("x86", InstallerUrl="x86"),
            installer("x64", "Microsoft.VCRedist.2015+.x64"),
            installer("x64", "Microsoft.VCRedist.2015+.x86"),
            installer("arm64", InstallerUrl="arm64"),
        ]
        collapsed, changed = normalize.collapse(installers)
        self.assertTrue(changed)
        self.assertEqual([installers[0], installers[1], installers[3]], collapsed)

    def test_empty_manifest(self):
        self.assertEqual(([], False), normalize.collapse([]))


class NormalizationErrorTest(unittest.TestCase):
    def test_accepts_a_correct_collapse(self):
        installers = [
            installer("x64", "Microsoft.VCRedist.2015+.x86"),
            installer("x64", "Microsoft.VCRedist.2015+.x64"),
        ]
        collapsed, _ = normalize.collapse(installers)
        self.assertIsNone(normalize.normalization_error(installers, collapsed))

    def test_accepts_a_manifest_that_was_left_alone(self):
        installers = [installer("x64"), installer("x64", Scope="machine")]
        self.assertIsNone(normalize.normalization_error(installers, installers))

    def test_accepts_an_empty_result_for_an_empty_manifest(self):
        self.assertIsNone(normalize.normalization_error([], []))

    def test_rejects_an_entry_kept_twice(self):
        installers = [installer("x64"), installer("x64", Scope="machine")]
        self.assertIn(
            "more than once",
            normalize.normalization_error(installers, [installers[0], installers[0]])
            or "",
        )

    def test_rejects_an_entry_that_was_not_in_the_manifest(self):
        installers = [installer("x64")]
        self.assertIn(
            "not part of the manifest",
            normalize.normalization_error(installers, [installer("arm64")]) or "",
        )

    def test_rejects_reordered_entries(self):
        installers = [installer("x64", InstallerUrl="a"), installer("x64", InstallerUrl="b")]
        self.assertIn(
            "reordered",
            normalize.normalization_error(installers, list(reversed(installers))) or "",
        )

    def test_accepts_a_subset_that_keeps_the_original_order(self):
        installers = [
            installer("x64", InstallerUrl="a"),
            installer("x64", InstallerUrl="b"),
            installer("x64", InstallerUrl="c"),
        ]
        self.assertIsNone(
            normalize.normalization_error(installers, [installers[0], installers[2]])
        )


class SplitBlobRefTest(unittest.TestCase):
    def test_splits_a_github_blob_url(self):
        self.assertEqual(
            ("https://github.com/loonghao/msvc-kit", "HEAD", "LICENSE"),
            normalize.split_blob_ref(
                "https://github.com/loonghao/msvc-kit/blob/HEAD/LICENSE"
            ),
        )

    def test_splits_a_nested_license_path(self):
        self.assertEqual(
            ("https://github.com/o/r", "main", "docs/LICENSE.md"),
            normalize.split_blob_ref("https://github.com/o/r/blob/main/docs/LICENSE.md"),
        )

    def test_rejects_a_release_download_url(self):
        self.assertIsNone(
            normalize.split_blob_ref(
                "https://github.com/loonghao/msvc-kit/releases/download/v0.2.19/a.exe"
            )
        )

    def test_rejects_a_raw_url(self):
        self.assertIsNone(
            normalize.split_blob_ref(
                "https://raw.githubusercontent.com/loonghao/msvc-kit/main/LICENSE"
            )
        )

    def test_rejects_a_url_without_a_path(self):
        self.assertIsNone(normalize.split_blob_ref("https://github.com/o/r/blob/HEAD"))

    def test_rejects_a_url_with_an_empty_ref(self):
        self.assertIsNone(normalize.split_blob_ref("https://github.com/o/r/blob//LICENSE"))

    def test_rejects_a_non_github_host(self):
        self.assertIsNone(
            normalize.split_blob_ref("https://gitlab.com/o/r/blob/HEAD/LICENSE")
        )

    def test_rejects_plain_http(self):
        self.assertIsNone(
            normalize.split_blob_ref("http://github.com/o/r/blob/HEAD/LICENSE")
        )

    def test_rejects_a_github_lookalike_host(self):
        self.assertIsNone(
            normalize.split_blob_ref("https://github.com.evil.test/o/r/blob/HEAD/LICENSE")
        )


class NormalizeLicenseUrlTest(unittest.TestCase):
    """komac writes ``blob/HEAD/LICENSE``; ``HEAD`` is not a resolvable ref."""

    REFS = ["v0.2.19", "0.2.19", "main"]

    def test_rewrites_head_to_the_release_tag(self):
        self.assertEqual(
            "https://github.com/loonghao/msvc-kit/blob/v0.2.19/LICENSE",
            normalize.normalize_license_url(
                "https://github.com/loonghao/msvc-kit/blob/HEAD/LICENSE", self.REFS
            ),
        )

    def test_the_rewritten_url_no_longer_mentions_head(self):
        url = normalize.normalize_license_url(
            "https://github.com/loonghao/msvc-kit/blob/HEAD/LICENSE", self.REFS
        )
        self.assertNotIn("blob/HEAD", url)
        self.assertNotIn("HEAD", url.split("/blob/")[1])

    def test_the_rewritten_url_points_at_a_stable_ref(self):
        url = normalize.normalize_license_url(
            "https://github.com/loonghao/msvc-kit/blob/HEAD/LICENSE", self.REFS
        )
        ref = url.split("/blob/")[1].split("/")[0]
        self.assertIn(ref, self.REFS)

    def test_the_head_ref_is_matched_case_insensitively(self):
        self.assertEqual(
            "https://github.com/o/r/blob/v1/LICENSE",
            normalize.normalize_license_url(
                "https://github.com/o/r/blob/head/LICENSE", ["v1"]
            ),
        )

    def test_falls_back_to_the_default_branch_without_a_tag(self):
        self.assertEqual(
            "https://github.com/o/r/blob/main/LICENSE",
            normalize.normalize_license_url(
                "https://github.com/o/r/blob/HEAD/LICENSE", ["main"]
            ),
        )

    def test_a_url_that_is_already_resolvable_is_left_alone(self):
        url = "https://github.com/o/r/blob/main/LICENSE"
        self.assertEqual(url, normalize.normalize_license_url(url, self.REFS))

    def test_a_pinned_tag_is_left_alone(self):
        url = "https://github.com/o/r/blob/v0.2.19/LICENSE"
        self.assertEqual(url, normalize.normalize_license_url(url, self.REFS))

    def test_a_non_github_url_is_left_alone(self):
        url = "https://example.com/license.html"
        self.assertEqual(url, normalize.normalize_license_url(url, self.REFS))

    def test_a_non_github_blob_url_is_left_alone(self):
        url = "https://gitlab.com/o/r/blob/HEAD/LICENSE"
        self.assertEqual(url, normalize.normalize_license_url(url, self.REFS))

    def test_a_plain_http_github_url_is_left_alone(self):
        url = "http://github.com/o/r/blob/HEAD/LICENSE"
        self.assertEqual(url, normalize.normalize_license_url(url, self.REFS))

    def test_an_empty_url_is_left_alone(self):
        self.assertEqual("", normalize.normalize_license_url("", self.REFS))

    def test_a_candidate_head_is_never_used_as_a_replacement(self):
        # With only ``HEAD`` on offer there is no resolvable ref to pick, so the
        # rewrite must fail loudly rather than rewrite HEAD to HEAD.
        with self.assertRaises(normalize.LicenseUrlError):
            normalize.normalize_license_url(
                "https://github.com/o/r/blob/HEAD/LICENSE", ["HEAD"]
            )

    def test_no_candidate_refs_is_an_error(self):
        with self.assertRaises(normalize.LicenseUrlError):
            normalize.normalize_license_url(
                "https://github.com/o/r/blob/HEAD/LICENSE", []
            )

    def test_the_license_path_is_preserved(self):
        self.assertEqual(
            "https://github.com/o/r/blob/v1/docs/LICENSE.md",
            normalize.normalize_license_url(
                "https://github.com/o/r/blob/HEAD/docs/LICENSE.md", ["v1"]
            ),
        )

    def test_the_rewritten_url_is_stable_under_repeated_normalization(self):
        once = normalize.normalize_license_url(
            "https://github.com/loonghao/msvc-kit/blob/HEAD/LICENSE", self.REFS
        )
        self.assertEqual(once, normalize.normalize_license_url(once, self.REFS))


class RefExistsTest(unittest.TestCase):
    """A silent degradation to the default branch must not go unnoticed."""

    def captured(self, fn):
        import contextlib
        import io

        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            result = fn()
        return result, buffer.getvalue()

    def http_error(self, code):
        def get(url):
            raise urllib.error.HTTPError(url, code, "err", {}, None)

        return get

    class FakeGitHub:
        def __init__(self, get):
            self._get = get

        def get(self, url):
            return self._get(url)

    def test_a_missing_ref_is_reported_without_a_warning(self):
        # A tag that is not pushed yet is the expected case, not a problem.
        result, output = self.captured(
            lambda: normalize.ref_exists(self.FakeGitHub(self.http_error(404)), "o/r", "v1")
        )
        self.assertFalse(result)
        self.assertNotIn("::warning::", output)

    def test_rate_limiting_is_reported(self):
        result, output = self.captured(
            lambda: normalize.ref_exists(self.FakeGitHub(self.http_error(403)), "o/r", "v1")
        )
        self.assertFalse(result)
        self.assertIn("::warning::", output)
        self.assertIn("v1", output)

    def test_an_expired_token_is_reported(self):
        result, output = self.captured(
            lambda: normalize.ref_exists(self.FakeGitHub(self.http_error(401)), "o/r", "v1")
        )
        self.assertFalse(result)
        self.assertIn("::warning::", output)

    def test_a_resolvable_ref_is_accepted(self):
        self.assertTrue(
            normalize.ref_exists(self.FakeGitHub(lambda url: {"sha": "0" * 40}), "o/r", "v1")
        )

    def test_a_transport_failure_is_reported(self):
        def get(url):
            raise urllib.error.URLError("offline")

        result, output = self.captured(
            lambda: normalize.ref_exists(self.FakeGitHub(get), "o/r", "v1")
        )
        self.assertFalse(result)
        self.assertIn("::warning::", output)

    def test_a_ref_requiring_escaping_is_quoted(self):
        seen = {}

        def get(url):
            seen["url"] = url
            return {"sha": "0" * 40}

        self.assertTrue(normalize.ref_exists(self.FakeGitHub(get), "o/r", "v1 beta"))
        self.assertNotIn(" ", seen["url"].rsplit("/commits/", 1)[1])


class LicenseRefsTest(unittest.TestCase):
    REPO = "loonghao/msvc-kit"

    class FakeGitHub:
        """Serves the repository metadata plus the refs that resolve."""

        def __init__(self, branch="main", existing=("v0.2.19",)):
            self.branch = branch
            self.existing = set(existing)

        def get(self, url):
            if url.endswith(f"/repos/{LicenseRefsTest.REPO}"):
                return {"default_branch": self.branch}
            if "/commits/" in url:
                ref = urllib.parse.unquote(url.rsplit("/commits/", 1)[1])
                if ref in self.existing:
                    return {"sha": "0" * 40}
                raise urllib.error.HTTPError(url, 404, "Not Found", {}, None)
            raise AssertionError(f"unexpected url: {url}")

    class Unreachable:
        def get(self, url):
            raise urllib.error.URLError("offline")

    def test_prefers_the_release_tag_over_the_default_branch(self):
        self.assertEqual(
            ["v0.2.19", "main"],
            normalize.license_refs(self.FakeGitHub(), self.REPO, "0.2.19"),
        )

    def test_an_unpublished_tag_is_skipped_and_the_branch_is_used(self):
        refs = normalize.license_refs(
            self.FakeGitHub(existing=()), self.REPO, "0.2.19"
        )
        self.assertEqual(["main"], refs)
        url = normalize.normalize_license_url(
            "https://github.com/loonghao/msvc-kit/blob/HEAD/LICENSE", refs
        )
        self.assertEqual("https://github.com/loonghao/msvc-kit/blob/main/LICENSE", url)

    def test_refs_never_contain_head(self):
        for ref in normalize.license_refs(self.FakeGitHub(), self.REPO, "0.2.19"):
            self.assertNotEqual("HEAD", ref.upper())

    def test_a_missing_default_branch_still_offers_the_verified_tag(self):
        class NoBranch(self.FakeGitHub):
            def get(self, url):
                if url.endswith(f"/repos/{LicenseRefsTest.REPO}"):
                    raise urllib.error.HTTPError(url, 404, "Not Found", {}, None)
                return super().get(url)

        self.assertEqual(
            ["v0.2.19"], normalize.license_refs(NoBranch(), self.REPO, "0.2.19")
        )

    def test_an_unreachable_repository_falls_back_to_the_tag(self):
        # Degraded but not broken: an unverified tag still beats ``HEAD``.
        self.assertEqual(
            ["v0.2.19", "0.2.19"],
            normalize.license_refs(self.Unreachable(), self.REPO, "0.2.19"),
        )

    def test_the_first_ref_is_preferred_by_the_rewrite(self):
        refs = normalize.license_refs(self.FakeGitHub(), self.REPO, "0.2.19")
        url = normalize.normalize_license_url(
            "https://github.com/loonghao/msvc-kit/blob/HEAD/LICENSE", refs
        )
        self.assertIn(url.split("/blob/")[1].split("/")[0], refs)


class MainLocaleReadTest(unittest.TestCase):
    """Drive ``main()`` end to end with a fake GitHub API.

    The unit tests above only cover pure functions, which left the locale read
    path - including its error handling - unverified. These tests run the real
    entry point so a status code the script mishandles fails here.
    """

    IDENTIFIER = "loonghao.msvc-kit"
    VERSION = "0.2.19"
    BRANCH = "loonghao.msvc-kit-0.2.19-ABC"
    INSTALLER_PATH = (
        "manifests/l/loonghao/msvc-kit/0.2.19/loonghao.msvc-kit.installer.yaml"
    )
    LOCALE_PATH = (
        "manifests/l/loonghao/msvc-kit/0.2.19/loonghao.msvc-kit.locale.en-US.yaml"
    )

    INSTALLER_YAML = (
        "# yaml-language-server: $schema=https://aka.ms/winget-manifest\n"
        "PackageIdentifier: loonghao.msvc-kit\n"
        "PackageVersion: 0.2.19\n"
        "Installers:\n"
        "- Architecture: x64\n"
        "  InstallerUrl: https://example.invalid/msvc-kit.exe\n"
        "  InstallerSha256: " + "A" * 64 + "\n"
        "ManifestType: installer\n"
        "ManifestVersion: 1.12.0\n"
    )
    LOCALE_YAML = (
        "# yaml-language-server: $schema=https://aka.ms/winget-manifest\n"
        "PackageIdentifier: loonghao.msvc-kit\n"
        "PackageVersion: 0.2.19\n"
        "License: MIT\n"
        "LicenseUrl: https://github.com/loonghao/msvc-kit/blob/HEAD/LICENSE\n"
        "ManifestType: defaultLocale\n"
        "ManifestVersion: 1.12.0\n"
    )

    def encode(self, text):
        return base64.b64encode(text.encode("utf-8")).decode()

    class FakeGitHub:
        """Serves one PR plus per-path responses keyed by status code."""

        def __init__(self, statuses):
            self.statuses = statuses  # {path: (status, body) or None}
            self.written: list[dict] = []

        def get(self, url):
            if "/search/issues" in url:
                return {"items": [{"number": 42, "title": "New version: x 0.2.19"}]}
            if url.endswith("/pulls/42"):
                return {
                    "number": 42,
                    "head": {
                        "ref": MainLocaleReadTest.BRANCH,
                        "repo": {"full_name": "loonghao/winget-pkgs"},
                    },
                }
            if url.endswith("/repos/loonghao/msvc-kit"):
                return {"default_branch": "main"}
            if "/commits/" in url:
                ref = urllib.parse.unquote(url.rsplit("/commits/", 1)[1])
                if ref == "v0.2.19":
                    return {"sha": "0" * 40}
                raise urllib.error.HTTPError(url, 404, "Not Found", {}, None)
            for path, outcome in self.statuses.items():
                if f"/contents/{path}?" in url:
                    if outcome is None:
                        raise urllib.error.HTTPError(url, 404, "Not Found", {}, None)
                    status, body = outcome
                    if status != 200:
                        raise urllib.error.HTTPError(url, status, "err", {}, None)
                    return {"sha": "blob1", "content": body}
            raise AssertionError(f"unexpected GET: {url}")

        def put(self, url, payload):
            self.written.append(payload)
            return {}

        def file(self, repo, path, ref):
            result = self.get(
                f"https://api.github.com/repos/{repo}/contents/{path}?ref={ref}"
            )
            return result["sha"], base64.b64decode(result["content"]).decode("utf-8")

    def run_main(self, locale_status):
        import contextlib
        import io

        try:
            import yaml
        except ImportError:
            self.skipTest("PyYAML is not installed")

        locale_outcome = (
            None
            if locale_status == 404
            else (locale_status, self.encode(self.LOCALE_YAML))
        )
        fake = self.FakeGitHub(
            {
                self.INSTALLER_PATH: (200, self.encode(self.INSTALLER_YAML)),
                self.LOCALE_PATH: locale_outcome,
            }
        )
        argv = [
            "normalize-winget-manifest.py",
            "--fork",
            "loonghao/winget-pkgs",
            "--identifier",
            self.IDENTIFIER,
            "--version",
            self.VERSION,
        ]
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            with unittest.mock.patch.object(normalize, "GitHub", lambda token: fake):
                with unittest.mock.patch.object(sys, "argv", argv):
                    with unittest.mock.patch.dict(os.environ, {"GH_TOKEN": "t"}):
                        code = normalize.main()
        return code, buffer.getvalue(), fake

    def test_a_missing_locale_manifest_is_skipped(self):
        code, output, fake = self.run_main(404)
        self.assertEqual(0, code)
        self.assertIn("No default locale manifest", output)
        self.assertEqual([], fake.written)

    def test_rate_limited_locale_read_fails_the_run(self):
        # A 403 means the LicenseUrl was never checked; skipping would publish
        # blob/HEAD/LICENSE behind a green release.
        code, output, fake = self.run_main(403)
        self.assertEqual(1, code)
        self.assertIn("::error::", output)
        self.assertNotIn("No default locale manifest", output)
        self.assertEqual([], fake.written)

    def test_an_expired_token_fails_the_run(self):
        code, output, fake = self.run_main(401)
        self.assertEqual(1, code)
        self.assertIn("::error::", output)

    def test_a_server_error_fails_the_run(self):
        code, output, fake = self.run_main(500)
        self.assertEqual(1, code)
        self.assertIn("::error::", output)

    def test_a_readable_locale_manifest_is_rewritten(self):
        code, output, fake = self.run_main(200)
        self.assertEqual(0, code)
        self.assertIn("blob/HEAD/LICENSE", output)
        self.assertIn("blob/v0.2.19/LICENSE", output)
        # Without --apply nothing may be written.
        self.assertEqual([], fake.written)

    def test_apply_pushes_the_rewritten_locale_manifest(self):
        import contextlib
        import io

        fake = self.FakeGitHub(
            {
                self.INSTALLER_PATH: (200, self.encode(self.INSTALLER_YAML)),
                self.LOCALE_PATH: (200, self.encode(self.LOCALE_YAML)),
            }
        )
        argv = [
            "normalize-winget-manifest.py",
            "--fork",
            "loonghao/winget-pkgs",
            "--identifier",
            self.IDENTIFIER,
            "--version",
            self.VERSION,
            "--apply",
        ]
        with contextlib.redirect_stdout(io.StringIO()):
            with unittest.mock.patch.object(normalize, "GitHub", lambda token: fake):
                with unittest.mock.patch.object(sys, "argv", argv):
                    with unittest.mock.patch.dict(os.environ, {"GH_TOKEN": "t"}):
                        code = normalize.main()
        self.assertEqual(0, code)
        self.assertEqual(1, len(fake.written))
        pushed = base64.b64decode(fake.written[0]["content"]).decode()
        self.assertNotIn("blob/HEAD", pushed)
        self.assertIn("blob/v0.2.19/LICENSE", pushed)

    def test_a_missing_locale_manifest_still_normalizes_installers(self):
        # The locale manifest is optional; its absence must not block the
        # installer collapse.
        code, output, _ = self.run_main(404)
        self.assertEqual(0, code)
        self.assertIn("installer", output)


class ForkTest(unittest.TestCase):
    def pr(self, full_name: str | None) -> dict:
        head: dict = {"ref": "loonghao.msvc-kit-0.2.16"}
        if full_name is not None:
            head["repo"] = {"full_name": full_name}
        return {"number": 42, "head": head}

    def test_head_repository_of_a_pull_request(self):
        self.assertEqual(
            "loonghao/winget-pkgs", normalize.head_repo(self.pr("loonghao/winget-pkgs"))
        )

    def test_head_repository_without_a_repo(self):
        self.assertEqual("", normalize.head_repo(self.pr(None)))

    def test_pull_request_without_a_head(self):
        self.assertEqual("", normalize.head_repo({"number": 42}))

    def test_matching_fork_is_accepted(self):
        self.assertIsNone(
            normalize.fork_error("loonghao/winget-pkgs", "loonghao/winget-pkgs")
        )

    def test_fork_comparison_ignores_case(self):
        self.assertIsNone(
            normalize.fork_error("Loonghao/Winget-Pkgs", "loonghao/winget-pkgs")
        )

    def test_a_foreign_head_repository_is_rejected(self):
        error = normalize.fork_error("someone-else/winget-pkgs", "loonghao/winget-pkgs")
        self.assertIsNotNone(error)
        self.assertIn("someone-else/winget-pkgs", error)
        self.assertIn("loonghao/winget-pkgs", error)

    def test_a_missing_head_repository_is_rejected(self):
        self.assertIsNotNone(normalize.fork_error("", "loonghao/winget-pkgs"))


class LocaleManifestTest(unittest.TestCase):
    """End-to-end: the default locale manifest komac generated for 0.2.19."""

    LOCALE_MANIFEST = """\
# Created with WinGet Releaser using komac v2.16.0
# yaml-language-server: $schema=https://aka.ms/winget-manifest.defaultLocale.1.12.0.schema.json

PackageIdentifier: loonghao.msvc-kit
PackageVersion: 0.2.19
PackageLocale: en-US
Publisher: loonghao
PackageName: msvc-kit
License: MIT
LicenseUrl: https://github.com/loonghao/msvc-kit/blob/HEAD/LICENSE
ShortDescription: A portable MSVC Build Tools installer and manager
ManifestType: defaultLocale
ManifestVersion: 1.12.0
"""

    class FakeGitHub:
        def get(self, url):
            if url.endswith("/repos/loonghao/msvc-kit"):
                return {"default_branch": "main"}
            if "/commits/" in url:
                # Only the release tag exists; the plain version is not a tag.
                if urllib.parse.unquote(url.rsplit("/commits/", 1)[1]) == "v0.2.19":
                    return {"sha": "0" * 40}
                raise urllib.error.HTTPError(url, 404, "Not Found", {}, None)
            raise AssertionError(f"unexpected url: {url}")

    def normalized_license_url(self):
        try:
            import yaml
        except ImportError:
            self.skipTest("PyYAML is not installed")

        _, body = normalize.split_header(self.LOCALE_MANIFEST)
        locale = yaml.safe_load(body)
        license_url = str(locale.get("LicenseUrl") or "")
        parts = normalize.split_blob_ref(license_url)
        refs = normalize.license_refs(
            self.FakeGitHub(), parts[0].removeprefix("https://github.com/"), "0.2.19"
        )
        return license_url, normalize.normalize_license_url(license_url, refs)

    def test_the_unresolvable_head_ref_is_replaced(self):
        before, after = self.normalized_license_url()
        self.assertIn("blob/HEAD", before)
        self.assertNotIn("blob/HEAD", after)

    def test_the_result_points_at_a_stable_ref(self):
        _, after = self.normalized_license_url()
        ref = after.split("/blob/")[1].split("/")[0]
        self.assertIn(ref, ["v0.2.19", "0.2.19", "main"])
        self.assertNotEqual("HEAD", ref)

    def test_the_license_path_survives_the_rewrite(self):
        _, after = self.normalized_license_url()
        self.assertTrue(after.endswith("/LICENSE"))


if __name__ == "__main__":
    unittest.main()
