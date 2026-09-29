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

What this script does
---------------------
It locates the open winget-pkgs pull request for ``<identifier>`` at
``<version>``, reads the installer manifest on the PR branch, collapses
duplicated installer entries down to one per
``(Architecture, InstallerUrl, InstallerSha256)`` keeping the entry whose
package dependency
matches the installer architecture, and pushes the result back to the PR branch
when it changed.

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
VCREDIST_PREFIX = "Microsoft.VCRedist."


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
        print("::error::GH_TOKEN is not set; skipping manifest normalization")
        return 0

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
    path = (
        f"manifests/{partition}/{folder}/{args.version}/"
        f"{args.identifier}.installer.yaml"
    )

    try:
        blob_sha, content = gh.file(target_repo, path, branch)
    except urllib.error.HTTPError as error:
        print(f"::error::Could not read {path} on {target_repo}@{branch}: {error}")
        return 1

    header, body = split_header(content)
    manifest = yaml.safe_load(body)
    installers = manifest.get("Installers") or []
    normalized, changed = collapse(installers)

    error = normalization_error(installers, normalized)
    if error is not None:
        print(f"::error::{error}; leaving {path} untouched")
        return 1

    if not changed:
        print(f"{path} has no duplicate installer entries")
        return 0

    removed = len(installers) - len(normalized)
    print(f"Collapsing {removed} duplicate installer entr{'y' if removed == 1 else 'ies'} in {path}")
    for installer in normalized:
        print(
            f"  keeping {installer.get('Architecture')} "
            f"{installer.get('InstallerSha256', '')[:12]} "
            f"deps={sorted(dependency_architectures(installer)) or '-'}"
        )

    manifest["Installers"] = normalized
    rendered = header + yaml.safe_dump(
        manifest, sort_keys=False, default_flow_style=False, allow_unicode=True
    )

    if not args.apply:
        print("--apply not set; leaving the branch untouched")
        return 0

    gh.put(
        f"{API_ROOT}/repos/{target_repo}/contents/{path}",
        {
            "message": f"New version: {args.identifier} version {args.version}\n\n"
            "Collapse duplicate installer entries so the release ships a single "
            "installer per architecture.",
            "content": base64.b64encode(rendered.encode("utf-8")).decode(),
            "sha": blob_sha,
            "branch": branch,
        },
    )
    print(f"Pushed normalized manifest to {target_repo}@{branch} (PR #{pr['number']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
