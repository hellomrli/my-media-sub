#!/usr/bin/env python3
"""Print the pinned Rust version, failing if the Docker builder drifts."""

import re
from pathlib import Path

root = Path(__file__).resolve().parent.parent
toolchain = re.search(
    r'^channel = "(\d+\.\d+\.\d+)"$',
    (root / "rust-toolchain.toml").read_text(),
    re.MULTILINE,
)
builder = re.search(
    r"^FROM rust:(\d+\.\d+\.\d+)-bookworm AS builder$",
    (root / "Dockerfile").read_text(),
    re.MULTILINE,
)
if not toolchain or not builder or toolchain[1] != builder[1]:
    raise SystemExit("Pin the same Rust version in rust-toolchain.toml and Dockerfile")
print(toolchain[1])
