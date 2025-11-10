#!/bin/sh
set -e

DEVICE_ID="${DEVICE_ID:-}"
C8Y_URL="${C8Y_URL:-}"
DEVICE_ONE_TIME_PASSWORD="${DEVICE_ONE_TIME_PASSWORD:-}"
export TEDGE_CONFIG_DIR="${TEDGE_CONFIG_DIR:-/etc/tedge}"

ACTION=register

usage() {
    cat <<EOT
Enroll a device with Cumulocity

$0 [OPTIONS]

ARGUMENTS
  --c8y-url <url>           Cumulocity URL
  --device-id <string>      Device ID to use during initialization. Defaults to first non-zero value from: DEVICE_ID env, tedge-identity, hostname
  -p, --one-time-password <string>      one-time-password use to request the certificate from the Cumulocity CA
  --debug                   Enable debugging
  -h, --help                Show this help

EXAMPLES

## Enroll a device

sudo $0 --device-id "gateway01" --c8y-url "example.cumulocity.com"

EOT
}

#
# Parse arguments
#
while [ $# -gt 0 ]; do
    case "$1" in
        --device-id)
            DEVICE_ID="$2"
            shift
            ;;
        --c8y-url)
            C8Y_URL="$2"
            shift
            ;;
        # Cumulocity Enrollment token
        --one-time-password|-p)
            DEVICE_ONE_TIME_PASSWORD="$2"
            shift
            ;;
        --debug)
            set -x
            ;;
        --help|-h)
            usage
            exit 0
            ;;
    esac
    shift
done

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

if [ "$(id -u)" -ne 0 ]; then
    fail "Script must be run as root"
fi

if [ -z "${DEVICE_ID:-}" ]; then
    DEVICE_ID=$(tedge config get device.id 2>/dev/null || tedge-identity 2>/dev/null || hostname)
fi

if [ -z "${C8Y_URL:-}" ]; then
    C8Y_URL=$(tedge config get c8y.url 2>/dev/null ||:)
fi


get_random_code() {
    awk '
function rand_string(n,         s,i) {
    for ( i=1; i<=n; i++ ) {
        s = s chars[int(1+rand()*numChars)]
    }
    return s
}
BEGIN{
    srand()
    for (i=48; i<=122; i++) {
        char = sprintf("%c", i)
        if ( char ~ /[[:alnum:]]/ ) {
            chars[++numChars] = char
        }
    }

    for (i=1; i<=1; i++) {print rand_string(30)}
}'
}

is_enrolled() {
    tedge cert show;
}

register() {
    if is_enrolled; then
        echo "The device has already been onboarded" >&2
        exit 1
    fi

    if [ -n "$C8Y_URL" ]; then
        # normalize the valeus
        C8Y_URL=$(echo "$C8Y_URL" | sed 's|https?://||g')
        tedge config set c8y.url "$C8Y_URL"
    fi
    
    tedge cert create-csr --device-id "$DEVICE_ID"

    if [ -z "$DEVICE_ONE_TIME_PASSWORD" ]; then
        # User didn't provide a value, so generate a randomized code
        DEVICE_ONE_TIME_PASSWORD=$(get_random_code)
    fi

    if [ -n "$C8Y_URL" ]; then
        echo "Register in Cumulocity using:" >&2
        echo "" >&2
        echo "  https://$C8Y_URL/apps/devicemanagement/index.html#/deviceregistration?externalId=$DEVICE_ID&one-time-password=$DEVICE_ONE_TIME_PASSWORD" >&2
        echo "" >&2
    fi

    tedge cert download c8y --device-id "$DEVICE_ID" --one-time-password "$DEVICE_ONE_TIME_PASSWORD" --retry-every 5s
    tedge reconnect c8y
    printf '\n\nDevice was enrolled successfully\n'  >&2
}

case "$ACTION" in
    register)
        register
        ;;
    *)
        echo "No action given by the user" >&2
        ;;
esac
