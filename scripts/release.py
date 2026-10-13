#!/usr/bin/env python3
"""Release from an exact, clean tag; keep vx-generated state outside its tree.

Requires Python 3.11+. Rust remains rustup-managed, as declared in vx.toml;
--locked fixes Cargo dependency resolution, not the compiler or binary bytes.
"""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib
import urllib.error
import urllib.request


def git(root, *args):
    return subprocess.check_output(
        ["git", "-C", str(root), *args], text=True, encoding="utf-8"
    ).strip()


def assert_clean(root):
    status = git(root, "status", "--porcelain=v1", "--untracked-files=all")
    if status:
        raise RuntimeError(f"Release source tree is dirty; refusing to hide changes:\n{status}")


def release_version(root, tag):
    assert_clean(root)
    package = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    version = package["version"]
    if tag != f"v{version}":
        raise RuntimeError(f"Tag {tag!r} does not match Cargo.toml version {version!r}")
    if git(root, "rev-parse", "HEAD") != git(root, "rev-parse", f"refs/tags/{tag}^{{commit}}"):
        raise RuntimeError(f"HEAD is not the release tag {tag}")
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    versions = [p["version"] for p in lock["package"]
                if p["name"] == package["name"] and "source" not in p]
    manifest = json.loads((root / ".release-please-manifest.json").read_text(encoding="utf-8"))
    if versions != [version] or manifest.get(".") != version:
        raise RuntimeError("Cargo.toml, Cargo.lock and release-please manifest versions disagree")
    return package["name"], version


def version_exists(name, version):
    # Check this version, not newest_version: an older tag may already exist too.
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}/{version}",
        headers={"User-Agent": "msvc-kit-release-ci"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            data = json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return False
        raise
    if data.get("version", {}).get("num") != version:
        raise RuntimeError("Unexpected crates.io version response; refusing to publish")
    return True


def tool_versions(root):
    lock = tomllib.loads((root / "vx.lock").read_text(encoding="utf-8"))
    return {name: tool["version"] for name, tool in lock["tools"].items()}


def run_cargo(root, tag, command):
    name, version = release_version(root, tag)
    if command == "publish" and version_exists(name, version):
        print(f"{name} {version} already exists on crates.io; skipping publish")
        return
    expected_tools = tool_versions(root)
    # vx 0.9.37 can update vx.lock during setup AND tool execution. Run both
    # outside the checkout, using Cargo's supported --manifest-path to select it.
    # Never reset, ignore or exclude a dirty source file to make Cargo accept it.
    with tempfile.TemporaryDirectory(prefix="msvc-release-tools-", dir=os.environ.get("RUNNER_TEMP")) as tmp:
        tools = Path(tmp).resolve()
        if tools.is_relative_to(root):
            raise RuntimeError("The temporary tools directory must be outside the release tree")
        for filename in ("vx.toml", "vx.lock"):
            shutil.copyfile(root / filename, tools / filename)
        try:
            subprocess.run(["vx", "setup"], cwd=tools, check=True)
        finally:
            assert_clean(root)
        if tool_versions(tools) != expected_tools:
            raise RuntimeError("vx setup changed locked tool versions")
        args = ["vx", "cargo", command, "--locked", "--manifest-path", str(root / "Cargo.toml")]
        if command == "build":
            args.append("--release")
        try:
            subprocess.run(args, cwd=tools, check=True)
        finally:
            assert_clean(root)
        if tool_versions(tools) != expected_tools:
            raise RuntimeError("vx execution changed locked tool versions")


def release_notes(root, tag):
    _, version = release_version(root, tag)
    changelog = (root / "CHANGELOG.md").read_text(encoding="utf-8")
    # release-please is the single notes source, including for manual tag runs.
    sections = re.split(r"(?=^## )", changelog, flags=re.MULTILINE)
    matches = [section for section in sections
               if re.match(r"^## \[" + re.escape(version) + r"\](?:\(|\s)", section)]
    if len(matches) != 1:
        raise RuntimeError(f"Expected one CHANGELOG.md section for {version}")
    notes = matches[0].strip()
    for before, after in re.findall(r"/compare/([^/\s)]+?)\.\.\.([^/\s)]+)", notes):
        if before == after:
            raise RuntimeError("Release notes contain a self-comparison")
    return notes + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("build", "publish", "notes"))
    parser.add_argument("--tag", required=True)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    if args.command == "notes":
        if args.output is None or args.output.resolve().is_relative_to(root):
            parser.error("notes requires --output outside the release source tree")
        args.output.write_text(release_notes(root, args.tag), encoding="utf-8")
    else:
        run_cargo(root, args.tag, args.command)


if __name__ == "__main__":
    main()
