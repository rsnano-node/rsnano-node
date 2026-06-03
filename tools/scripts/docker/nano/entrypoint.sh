#!/bin/sh
set -e

DATA_DIR=/home/nanocurrency/Nano
APP_DIR="$DATA_DIR"
NEXT_ARG_IS_DATA_PATH=false

for arg in "$@"; do
    if [ "$NEXT_ARG_IS_DATA_PATH" = true ]; then
        APP_DIR="$arg"
        NEXT_ARG_IS_DATA_PATH=false
        continue
    fi

    case "$arg" in
        --data-path=*|--data_path=*)
            APP_DIR="${arg#*=}"
            ;;
        --data-path|--data_path)
            NEXT_ARG_IS_DATA_PATH=true
            ;;
    esac
done

mkdir -p "$DATA_DIR" "$APP_DIR"

APP_PARENT="$(dirname "$APP_DIR")"
case "$APP_PARENT" in
    /root|/home/nanocurrency)
        # V2.0 Docker images ran as root, so existing setups may keep data under /root.
        chown nanocurrency:nanocurrency "$APP_PARENT"
        ;;
esac

chown -R nanocurrency:nanocurrency "$DATA_DIR" "$APP_DIR"

export HOME=/home/nanocurrency

exec gosu nanocurrency /usr/bin/rsnano "$@"
