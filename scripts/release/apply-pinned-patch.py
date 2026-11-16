#!/usr/bin/env python3
"""Apply the checksummed unified diffs named by a sidecar pin.

Usage: apply-pinned-patch.py <pins.json> <dependency> <source-tree> <patch-dir>

The patch file is the source of truth. A checksum mismatch or a hunk that
does not match the tree exits 1. The `patch` program is not required.
"""

import hashlib
import json
import pathlib
import sys


def main() -> None:
    pins_path, dep_name, tree, patch_dir = sys.argv[1:5]
    pins = json.loads(pathlib.Path(pins_path).read_text())
    if not isinstance(pins, dict) or "dependencies" not in pins:
        raise SystemExit(f"{pins_path} has no dependencies")
    deps = pins["dependencies"]
    dep = next((item for item in deps if item["name"] == dep_name), None)
    if dep is None:
        raise SystemExit(f"{pins_path} has no {dep_name} dependency")
    root = pathlib.Path(tree)
    directory = pathlib.Path(patch_dir)
    for spec in dep.get("patches", []):
        patch_path = directory / spec["file"]
        data = patch_path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if digest != spec["sha256"]:
            raise SystemExit(
                f"checksum mismatch for {spec['file']}: expected {spec['sha256']}, got {digest}"
            )
        apply_diff(data.decode("utf-8"), root)


def apply_diff(diff: str, root: pathlib.Path) -> None:
    for path, hunks in parse_diff(diff):
        target = root / path
        original = target.read_text()
        lines = original.splitlines()
        for old, new in hunks:
            found = [
                index
                for index in range(len(lines) - len(old) + 1)
                if lines[index : index + len(old)] == old
            ]
            if len(found) != 1:
                raise SystemExit(f"{path}: patch hunk matched {len(found)} places")
            index = found[0]
            lines = lines[:index] + new + lines[index + len(old) :]
        text = "\n".join(lines)
        if original.endswith("\n") or text:
            text += "\n"
        target.write_text(text)


def parse_diff(diff: str):
    lines = diff.splitlines()
    files = []
    current = None
    index = 0
    while index < len(lines):
        line = lines[index]
        if line.startswith("--- "):
            old = line[4:].split("\t", 1)[0].strip()
            if old.startswith("a/"):
                old = old[2:]
            if old == "/dev/null":
                raise SystemExit("patch creates a file, which this applier does not support")
            current = (old, [])
            files.append(current)
            index += 2
            continue
        if line.startswith("@@"):
            if current is None:
                raise SystemExit("patch hunk has no file header")
            index += 1
            old_lines = []
            new_lines = []
            while index < len(lines) and not lines[index].startswith(
                ("@@", "diff ", "--- ")
            ):
                body = lines[index]
                index += 1
                if body.startswith("\\"):
                    continue
                if not body:
                    raise SystemExit("patch line is missing its prefix")
                tag, text = body[0], body[1:]
                if tag == " ":
                    old_lines.append(text)
                    new_lines.append(text)
                elif tag == "-":
                    old_lines.append(text)
                elif tag == "+":
                    new_lines.append(text)
                else:
                    raise SystemExit(f"bad patch line: {body!r}")
            current[1].append((old_lines, new_lines))
            continue
        index += 1
    if not files:
        raise SystemExit("patch has no file hunk")
    return files


if __name__ == "__main__":
    main()
