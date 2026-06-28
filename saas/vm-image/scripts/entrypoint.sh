#!/bin/sh
# Container entrypoint.
#
# Order:
#   1. Run first-boot.sh once (model fetch, neo4j init, write api token).
#   2. Exec supervisord (passed in via CMD).
set -eu

FIRST_BOOT_MARKER=/state/.first-boot-complete

if [ ! -f "$FIRST_BOOT_MARKER" ]; then
    echo "[entrypoint] Running first-boot.sh"
    /usr/local/bin/first-boot.sh
    touch "$FIRST_BOOT_MARKER"
fi

echo "[entrypoint] Handing off to: $*"
exec "$@"
