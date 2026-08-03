#!/usr/bin/env python3
# Copyright 2026 PaddleOCR-Rust Contributors
# SPDX-License-Identifier: Apache-2.0
"""Audit aggregate Unicode structure of one pinned recognizer YAML file.

This developer-only tool accepts only the exact PP-OCRv6 medium recognizer
``inference.yml`` recorded by this repository. It refuses symlinks,
non-regular files, empty files, oversized files, and a mismatched YAML digest
before parsing a deliberately narrow ``character_dict`` syntax.

The tool never downloads, modifies, or executes a model. It emits one
aggregate-only JSON record on stdout: no dictionary entry, decoded text, tensor
value, model output, or input path appears in that record. It is not a model
resolver, runtime, artifact approval, or license decision.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import sys
import unicodedata
from collections import Counter
from pathlib import Path
from typing import Sequence


SCHEMA_VERSION = "paddleocr-rust/dictionary-unicode-audit/v2"
EXPECTED_CONFIG_SHA256 = "991b700facf5b50a7de193468207d5f4255b538dde0d312ae3b7c7a9b6873129"
EXPECTED_ENTRY_STREAM_SHA256 = (
    "b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d"
)
EXPECTED_ENTRY_COUNT = 18_708
DEFAULT_MAX_CONFIG_BYTES = 512 * 1024


def positive_integer(value: str) -> int:
    """Parse one strictly positive command-line integer."""

    try:
        parsed = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("must be an integer") from error
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return parsed


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse options without reading the supplied configuration file."""

    parser = argparse.ArgumentParser(
        description=(
            "Audit aggregate Unicode structure of the pinned local PP-OCRv6 "
            "medium recognizer YAML."
        )
    )
    parser.add_argument(
        "--max-config-bytes",
        type=positive_integer,
        default=DEFAULT_MAX_CONFIG_BYTES,
        help=(
            "Maximum accepted configuration size before hashing "
            f"(default: {DEFAULT_MAX_CONFIG_BYTES})."
        ),
    )
    parser.add_argument(
        "configuration",
        type=Path,
        help="Explicit local path to the recorded recognizer inference.yml file.",
    )
    return parser.parse_args(arguments)


def require_regular_bounded_file(path: Path, maximum_bytes: int) -> os.stat_result:
    """Return lstat metadata after rejecting unsafe input-file forms."""

    try:
        metadata = path.lstat()
    except OSError as error:
        raise ValueError("cannot inspect configuration path") from error

    if stat.S_ISLNK(metadata.st_mode):
        raise ValueError("refusing a symlinked configuration path")
    if not stat.S_ISREG(metadata.st_mode):
        raise ValueError("configuration path is not a regular file")
    if metadata.st_size <= 0:
        raise ValueError("configuration file is empty")
    if metadata.st_size > maximum_bytes:
        raise ValueError(
            f"configuration file exceeds {maximum_bytes} byte limit: {metadata.st_size}"
        )
    return metadata


def read_regular_bounded_file(path: Path, maximum_bytes: int) -> bytes:
    """Read one bounded regular file without following a path-replacement symlink."""

    checked_metadata = require_regular_bounded_file(path, maximum_bytes)
    no_follow = getattr(os, "O_NOFOLLOW", None)
    if no_follow is None:
        raise ValueError("platform does not provide no-follow file opening")
    flags = os.O_RDONLY | no_follow | getattr(os, "O_CLOEXEC", 0)
    try:
        descriptor = os.open(path, flags)
        with os.fdopen(descriptor, "rb", closefd=True) as source:
            opened_metadata = os.fstat(source.fileno())
            if not stat.S_ISREG(opened_metadata.st_mode):
                raise ValueError("opened configuration is not a regular file")
            if (
                checked_metadata.st_dev != opened_metadata.st_dev
                or checked_metadata.st_ino != opened_metadata.st_ino
            ):
                raise ValueError("configuration path changed while opening")
            if (
                opened_metadata.st_size <= 0
                or opened_metadata.st_size > maximum_bytes
            ):
                raise ValueError("configuration size changed while opening")
            raw = source.read(opened_metadata.st_size + 1)
    except OSError as error:
        raise ValueError("cannot read configuration file") from error

    if len(raw) != opened_metadata.st_size:
        raise ValueError("configuration size changed while reading")
    return raw


def parse_character_dict(configuration_text: str) -> list[str]:
    """Parse only the known final ``PostProcess.character_dict`` list syntax."""

    entries: list[str] = []
    inside_character_dict = False
    found_character_dict = False
    for line_number, line in enumerate(
        configuration_text.splitlines(keepends=True), start=1
    ):
        if line == "  character_dict:\n":
            if found_character_dict:
                raise ValueError("duplicate character_dict declaration")
            found_character_dict = True
            inside_character_dict = True
            continue
        if not inside_character_dict:
            continue
        if not line.startswith("  - "):
            raise ValueError(
                f"unexpected line {line_number} within character_dict list"
            )

        scalar = line[4:].rstrip("\r\n")
        if not scalar:
            raise ValueError(f"empty character dictionary scalar at line {line_number}")
        if scalar.startswith("'"):
            if len(scalar) < 2 or not scalar.endswith("'"):
                raise ValueError(
                    f"malformed single-quoted character dictionary scalar at line {line_number}"
                )
            scalar = scalar[1:-1].replace("''", "'")
        elif "'" in scalar:
            raise ValueError(
                f"unexpected quote in character dictionary scalar at line {line_number}"
            )
        entries.append(scalar)

    if not found_character_dict or not entries:
        raise ValueError("missing or empty character_dict list")
    return entries


def transform_collision_counts(entries: list[str], transform: str) -> tuple[int, int]:
    """Return collision-group and collision-entry counts after one transform."""

    transformed_counts: Counter[str] = Counter()
    for entry in entries:
        transformed = (
            entry.casefold()
            if transform == "casefold"
            else unicodedata.normalize(transform, entry)
        )
        transformed_counts[transformed] += 1
    collision_counts = [
        count for count in transformed_counts.values() if count > 1
    ]
    return len(collision_counts), sum(collision_counts)


def is_noncharacter(codepoint: int) -> bool:
    """Return whether one scalar is permanently reserved as a Unicode noncharacter."""

    return 0xFDD0 <= codepoint <= 0xFDEF or (codepoint & 0xFFFF) in (0xFFFE, 0xFFFF)


def audit_configuration(path: Path, maximum_bytes: int) -> dict[str, object]:
    """Verify the pinned input then return only aggregate Unicode metadata."""

    raw = read_regular_bounded_file(path, maximum_bytes)
    input_digest = hashlib.sha256(raw).hexdigest()
    if input_digest != EXPECTED_CONFIG_SHA256:
        raise ValueError("unexpected recognizer YAML SHA-256")
    try:
        entries = parse_character_dict(raw.decode("utf-8"))
    except UnicodeDecodeError as error:
        raise ValueError("recognizer YAML is not valid UTF-8") from error

    entry_stream = b"".join(entry.encode("utf-8") + b"\n" for entry in entries)
    stream_digest = hashlib.sha256(entry_stream).hexdigest()
    if len(entries) != EXPECTED_ENTRY_COUNT:
        raise ValueError("unexpected character dictionary entry count")
    if stream_digest != EXPECTED_ENTRY_STREAM_SHA256:
        raise ValueError("unexpected ordered character dictionary digest")
    if len(set(entries)) != len(entries):
        raise ValueError("character dictionary contains duplicate entries")

    category_counts: Counter[str] = Counter()
    entry_codepoint_lengths: Counter[int] = Counter()
    entry_flags: Counter[str] = Counter()
    whitespace_codepoints: set[str] = set()
    non_printable_codepoints: set[str] = set()
    total_codepoints = 0

    for entry in entries:
        entry_codepoint_lengths[len(entry)] += 1
        total_codepoints += len(entry)
        saw_whitespace = False
        saw_combining_mark = False
        saw_control = False
        saw_format = False
        saw_private_use = False
        saw_unassigned = False
        saw_noncharacter = False
        saw_non_printable = False
        for scalar in entry:
            category = unicodedata.category(scalar)
            category_counts[category] += 1
            saw_whitespace |= scalar.isspace()
            saw_combining_mark |= unicodedata.combining(scalar) != 0
            saw_control |= category == "Cc"
            saw_format |= category == "Cf"
            saw_private_use |= category == "Co"
            saw_unassigned |= category == "Cn"
            saw_noncharacter |= is_noncharacter(ord(scalar))
            saw_non_printable |= not scalar.isprintable()
            if scalar.isspace():
                whitespace_codepoints.add(f"U+{ord(scalar):04X}")
            if not scalar.isprintable():
                non_printable_codepoints.add(f"U+{ord(scalar):04X}")
        entry_flags["contains_whitespace"] += saw_whitespace
        entry_flags["contains_combining_mark"] += saw_combining_mark
        entry_flags["contains_control"] += saw_control
        entry_flags["contains_format"] += saw_format
        entry_flags["contains_private_use"] += saw_private_use
        entry_flags["contains_unassigned"] += saw_unassigned
        entry_flags["contains_noncharacter"] += saw_noncharacter
        entry_flags["contains_non_printable"] += saw_non_printable
        if unicodedata.normalize("NFC", entry) != entry:
            entry_flags["changes_under_nfc"] += 1
        if unicodedata.normalize("NFKC", entry) != entry:
            entry_flags["changes_under_nfkc"] += 1

    nfc_groups, nfc_entries = transform_collision_counts(entries, "NFC")
    nfkc_groups, nfkc_entries = transform_collision_counts(entries, "NFKC")
    casefold_groups, casefold_entries = transform_collision_counts(entries, "casefold")
    return {
        "schema_version": SCHEMA_VERSION,
        "unicode_data_version": unicodedata.unidata_version,
        "input_bytes": len(raw),
        "input_sha256": input_digest,
        "ordered_entry_stream_sha256": stream_digest,
        "entry_count": len(entries),
        "total_codepoints": total_codepoints,
        "entry_codepoint_lengths": {
            str(length): entry_codepoint_lengths[length]
            for length in sorted(entry_codepoint_lengths)
        },
        "unicode_general_categories": {
            category: category_counts[category]
            for category in sorted(category_counts)
        },
        "entry_flags": {
            name: entry_flags[name]
            for name in (
                "contains_whitespace",
                "contains_combining_mark",
                "contains_control",
                "contains_format",
                "contains_private_use",
                "contains_unassigned",
                "contains_noncharacter",
                "contains_non_printable",
                "changes_under_nfc",
                "changes_under_nfkc",
            )
        },
        "exception_codepoints": {
            "whitespace": sorted(whitespace_codepoints),
            "non_printable": sorted(non_printable_codepoints),
        },
        "collision_groups": {
            "nfc": nfc_groups,
            "nfc_entries": nfc_entries,
            "nfkc": nfkc_groups,
            "nfkc_entries": nfkc_entries,
            "casefold": casefold_groups,
            "casefold_entries": casefold_entries,
        },
    }


def main(arguments: Sequence[str] | None = None) -> int:
    """Run the bounded aggregate-only inspection."""

    parsed = parse_arguments(arguments)
    try:
        result = audit_configuration(parsed.configuration, parsed.max_config_bytes)
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
