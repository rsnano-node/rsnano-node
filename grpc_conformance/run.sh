#!/bin/sh
set -u

SUITE_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
COMPOSE_FILE="$SUITE_DIR/compose.yml"

mkdir -p "$SUITE_DIR/reports"
export LOCAL_UID="$(id -u)"
export LOCAL_GID="$(id -g)"

cleanup() {
    docker compose -f "$COMPOSE_FILE" down --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

docker compose -f "$COMPOSE_FILE" up \
    --build \
    --abort-on-container-exit \
    --exit-code-from conformance
