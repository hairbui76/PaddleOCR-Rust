#!/usr/bin/env python3
"""Generate the byte-identical self-authored input for the no-text oracle."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
from pathlib import Path


SOURCE_FIXTURE_ID = "classic-v1-image-input-png-rgb-3x2"
SOURCE_DIGEST = "c422c83b3b20d3b206d47643e3f5e6aa3d87ece61e6433ddd5be5bda8906bccd"
SOURCE_LENGTH = 81


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Write the byte-identical self-authored PNG used by the "
            "classic-v1-e2e-no-text fixture. Refuses to overwrite a file."
        )
    )
    parser.add_argument(
        "--output",
        required=True,
        type=Path,
        help="new output file to create exclusively",
    )
    return parser.parse_args()


def require_regular_fixture_payload(repository_root: Path) -> bytes:
    capture_path = (
        repository_root
        / "tests"
        / "fixtures"
        / "classic-v1-image-inputs"
        / "capture.json"
    )
    try:
        capture = json.loads(capture_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"cannot read source fixture capture: {error}") from error

    cases = capture.get("cases")
    if not isinstance(cases, list):
        raise SystemExit("source fixture capture has no cases list")
    matches = [
        case
        for case in cases
        if isinstance(case, dict) and case.get("fixture_id") == SOURCE_FIXTURE_ID
    ]
    if len(matches) != 1:
        raise SystemExit("source fixture capture does not contain exactly one expected case")

    payload = matches[0].get("encoded_image")
    if not isinstance(payload, dict) or not isinstance(payload.get("base64"), str):
        raise SystemExit("source fixture capture has no encoded PNG payload")
    try:
        contents = base64.b64decode(payload["base64"], validate=True)
    except (ValueError, TypeError) as error:
        raise SystemExit(f"source fixture PNG is not valid base64: {error}") from error

    digest = hashlib.sha256(contents).hexdigest()
    if (
        payload.get("sha256") != SOURCE_DIGEST
        or payload.get("byte_length") != SOURCE_LENGTH
        or digest != SOURCE_DIGEST
        or len(contents) != SOURCE_LENGTH
    ):
        raise SystemExit("source fixture PNG identity does not match the reviewed record")
    return contents


def reject_upstream_target(repository_root: Path, output_path: Path) -> None:
    upstream_target = (repository_root / "PaddleOCR").resolve()
    try:
        output_path.relative_to(upstream_target)
    except ValueError:
        return
    raise SystemExit("refusing to write inside the read-only PaddleOCR upstream checkout")


def main() -> None:
    arguments = parse_arguments()
    repository_root = Path(__file__).resolve().parent.parent
    contents = require_regular_fixture_payload(repository_root)

    output_path = arguments.output
    if not output_path.is_absolute():
        output_path = Path.cwd() / output_path
    output_path = output_path.resolve(strict=False)
    reject_upstream_target(repository_root, output_path)

    if not output_path.parent.is_dir():
        raise SystemExit(f"output directory does not exist: {output_path.parent}")
    if output_path.exists():
        raise SystemExit(f"refusing to overwrite existing output: {output_path}")

    try:
        with output_path.open("xb") as output:
            output.write(contents)
    except OSError as error:
        raise SystemExit(f"cannot create output: {error}") from error

    record = {
        "bytes": len(contents),
        "output": str(output_path),
        "sha256": hashlib.sha256(contents).hexdigest(),
        "source_fixture_id": SOURCE_FIXTURE_ID,
    }
    print(json.dumps(record, separators=(",", ":"), sort_keys=True))


if __name__ == "__main__":
    main()
