#!/usr/bin/env python3
"""Generate the software bill of materials from Cargo.lock.

Roadmap item: `SUPPLY-001`.

The SBOM is generated from the lockfile rather than written by hand, because a
hand-written one drifts the moment a dependency changes and a stale SBOM is worse
than none: it asserts a composition that is no longer true.

Licences are read from the vendored crate sources in the local Cargo registry,
not from a curated list, for the same reason. If a crate is not vendored the
licence is emitted as `NOASSERTION` rather than guessed, which is what the
SPDX and CycloneDX conventions mean by that value.

The `checksum` in Cargo.lock is the SHA-256 of the published `.crate` file, so it
travels into the SBOM as the component hash. That is the provenance link: an
auditor can verify a downloaded crate against the SBOM without trusting this
project.

Only the Python standard library is used. Nothing is downloaded.

Usage:
    python3 tools/generate_sbom.py <Cargo.lock> <output.cdx.json>
"""

from __future__ import annotations

import glob
import json
import re
import sys
from pathlib import Path

SBOM_FORMAT = "CycloneDX"
SPEC_VERSION = "1.5"

# This project's own package, which is a component of nothing and the subject of
# everything.
ROOT_NAME = "paddleocr-rust"


def parse_lockfile(path: Path) -> list[dict]:
    """Extracts name, version, source, and checksum for every locked package."""
    packages = []
    current: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line == "[[package]]":
            if current:
                packages.append(current)
            current = {}
            continue
        match = re.match(r'^(name|version|source|checksum) = "(.*)"$', line)
        if match and current is not None:
            current[match.group(1)] = match.group(2)
    if current:
        packages.append(current)
    return [package for package in packages if "name" in package and "version" in package]


def registry_license(name: str, version: str) -> str:
    """Reads a crate's declared licence from its vendored source."""
    for root in glob.glob(str(Path.home() / ".cargo/registry/src/*")):
        manifest = Path(root) / f"{name}-{version}" / "Cargo.toml"
        if not manifest.is_file():
            continue
        for line in manifest.read_text(encoding="utf-8").splitlines():
            match = re.match(r'^license\s*=\s*"(.*)"$', line.strip())
            if match:
                return match.group(1)
        # Vendored but with no `license` key: a licence-file-only crate. Saying
        # so is more accurate than inferring one from the file's contents.
        return "NOASSERTION"
    return "NOASSERTION"


def component(package: dict) -> dict:
    name = package["name"]
    version = package["version"]
    entry: dict = {
        "type": "library",
        "name": name,
        "version": version,
        "purl": f"pkg:cargo/{name}@{version}",
        "licenses": [{"expression": registry_license(name, version)}],
    }
    if "checksum" in package:
        # Cargo records the SHA-256 of the published .crate archive.
        entry["hashes"] = [{"alg": "SHA-256", "content": package["checksum"]}]
    if "source" in package:
        entry["externalReferences"] = [
            {"type": "distribution", "url": package["source"]}
        ]
    return entry


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    lockfile = Path(sys.argv[1])
    output = Path(sys.argv[2])

    packages = parse_lockfile(lockfile)
    components = [
        component(package) for package in packages if package["name"] != ROOT_NAME
    ]
    components.sort(key=lambda entry: (entry["name"], entry["version"]))

    root = next(
        (package for package in packages if package["name"] == ROOT_NAME), None
    )
    document = {
        "bomFormat": SBOM_FORMAT,
        "specVersion": SPEC_VERSION,
        "version": 1,
        "metadata": {
            # No timestamp: it is the only field that would change on every
            # regeneration, which would make the committed file impossible to
            # diff usefully and impossible to check for drift.
            "component": {
                "type": "application",
                "name": ROOT_NAME,
                "version": root["version"] if root else "0.0.0",
                "purl": f"pkg:cargo/{ROOT_NAME}@{root['version'] if root else '0.0.0'}",
                "licenses": [{"expression": "Apache-2.0"}],
            },
            "tools": [{"name": "tools/generate_sbom.py"}],
        },
        "components": components,
    }
    output.write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    unresolved = [
        entry["name"]
        for entry in components
        if entry["licenses"][0]["expression"] == "NOASSERTION"
    ]
    print(f"wrote {output}: {len(components)} components")
    if unresolved:
        print(f"licences unresolved for: {', '.join(unresolved)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
