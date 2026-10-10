#!/usr/bin/env python3
"""Collapse duplicate installer entries in a winget-pkgs manifest PR branch.

Background
----------
komac builds the new manifest by pairing the *previously published* installer
entries with the assets of the new release (``match_installers`` in
``src/commands/update_version.rs``). When the previous manifest declares one
installer per architecture but the new release ships a single architecture,
every previous entry is paired with the same new asset. The result is several
installer entries sharing one ``Architecture``/``InstallerUrl``/
``InstallerSha256`` and differing only by their inherited package dependency,
which winget rejects with::

    Manifest Error: Duplicate installer entry found.

``installers-regex`` cannot prevent this: it filters the *assets* of the new
release, not the pairing against the previous manifest.

komac also derives ``LicenseUrl`` as ``https://github.com/<owner>/<repo>/blob/
HEAD/<file>`` (``license_url`` in ``src/github/client.rs``). ``HEAD`` is a
symbolic ref, not a real one, so github.com does not resolve it reliably and
winget validation fails the manifest with ``URL-Validation-Error``.

What this script does
---------------------
It locates the open winget-pkgs pull request for ``<identifier>`` at
``<version>``, reads the installer manifest on the PR branch, collapses
duplicated installer entries down to one per
``(Architecture, InstallerUrl, InstallerSha256)`` keeping the entry whose
package dependency
matches the installer architecture, and pushes the result back to the PR branch
when it changed.

It does the same for the ``LicenseUrl`` of the default locale manifest, which
komac writes with the unresolvable ``blob/HEAD/`` prefix. The ref is rewritten
to the release tag when one exists, falling back to the repository default
branch.

It never creates a pull request and never touches any branch other than the one
the pull request already points at: the pull request head must live in
``--fork``, otherwise the script fails instead of writing somewhere else. If no
pull request is found the script exits successfully so it cannot break a
release.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request

API_ROOT = "https://api.github.com"
GITHUB_ROOT = "https://github.com/"
VCREDIST_PREFIX = "Microsoft.VCRedist."
UNRESOLVABLE_REFS = ("HEAD",)


class LicenseUrlError(Exception):
    """Raised when a ``LicenseUrl`` cannot be rewritten to a resolvable ref."""


class GitHub:
    def __init__(self, token: str) -> None:
        self._token = token

    def _request(self, method: str, url: str, payload: dict | None = None):
        data = json.dumps(payload).encode() if payload is not None else None
        request = urllib.request.Request(url, data=data, method=method)
        request.add_header("Authorization", f"Bearer {self._token}")
        request.add_header("Accept", "application/vnd.github+json")
        request.add_header("X-GitHub-Api-Version", "2022-11-28")
        if data is not None:
            request.add_header("Content-Type", "application/json")
        with urllib.request.urlopen(request) as response:
            body = response.read()
        return json.loads(body) if body else None

    def get(self, url: str):
        return self._request("GET", url)

    def put(self, url: str, payload: dict):
        return self._request("PUT", url, payload)

    def file(self, repo: str, path: str, ref: str) -> tuple[str, str]:
        """Return ``(blob_sha, decoded_content)`` for ``path`` at ``ref``."""
        result = self.get(f"{API_ROOT}/repos/{repo}/contents/{path}?ref={ref}")
        return result["sha"], base64.b64decode(result["content"]).decode("utf-8")


def find_pull_request(gh: GitHub, upstream: str, identifier: str, version: str):
    """Find the open PR created for ``identifier`` at ``version``."""
    search = urllib.parse.quote(f"{identifier} in:title type:pr state:open repo:{upstream}")
    for pr in gh.get(f"{API_ROOT}/search/issues?q={search}&per_page=100").get("items", []):
        title = pr.get("title") or ""
        if version in title:
            # The search API does not expose head ref details; fetch the full PR.
            return gh.get(f"{API_ROOT}/repos/{upstream}/pulls/{pr['number']}")
    return None


def split_header(content: str) -> tuple[str, str]:
    """Split leading ``#`` comment lines from the YAML body.

    komac writes a ``# yaml-language-server:`` schema hint at the top of every
    manifest; rewriting the file must keep it.
    """
    lines = content.splitlines(keepends=True)
    header: list[str] = []
    for line in lines:
        if line.startswith("#") or (header and line.strip() == ""):
            header.append(line)
        else:
            break
    prefix = "".join(header)
    return prefix, content[len(prefix) :]


def installers_key(installer: dict) -> tuple:
    return (
        installer.get("Architecture"),
        installer.get("InstallerUrl"),
        installer.get("InstallerSha256"),
    )


def split_blob_ref(url: str) -> tuple[str, str, str] | None:
    """Split a github.com ``blob/<ref>/<path>`` URL into ``(root, ref, path)``.

    Returns ``None`` for anything else - a non-github.com host, a plain http
    URL, or a URL without a ``blob/`` segment - so callers leave URLs they do
    not understand alone instead of mangling them.
    """
    if not url.startswith(GITHUB_ROOT):
        return None
    prefix, separator, rest = url.partition("/blob/")
    if not separator:
        return None
    ref, slash, path = rest.partition("/")
    if not slash or not ref or not path:
        return None
    return prefix, ref, path


def normalize_license_url(url: str, refs: list[str]) -> str:
    """Rewrite ``blob/HEAD/...`` to the first resolvable ref in ``refs``.

    ``HEAD`` is a symbolic ref: github.com does not resolve it as part of a
    ``blob/`` path, so winget's URL validation fails the manifest. The rewrite
    pins the URL to a real ref - the release tag when it exists, otherwise the
    repository default branch.

    URLs that are not GitHub ``blob/`` URLs, or that already name a real ref,
    are returned unchanged: the script must never rewrite what it cannot prove
    is broken.
    """
    if not url:
        return url

    parts = split_blob_ref(url)
    if parts is None:
        return url

    prefix, ref, path = parts
    if ref.upper() not in {candidate.upper() for candidate in UNRESOLVABLE_REFS}:
        return url

    for candidate in refs:
        if candidate and candidate.upper() not in {
            broken.upper() for broken in UNRESOLVABLE_REFS
        }:
            return f"{prefix}/blob/{candidate}/{path}"

    raise LicenseUrlError(
        f"{url} uses an unresolvable ref and no replacement ref is available"
    )


def ref_exists(gh: GitHub, repo: str, ref: str) -> bool:
    """Return whether ``ref`` resolves in ``repo``.

    A tag that has not been pushed yet must not be written into the manifest:
    the resulting URL would 404, which is no better than the ``HEAD`` it
    replaced. Unreadable repositories are reported as missing so the caller
    falls back to a ref it can verify.

    A 404 (the ref genuinely is not there yet) is not worth reporting, but any
    other failure is: silently degrading to the default branch would pin the
    license to the wrong ref with no trace in the log.
    """
    quoted = urllib.parse.quote(ref, safe="")
    try:
        gh.get(f"{API_ROOT}/repos/{repo}/commits/{quoted}")
    except urllib.error.HTTPError as error:
        if error.code != 404:
            print(f"::warning::Could not verify ref {ref} on {repo}: {error}")
        return False
    except urllib.error.URLError as error:
        print(f"::warning::Could not verify ref {ref} on {repo}: {error}")
        return False
    return True


def default_branch(gh: GitHub, repo: str) -> str | None:
    """Return the default branch of ``repo``, or ``None`` if it cannot be read."""
    try:
        return gh.get(f"{API_ROOT}/repos/{repo}").get("default_branch")
    except (urllib.error.HTTPError, urllib.error.URLError) as error:
        print(f"::warning::Could not read the default branch of {repo}: {error}")
        return None


def license_refs(gh: GitHub, repo: str, version: str) -> list[str]:
    """Return the refs a ``blob/HEAD/`` URL should be rewritten to, best first.

    The release tag is preferred because the license of the shipped version is
    what the manifest documents; the default branch is the fallback for a tag
    that has not been pushed yet. Both are verified to resolve before being
    offered - offering a ref that does not exist would trade one 404 for
    another.
    """
    candidate_tags = [f"v{version}", version]
    verified = [tag for tag in candidate_tags if ref_exists(gh, repo, tag)]

    branch = default_branch(gh, repo) or ""
    if branch and branch not in verified:
        verified.append(branch)

    # With nothing verifiable, fall back to the tag anyway: an unverified tag is
    # still a better manifest than an unresolvable ``HEAD``, and release.yml
    # only runs this after the release tag exists.
    return verified or candidate_tags


def head_repo(pr: dict) -> str:
    """Return the full name of the repository hosting the pull request branch."""
    head = pr.get("head") or {}
    repo = head.get("repo") or {}
    return repo.get("full_name") or ""


def fork_error(head: str, fork: str) -> str | None:
    """Return why ``head`` is not a safe place to push, or ``None`` if it is.

    ``--fork`` is the only repository this script is allowed to write to. The
    pull request is located by searching titles in the upstream repository, so
    without this check a pull request opened from anywhere else would be
    normalized (and pushed to) silently.
    """
    if not head:
        return "the pull request has no head repository"
    if head.lower() != fork.lower():
        return f"the pull request head is {head}, not the fork {fork}"
    return None


def dependency_architectures(installer: dict) -> set[str]:
    dependencies = installer.get("Dependencies") or {}
    architectures = set()
    for dependency in dependencies.get("PackageDependencies") or []:
        identifier = str(dependency.get("PackageIdentifier", ""))
        # Identifiers look like ``Microsoft.VCRedist.2015+.x64``; the
        # architecture is the trailing segment.
        if identifier.startswith(VCREDIST_PREFIX):
            architectures.add(identifier.rsplit(".", 1)[-1].lower())
    return architectures


def collapse(installers: list[dict]) -> tuple[list[dict], bool]:
    """Collapse duplicate installers, returning ``(installers, changed)``.

    Entries are grouped by architecture, URL and hash. Within a group the entry
    whose VCRedist dependency matches the installer architecture wins; otherwise
    the first entry wins.
    """
    collapsed: list[dict] = []
    seen: dict[tuple, int] = {}
    changed = False

    for installer in installers:
        key = installers_key(installer)
        if key not in seen:
            seen[key] = len(collapsed)
            collapsed.append(installer)
            continue

        changed = True
        index = seen[key]
        architecture = str(key[0] or "").lower()
        incumbent = collapsed[index]
        if architecture in dependency_architectures(installer) and architecture not in dependency_architectures(incumbent):
            collapsed[index] = installer

    return collapsed, changed


def normalization_error(original: list[dict], normalized: list[dict]) -> str | None:
    """Return why ``normalized`` is not a safe collapse of ``original``.

    Collapsing may drop entries, but it may never invent one, keep the same
    entry twice or reorder the survivors - the pushed manifest must be an
    order-preserving subsequence of the manifest that was read.

    The check compares entries by identity instead of re-deriving the grouping
    key, so it does not repeat the bookkeeping :func:`collapse` used: a bug in
    the grouping itself is reported here instead of being pushed to
    winget-pkgs.
    """
    positions = {}
    for index, installer in enumerate(original):
        positions.setdefault(id(installer), index)

    kept: set[int] = set()
    previous = -1
    for installer in normalized:
        identifier = id(installer)
        if identifier in kept:
            return "an installer entry is kept more than once"
        if identifier not in positions:
            return "an installer entry was not part of the manifest"
        kept.add(identifier)
        position = positions[identifier]
        if position < previous:
            return "the installer entries were reordered"
        previous = position

    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", default="microsoft/winget-pkgs")
    parser.add_argument(
        "--fork",
        required=True,
        help="Fork the PR branch must live in; the script refuses to write anywhere else",
    )
    parser.add_argument("--identifier", required=True, help="PackageIdentifier")
    parser.add_argument("--version", required=True, help="PackageVersion")
    parser.add_argument(
        "--apply",
        action="store_true",
        help="Push the normalized manifest back to the PR branch",
    )
    args = parser.parse_args()

    token = os.environ.get("GH_TOKEN")
    if not token:
        # Exit non-zero: without a token the LicenseUrl is never inspected, and
        # a green run here would publish blob/HEAD/LICENSE just as surely as the
        # failures handled below. The winget job runs after the GitHub Release
        # is published, so failing cannot undo it.
        print("::error::GH_TOKEN is not set; cannot normalize the manifest")
        return 1

    try:
        import yaml
    except ImportError:
        print("::error::PyYAML is required to normalize the manifest")
        return 1

    gh = GitHub(token)
    pr = find_pull_request(gh, args.upstream, args.identifier, args.version)
    if pr is None:
        print(
            f"No open pull request found for {args.identifier} {args.version}; nothing to normalize"
        )
        return 0

    branch = pr["head"]["ref"]
    target_repo = head_repo(pr)
    error = fork_error(target_repo, args.fork)
    if error is not None:
        print(f"::error::Refusing to normalize PR #{pr['number']}: {error}")
        return 1

    partition = args.identifier[0].lower()
    folder = "/".join(args.identifier.split("."))
    directory = f"manifests/{partition}/{folder}/{args.version}"
    manifest_root = f"{directory}/{args.identifier}"

    edits: list[tuple[str, str, str]] = []  # (path, blob_sha, new content)

    installer_path = f"{manifest_root}.installer.yaml"
    try:
        blob_sha, content = gh.file(target_repo, installer_path, branch)
    except urllib.error.HTTPError as error:
        print(
            f"::error::Could not read {installer_path} on {target_repo}@{branch}: {error}"
        )
        return 1

    header, body = split_header(content)
    manifest = yaml.safe_load(body)
    installers = manifest.get("Installers") or []
    normalized, installer_changed = collapse(installers)

    error = normalization_error(installers, normalized)
    if error is not None:
        print(f"::error::{error}; leaving {installer_path} untouched")
        return 1

    if installer_changed:
        removed = len(installers) - len(normalized)
        print(
            f"Collapsing {removed} duplicate installer "
            f"entr{'y' if removed == 1 else 'ies'} in {installer_path}"
        )
        for installer in normalized:
            print(
                f"  keeping {installer.get('Architecture')} "
                f"{installer.get('InstallerSha256', '')[:12]} "
                f"deps={sorted(dependency_architectures(installer)) or '-'}"
            )
        manifest["Installers"] = normalized
        edits.append(
            (
                installer_path,
                blob_sha,
                header
                + yaml.safe_dump(
                    manifest, sort_keys=False, default_flow_style=False, allow_unicode=True
                ),
            )
        )
    else:
        print(f"{installer_path} has no duplicate installer entries")

    locale_path = f"{manifest_root}.locale.en-US.yaml"
    try:
        locale_sha, locale_content = gh.file(target_repo, locale_path, branch)
    except urllib.error.HTTPError as error:
        # Only a missing file means "this package has no default locale
        # manifest". Any other status - an expired token (401) or rate limiting
        # (403) - means the LicenseUrl was never checked, so skipping here would
        # publish ``blob/HEAD/LICENSE`` behind a green release. Fail loudly
        # instead: the winget job is the last step of the release, so a failure
        # here cannot undo the published GitHub Release.
        if error.code != 404:
            print(
                f"::error::Could not read {locale_path} on {target_repo}@{branch}: {error}"
            )
            return 1
        print(f"No default locale manifest at {locale_path}; skipping")
        locale_sha = locale_content = None

    if locale_content is not None:
        locale_header, locale_body = split_header(locale_content)
        locale = yaml.safe_load(locale_body) or {}
        license_url = str(locale.get("LicenseUrl") or "")
        if license_url:
            parts = split_blob_ref(license_url)
            refs = (
                license_refs(gh, parts[0].removeprefix(GITHUB_ROOT), args.version)
                if parts
                else []
            )
            try:
                rewritten = normalize_license_url(license_url, refs)
            except LicenseUrlError as failure:
                print(f"::error::{failure}; leaving {locale_path} untouched")
                return 1

            if rewritten != license_url:
                print(f"Rewriting LicenseUrl in {locale_path}")
                print(f"  before: {license_url}")
                print(f"  after:  {rewritten}")
                locale["LicenseUrl"] = rewritten
                edits.append(
                    (
                        locale_path,
                        locale_sha,
                        locale_header
                        + yaml.safe_dump(
                            locale,
                            sort_keys=False,
                            default_flow_style=False,
                            allow_unicode=True,
                        ),
                    )
                )
            else:
                print(f"{locale_path} LicenseUrl already uses a resolvable ref")

    if not edits:
        print(f"{directory} is already normalized")
        return 0

    if not args.apply:
        print("--apply not set; leaving the branch untouched")
        return 0

    # One Contents API write per file, so N edits become N commits. That is
    # deliberate: the branch is only the source of a winget-pkgs pull request
    # that upstream squash-merges, each edit targets a different file with a
    # blob sha unaffected by the others, and collapsing them into a single
    # commit would mean driving the Git Data API (tree, commit, ref update) -
    # a much larger failure surface on the last step of a release.
    for path, blob_sha, rendered in edits:
        gh.put(
            f"{API_ROOT}/repos/{target_repo}/contents/{path}",
            {
                "message": f"New version: {args.identifier} version {args.version}\n\n"
                "Normalize the generated manifest so the release ships a single "
                "installer per architecture and a LicenseUrl that resolves.",
                "content": base64.b64encode(rendered.encode("utf-8")).decode(),
                "sha": blob_sha,
                "branch": branch,
            },
        )
        print(f"Pushed normalized manifest to {path}")
    print(f"Pushed normalized manifests to {target_repo}@{branch} (PR #{pr['number']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
