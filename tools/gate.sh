#!/usr/bin/env bash
# Copyright 2026 PaddleOCR-Rust Contributors
# SPDX-License-Identifier: Apache-2.0
#
# The workspace gate from AGENTS.md, run across every feature configuration
# this crate defines, as one command that fails.
#
# It exists because running the gate and writing a commit in the same shell
# invocation lets the commit be written before the gate's result comes back.
# That happened -- see the change-log entry for `a803634` -- and produced a
# pushed commit whose message claimed a green gate over a red one.
#
# So: run this, read its exit code, and only then commit. It prints the figures
# a commit message should quote, and it exits non-zero if any of them is wrong.
#
#   tools/gate.sh && git commit ...
#
# Every command is offline and locked. Nothing here needs a model, a network, a
# GPU, or the upstream symlink.

set -u

: "${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER:=/usr/bin/gcc}"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER

status=0
note() { printf '%-34s %s\n' "$1" "$2"; }
fail() { status=1; }

# Formatting, first and cheapest.
if cargo fmt --all --check >/dev/null 2>&1; then
    note "fmt" "clean"
else
    note "fmt" "NEEDS FORMATTING"
    fail
fi

# Clippy and tests, once per feature configuration. `--features fuzzing` must be
# word-split, which zsh does not do to an unquoted parameter -- a bug this
# project has already hit once in an ad-hoc loop.
configurations=("" "--all-features" "--features fuzzing")
for configuration in "${configurations[@]}"; do
    label="${configuration:-default}"

    # A fresh touch, so a cached success is not mistaken for a run.
    touch src/lib.rs
    if cargo clippy --offline --locked --workspace ${configuration} \
        --all-targets -- -D warnings >/dev/null 2>&1; then
        note "clippy [$label]" "clean"
    else
        note "clippy [$label]" "FAILED"
        fail
    fi

    output="$(cargo test --offline --locked --workspace ${configuration} 2>&1)"
    passed="$(printf '%s' "$output" | grep -oE '[0-9]+ passed' \
        | awk '{ sum += $1 } END { print sum + 0 }')"
    failed="$(printf '%s' "$output" | grep -cE '^test result: FAILED')"
    if [ "$failed" -eq 0 ] && [ "$passed" -gt 0 ]; then
        note "tests [$label]" "$passed passed"
    else
        note "tests [$label]" "$passed passed, $failed suite(s) FAILED"
        fail
    fi
done

# Cross-compilation checks, which lock in the portability
# `docs/DEPLOY_DEC_001_EVIDENCE.md` measured once so it cannot silently regress.
#
# `check`, not `build`: these targets have no linker here, and a type-check is
# what the evidence actually claimed. Each is **skipped** when its target is not
# installed, so a clean checkout with only the host target still passes -- an
# absent toolchain is not a defect in this crate.
#
# Default features only. `--features onnxruntime` does not compile for wasm32
# and is not expected to: `ort`'s `load-dynamic` has no wasm32 backend, which is
# the recorded blocker rather than a regression.
for target in x86_64-pc-windows-msvc wasm32-unknown-unknown; do
    if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
        note "cross [$target]" "skipped, target not installed"
        continue
    fi
    if cargo check --offline --locked --target "$target" >/dev/null 2>&1; then
        note "cross [$target]" "type-checks"
    else
        note "cross [$target]" "FAILED"
        fail
    fi
done

if [ "$status" -eq 0 ]; then
    echo "gate: PASS"
else
    echo "gate: FAIL -- do not commit figures from this run"
fi
exit "$status"
