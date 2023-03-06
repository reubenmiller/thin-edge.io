#!/bin/sh
set -e

create_user_group() {
    USER="$1"
    GROUP="${2:-$USER}"
    if ! getent group "$GROUP" >/dev/null; then

        if command -v groupadd >/dev/null 2>&1; then
            groupadd --system "$GROUP"
        else
            addgroup -S "$GROUP"
        fi
    fi

    if ! getent passwd "$USER" >/dev/null; then
        if command -v groupadd >/dev/null 2>&1; then
            useradd --system --no-create-home --shell /sbin/nologin --gid "$GROUP" "$USER"
        else
            adduser -g "" -H -D "$USER" -G "$GROUP"
        fi
    fi
}

create_user_group tedge
create_user_group mosquitto

# FIXME: The directory should not have to be created by the user
SHOULD_CLEANUP_LOCK_DIR=0
LOCK_DIR="/run/lock"
if [ ! -d "$LOCK_DIR" ]; then
    mkdir -p "$LOCK_DIR"
    SHOULD_CLEANUP_LOCK_DIR=1
fi

while [ $# -gt 0 ]; do
    CMD="$1"
    if command -v "$CMD" >/dev/null 2>&1; then
        "$CMD" --init
    fi
    shift
done

if [ "$SHOULD_CLEANUP_LOCK_DIR" = "1" ]; then
    rm -rf "$LOCK_DIR"
fi
