# macOS arm64 installer

Ships a signed + notarized `.pkg` that places the `neuro-link` CLI into `/usr/local/bin` and a LaunchAgent into `~/Library/LaunchAgents/com.hyperfrequency.neuro-link.plist`.

## Certs (required on the build host)

- `Developer ID Application: Dan Repaci (TEAMID)` — signs the binary itself with hardened runtime.
- `Developer ID Installer: Dan Repaci (TEAMID)` — signs the .pkg.
- App-specific password stored in the Keychain via `xcrun notarytool store-credentials AC_PASSWORD`.

## Flow

`pkg/macos/build.sh <VERSION>` runs:

```bash
codesign --force --options runtime --timestamp \
  --sign "$APP_CERT" dist/root/usr/local/bin/neuro-link
pkgbuild --root dist/root --identifier com.hyperfrequency.neuro-link \
  --version "$VERSION" --install-location / \
  --scripts pkg/macos/scripts \
  --sign "$INSTALLER_CERT" dist/component.pkg
productbuild --distribution pkg/macos/resources/distribution.xml \
  --package-path dist --resources pkg/macos/resources \
  --sign "$INSTALLER_CERT" dist/neuro-link-$VERSION-arm64.pkg
xcrun notarytool submit dist/neuro-link-$VERSION-arm64.pkg \
  --keychain-profile "AC_PASSWORD" --wait
xcrun stapler staple dist/neuro-link-$VERSION-arm64.pkg
spctl -a -vv -t install dist/neuro-link-$VERSION-arm64.pkg
```

## Smoke test (before stamping READY)

1. `installer -pkg dist/neuro-link-$VERSION-arm64.pkg -target /` (as root, in a throwaway VM or an `--install-location /tmp/staging`).
2. `neuro-link --version` exits 0 and echoes `$VERSION`.
3. `launchctl list | grep com.hyperfrequency.neuro-link` — daemon present and pid > 0.
4. Model triple present at `$NLR_MODELS_DIR` (post-install-hook pulls them via `huggingface-cli`).
5. `curl -s http://localhost:8080` → optuna-dashboard HTML.
6. Write `pkg/.proof/macos-arm64.ready.json`.

## Uninstall

`pkg/macos/scripts/preinstall` handles upgrade cleanup; a standalone `neuro-link --uninstall` (added to the CLI) removes the LaunchAgent and `/usr/local/bin/neuro-link`.
