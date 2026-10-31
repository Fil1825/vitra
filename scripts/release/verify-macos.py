#!/usr/bin/env python3
"""Verify the actual app inside each finished DMG, including nested signatures/helpers."""
import argparse
from pathlib import Path
import plistlib
import subprocess


def run(*args):
    subprocess.run(args, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("target")
    parser.add_argument("--signed", action="store_true")
    args = parser.parse_args()
    images = sorted(args.directory.glob("*.dmg"))
    if not images:
        raise RuntimeError("No disk image was produced")
    for image in images:
        run("hdiutil", "verify", str(image))
        output = subprocess.check_output(
            ["hdiutil", "attach", "-readonly", "-nobrowse", "-plist", str(image)],
            # The image displays Vitra's own GPL notice before mounting.
            input=b"Y\n",
        )
        # hdiutil can print the GPL notice before its machine-readable output.
        start = output.find(b"<?xml")
        if start < 0:
            raise RuntimeError("hdiutil did not return the mounted image's plist")
        mounted = plistlib.loads(output[start:])
        volumes = [Path(item["mount-point"]) for item in mounted["system-entities"] if "mount-point" in item]
        try:
            apps = [volume / "Vitra.app" for volume in volumes if (volume / "Vitra.app").is_dir()]
            if len(apps) != 1:
                raise RuntimeError("Expected exactly one Vitra.app inside the DMG")
            app = apps[0]
            run("codesign", "--verify", "--deep", "--strict", "--verbose=2", str(app))
            run("node", "scripts/release/verify-sidecars.mjs", str(app), args.target)
            if args.signed:
                run("xcrun", "stapler", "validate", str(app))
                run("spctl", "--assess", "--type", "execute", "--verbose=2", str(app))
        finally:
            for volume in reversed(volumes):
                run("hdiutil", "detach", str(volume))


if __name__ == "__main__":
    main()
