# Linux aarch64 installer

Emits three artifacts per build:
- `dist/neuro-link_$VERSION_arm64.deb` (fpm `-t deb -a arm64`)
- `dist/neuro-link-$VERSION-1.aarch64.rpm` (fpm `-t rpm -a aarch64`)
- `dist/neuro-link-$VERSION-aarch64.AppImage`

## Flow

`pkg/linux-aarch64/build.sh <VERSION>` — identical to `linux-x86_64/build.sh` modulo `-a aarch64` + target-triple cross-compile hook.

## Cross-compile from mac arm64 host

Two supported paths:
1. **Native CI runner** (preferred) — GH Actions `ubuntu-latest-arm64` runner; matrix entry in `.github/workflows/release.yml` produced by `dist init`.
2. **Local cross** — `cargo-zigbuild` for Rust components; Docker `--platform linux/arm64` for Python image builds.

## Smoke test

Fresh `arm64v8/ubuntu:22.04` container — same steps as `linux-x86_64/README.md`.

Write `pkg/.proof/linux-aarch64.ready.json`.
