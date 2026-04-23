# Linux x86_64 installer

Emits three artifacts per build:
- `dist/neuro-link_$VERSION_amd64.deb` (fpm `-t deb -a amd64`)
- `dist/neuro-link-$VERSION-1.x86_64.rpm` (fpm `-t rpm -a x86_64`)
- `dist/neuro-link-$VERSION-x86_64.AppImage` (optional; requires `appimagetool`)

## Flow

`pkg/linux-x86_64/build.sh <VERSION>`:

```bash
fpm -s dir -t deb -n neuro-link -v $VERSION -a amd64 \
    --prefix /usr/local \
    --deb-systemd pkg/linux-x86_64/systemd/neuro-link.service \
    --after-install pkg/linux-x86_64/scripts/postinstall.sh \
    --before-remove pkg/linux-x86_64/scripts/preremove.sh \
    --description "neuro-link RAG + MCP server" \
    --url "https://github.com/HyperFrequency/neuro-link" \
    -C build-x86_64 . -p dist/neuro-link_${VERSION}_amd64.deb

fpm -s dir -t rpm -n neuro-link -v $VERSION -a x86_64 \
    --prefix /usr/local \
    --rpm-systemd pkg/linux-x86_64/systemd/neuro-link.service \
    --after-install pkg/linux-x86_64/scripts/postinstall.sh \
    --before-remove pkg/linux-x86_64/scripts/preremove.sh \
    -C build-x86_64 . -p dist/neuro-link-${VERSION}-1.x86_64.rpm
```

## Smoke test (before stamping READY)

Inside a fresh container (Ubuntu 22.04 + Fedora 40 minimums):
1. Install the .deb / .rpm.
2. `neuro-link --version` exits 0.
3. `systemctl status neuro-link` → active (running).
4. Model triple present post-install.
5. `curl -s http://localhost:8080` → optuna-dashboard.
6. Write `pkg/.proof/linux-x86_64.ready.json`.
