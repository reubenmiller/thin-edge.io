#!/bin/sh

# Wrapper script for c8y-remote-access-plugin to provide socket activation on OpenWrt/procd
# This replicates systemd's socket activation behavior using socat

exec socat UNIX-LISTEN:/run/c8y-remote-access-plugin.sock,fork,mode=660,user=tedge,group=tedge EXEC:/usr/bin/c8y-remote-access-plugin --child -