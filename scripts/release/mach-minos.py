#!/usr/bin/env python3
"""Read Mach-O LC_BUILD_VERSION / LC_VERSION_MIN from a file or tree of binaries."""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

MH_MAGIC_64 = 0xFEEDFACF
LC_BUILD_VERSION = 0x32
LC_VERSION_MIN_MACOSX = 0x24
FAT_MAGIC = 0xCAFEBABE
FAT_MAGIC_64 = 0xCAFEBABF


def version_triplet(packed: int) -> tuple[int, int, int]:
    return (packed >> 16) & 0xFFFF, (packed >> 8) & 0xFF, packed & 0xFF


def parse_macho_slice(data: bytes) -> tuple[int, int, int] | None:
    if len(data) < 32 or struct.unpack_from("<I", data, 0)[0] != MH_MAGIC_64:
        return None
    ncmds = struct.unpack_from("<I", data, 16)[0]
    off = 32
    for _ in range(ncmds):
        if off + 8 > len(data):
            break
        cmd, cmdsize = struct.unpack_from("<II", data, off)
        if cmd == LC_BUILD_VERSION and off + 24 <= len(data):
            min_v = struct.unpack_from("<I", data, off + 12)[0]
            return version_triplet(min_v)
        if cmd == LC_VERSION_MIN_MACOSX and off + 16 <= len(data):
            raw = struct.unpack_from("<I", data, off + 8)[0]
            return ((raw >> 16) & 0xFFFF, raw & 0xFFFF, 0)
        off += cmdsize
    return None


def parse_fat(data: bytes) -> list[tuple[str, tuple[int, int, int] | None]]:
    if len(data) < 8:
        return []
    magic = struct.unpack_from(">I", data, 0)[0]
    if magic not in (FAT_MAGIC, FAT_MAGIC_64):
        return [("thin", parse_macho_slice(data))]
    is64 = magic == FAT_MAGIC_64
    nfat = struct.unpack_from(">I", data, 4)[0]
    results = []
    off = 8
    for _ in range(nfat):
        if is64:
            if off + 32 > len(data):
                break
            _, offset, _, _, align = struct.unpack_from(">IIIIII", data, off)
            off += 32
        else:
            if off + 20 > len(data):
                break
            _, offset, _, align = struct.unpack_from(">IIII", data, off)
            off += 20
        slice_data = data[offset : offset + 32_000_000]
        cpu = struct.unpack_from("<I", slice_data, 4)[0] if len(slice_data) >= 8 else 0
        cpu_name = {0x01000007: "x86_64", 0x0100000C: "arm64"}.get(cpu, hex(cpu))
        results.append((cpu_name, parse_macho_slice(slice_data)))
    return results


def minos_label(v: tuple[int, int, int]) -> str:
    major, minor, patch = v
    if patch:
        return f"{major}.{minor}.{patch}"
    return f"{major}.{minor}"


def inspect(path: Path) -> list[tuple[Path, str, str | None]]:
    try:
        data = path.read_bytes()
    except OSError as err:
        return [(path, "error", str(err))]
    entries = parse_fat(data)
    out = []
    for arch, version in entries:
        label = f"{arch}" if len(entries) > 1 else "mach-o"
        out.append((path, label, minos_label(version) if version else None))
    return out


def is_probably_macho(path: Path) -> bool:
    if not path.is_file():
        return False
    if path.suffix in {".dylib", ".so"} or path.name in {"mpv", "ffmpeg", "ffprobe", "deno"}:
        return True
    if path.suffix == "" and path.parent.name == "MacOS":
        return True
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


def walk(root: Path) -> list[Path]:
    paths: list[Path] = []
    for path in root.rglob("*"):
        if is_probably_macho(path):
            paths.append(path)
    return sorted(paths)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="+", type=Path)
    parser.add_argument(
        "--max",
        type=str,
        metavar="MAJOR.MINOR",
        help="Fail if any slice requires a newer macOS",
    )
    args = parser.parse_args()
    max_major = max_minor = None
    if args.max:
        parts = args.max.split(".")
        max_major, max_minor = int(parts[0]), int(parts[1] if len(parts) > 1 else 0)

    failures: list[str] = []
    for root in args.paths:
        targets = [root] if root.is_file() else walk(root)
        for target in targets:
            for path, arch, version in inspect(target):
                rel = path.relative_to(root) if root.is_dir() else path.name
                if version is None:
                    print(f"SKIP {rel} ({arch}): no min OS", file=sys.stderr)
                    continue
                print(f"{rel}\t{arch}\tmacOS>={version}")
                if max_major is not None:
                    maj_s, min_s, *_ = version.split(".")
                    maj, minor = int(maj_s), int(min_s)
                    if maj > max_major or (maj == max_major and minor > max_minor):
                        failures.append(f"{rel} ({arch}) requires macOS {version}, max allowed {args.max}")
    if failures:
        for line in failures:
            print(f"FAIL: {line}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
