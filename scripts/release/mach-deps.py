#!/usr/bin/env python3
"""Offline Mach-O dylib / rpath audit for bundled macOS sidecars (Linux-friendly)."""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

MH_MAGIC_64 = 0xFEEDFACF
FAT_MAGIC = 0xCAFEBABE
FAT_MAGIC_64 = 0xCAFEBABF
LC_LOAD_DYLIB = 0x0C
LC_LOAD_WEAK_DYLIB = 0x80000018
LC_REEXPORT_DYLIB = 0x1F | 0x80000000
LC_RPATH = 0x1C | 0x80000000

SYSTEM_PREFIXES = (
    "/System/Library/",
    "/usr/lib/",
    "/lib/",
)

FORBIDDEN_NEEDLES = (
    "/opt/homebrew",
    "/usr/local/opt/",
    "/usr/local/lib/",
    "/usr/local/Cellar",
    "/homebrew/",
    "/Desktop/homebrew",
)


def read_cstring(data: bytes, start: int) -> str:
    end = data.find(b"\0", start)
    if end < 0:
        end = min(start + 4096, len(data))
    return data[start:end].decode("utf-8", errors="replace")


def thin_slice(data: bytes) -> bytes:
    magic = struct.unpack_from("<I", data, 0)[0] if len(data) >= 4 else 0
    if magic == MH_MAGIC_64:
        return data
    if magic in (FAT_MAGIC, FAT_MAGIC_64):
        is64 = magic == FAT_MAGIC_64
        nfat = struct.unpack_from(">I", data, 4)[0]
        off = 8
        for _ in range(nfat):
            if is64:
                _, offset, _, _, _ = struct.unpack_from(">IIIIII", data, off)
                off += 32
            else:
                _, offset, _, _ = struct.unpack_from(">IIII", data, off)
                off += 20
            return data[offset:]
    return data


def parse_dylibs_and_rpaths(data: bytes) -> tuple[list[str], list[str]]:
    data = thin_slice(data)
    if len(data) < 32 or struct.unpack_from("<I", data, 0)[0] != MH_MAGIC_64:
        return [], []
    ncmds = struct.unpack_from("<I", data, 16)[0]
    off = 32
    loads: list[str] = []
    rpaths: list[str] = []
    for _ in range(ncmds):
        if off + 8 > len(data):
            break
        cmd, cmdsize = struct.unpack_from("<II", data, off)
        if cmd in (LC_LOAD_DYLIB, LC_LOAD_WEAK_DYLIB, LC_REEXPORT_DYLIB) and off + 24 <= len(data):
            name_off = struct.unpack_from("<I", data, off + 8)[0]
            loads.append(read_cstring(data, off + name_off))
        elif cmd == LC_RPATH and off + 12 <= len(data):
            path_off = struct.unpack_from("<I", data, off + 8)[0]
            rpaths.append(read_cstring(data, off + path_off))
        off += cmdsize
    return loads, rpaths


def is_system_dylib(name: str) -> bool:
    return name.startswith(SYSTEM_PREFIXES)


def mach_executable_dir(binary: Path) -> Path:
    """@executable_path is the directory containing the main Mach-O (MacOS/), not lib/."""
    if binary.parent.name == "lib":
        return binary.parent.parent
    return binary.parent


def resolve_dep(name: str, binary: Path) -> Path | None:
    if is_system_dylib(name):
        return None
    base = mach_executable_dir(binary)
    if name.startswith("@loader_path/"):
        return (base / name.removeprefix("@loader_path/")).resolve()
    if name.startswith("@executable_path/"):
        return (base / name.removeprefix("@executable_path/")).resolve()
    if name.startswith("@rpath/"):
        tail = name.removeprefix("@rpath/")
        for root in (base, base / "lib", base.parent / "lib", binary.parent.parent / "MacOS/lib"):
            candidate = (root / tail).resolve()
            if candidate.is_file():
                return candidate
        return (base / "lib" / tail).resolve()
    if name.startswith("/"):
        return Path(name)
    return (base / name).resolve()


def audit_macho(binary: Path, bundle_root: Path) -> list[str]:
    problems: list[str] = []
    try:
        data = binary.read_bytes()
    except OSError as err:
        return [f"{binary}: {err}"]
    loads, rpaths = parse_dylibs_and_rpaths(data)
    rel = binary.relative_to(bundle_root) if bundle_root in binary.parents else binary.name
    for needle in FORBIDDEN_NEEDLES:
        for path in loads:
            if needle in path:
                problems.append(f"{rel}: forbidden load {path!r}")
    for path in loads:
        if is_system_dylib(path):
            continue
        resolved = resolve_dep(path, binary)
        if resolved is None:
            continue
        if not resolved.is_file():
            try:
                resolved.relative_to(bundle_root)
            except ValueError:
                problems.append(f"{rel}: missing bundle dep {path!r} (resolved {resolved})")
            else:
                problems.append(f"{rel}: missing dep {path!r} -> {resolved}")
        else:
            try:
                resolved.relative_to(bundle_root)
            except ValueError:
                problems.append(f"{rel}: dep outside bundle {path!r} -> {resolved}")

    return problems


def stale_rpaths(binary: Path, rpaths: list[str]) -> list[str]:
    rel = binary.name
    stale: list[str] = []
    for rpath in rpaths:
        if rpath.startswith("@executable_path"):
            continue
        if rpath.startswith(SYSTEM_PREFIXES) or rpath == "/usr/lib/swift":
            continue
        if any(needle in rpath for needle in FORBIDDEN_NEEDLES) or (
            rpath.startswith("/") and not rpath.startswith(SYSTEM_PREFIXES)
        ):
            stale.append(f"{rel}: stale rpath {rpath!r}")
    return stale


def is_macho(path: Path) -> bool:
    try:
        head = path.read_bytes()[:4]
    except OSError:
        return False
    return head in {
        b"\xcf\xfa\xed\xfe",
        b"\xca\xfe\xba\xbe",
        b"\xca\xfe\xba\xbf",
        b"\xfe\xed\xfa\xce",
        b"\xfe\xed\xfa\xcf",
    }


def walk_macho(root: Path) -> list[Path]:
    out: list[Path] = []
    for path in root.rglob("*"):
        if path.is_file() and (path.suffix == ".dylib" or path.name in {"mpv", "ffmpeg", "ffprobe", "deno"} or is_macho(path)):
            out.append(path)
    return sorted(out)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, help="Sidecar root (e.g. mpv/mpv.app)")
    parser.add_argument(
        "--entry",
        type=Path,
        default=None,
        help="Main binary to list (default: .../Contents/MacOS/mpv)",
    )
    parser.add_argument(
        "--warn-stale-rpath",
        action="store_true",
        help="Print stale builder rpaths (do not fail); strip before notarization",
    )
    args = parser.parse_args()
    root = args.root.resolve()
    if not root.is_dir():
        print(f"not a directory: {root}", file=sys.stderr)
        return 2
    entry = args.entry
    if entry is None:
        cand = root / "Contents/MacOS/mpv"
        entry = cand if cand.is_file() else root
    loads, rpaths = parse_dylibs_and_rpaths(entry.read_bytes())
    try:
        entry_rel = entry.relative_to(root)
    except ValueError:
        entry_rel = entry.name
    print(f"ENTRY {entry_rel}")
    for name in loads:
        print(f"LOAD\t{name}")
    for name in rpaths:
        print(f"RPATH\t{name}")
    if args.warn_stale_rpath:
        for line in stale_rpaths(entry, rpaths):
            print(f"WARN: {line}", file=sys.stderr)
    failures: list[str] = []
    for path in walk_macho(root):
        failures.extend(audit_macho(path, root))
    if failures:
        for line in failures:
            print(f"FAIL: {line}", file=sys.stderr)
        return 1
    print(f"OK: {len(walk_macho(root))} Mach-O files, bundle load closure resolved")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
