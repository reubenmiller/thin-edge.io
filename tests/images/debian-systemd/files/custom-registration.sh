#!/usr/bin/env bash
set -e

DEVICE_ID=${DEVICE_ID:-}
DEVICE_ONE_TIME_PASSWORD=${DEVICE_ONE_TIME_PASSWORD:-}
CERT_FILE=${CERT_FILE:-}
RESTART=${RESTART:-1}
DEBUG=${DEBUG:-0}
QR_CODE=${QR_CODE:-1}
OPEN_WEBSITE=${OPEN_WEBSITE:-1}

help() {
    cat <<EOT
$0 [DEVICE_ID] [--otp <DEVICE_ONE_TIME_PASSWORD>]

Enroll a device to Cumulocity using the Cumulocity certificate-authority feature

Positional Args:
  DEVICE_ID             Device ID to enrol, e.g. tedge0001. The tedge-identity or hostname will be used if it is not given by the user

Flags
  --url <string>            Cumulocity Url. Defaults to 'c8y.url' setting.
  --otp <string>           Device code used for providing a unique device code to request the first certificate from the platform
  --reconnect               Reconnect the mappers after replacing the certificate
  --qr|--no-qr              Show/hide Enrolment QR Code
  --no-reconnect            Don't reconnect the mappers after replacing the certificate
  --help, -h                Show this help

Examples

  $0 --url example.c8y.io
  # Enroll device to Cumulocity

  $0 mydevice001 --url example.c8y.io
  # Enroll use a user specified

  $0 mydevice001 --otp 8ajas7d013jdsd671kjd
  # Enroll device using an already known device one-time-password (that should be uploaded to the platform already)

EOT
}

POSITIONAL_ARGS=()
while [ $# -gt 0 ]; do
    case "$1" in
        --url)
            C8Y_HOST="$2"
            shift
            ;;
        --otp)
            DEVICE_ONE_TIME_PASSWORD="$2"
            shift
            ;;
        --qr)
            QR_CODE=1
            ;;
        --no-qr)
            QR_CODE=0
            ;;
        --reconnect)
            RESTART=1
            ;;
        --no-reconnect)
            RESTART=0
            ;;
        --help|-h)
            help
            exit 0
            ;;
        --debug)
            DEBUG=1
            ;;
        --*|*-)
            echo "Unknown flag" >&2
            help
            exit 1
            ;;
        *)
            POSITIONAL_ARGS+=("$1")
            ;;
    esac
    shift
done

set -- "${POSITIONAL_ARGS[@]}"

get_random_code() {
    awk -v r=$RANDOM '
function rand_string(n,         s,i) {
    for ( i=1; i<=n; i++ ) {
        s = s chars[int(1+rand()*numChars)]
    }
    return s
}
BEGIN{
    srand(r)      # Use srand ONCE only
    for (i=48; i<=122; i++) {
        char = sprintf("%c", i)
        if ( char ~ /[[:alnum:]]/ ) {
            chars[++numChars] = char
        }
    }

    for (i=1; i<=1; i++) {print rand_string(30)}
}'
}

main() {
    if [ "$(tedge config get c8y.url)" != "$C8Y_HOST" ]; then
        echo "Setting c8y.url $C8Y_HOST" >&2
        tedge config set c8y.url "$C8Y_HOST"
    fi

    if [ "$DISPLAY_DEVICE_CODE" = 1 ]; then
        echo
        echo "Please register the device in your Cumulocity tenant"
        echo "(the device will poll every 5 seconds)"
        echo
        if [ -n "$QR_CODE" ]; then
            echo "Scan the QR Code below with your phone"
            echo
            curl qrcode.show -d "https://$C8Y_HOST/apps/devicemanagement/index.html#/deviceregistration?externalId=$DEVICE_ID&one-time-password=$DEVICE_ONE_TIME_PASSWORD"
            echo "  Device:      $DEVICE_ID"
            echo "  Device Code: $DEVICE_ONE_TIME_PASSWORD"
            echo "  Cumulocity:  https://$C8Y_HOST/apps/devicemanagement/index.html#/deviceregistration?externalId=$DEVICE_ID&one-time-password=$DEVICE_ONE_TIME_PASSWORD"
            echo
        else
            echo "  Cumulocity:  https://$C8Y_HOST/apps/devicemanagement/index.html#/deviceregistration"
            echo "  Device:      $DEVICE_ID"
            echo "  OTP:         $DEVICE_ONE_TIME_PASSWORD"
            echo
        fi
    fi

    if ! tedge cert download c8y --retry-every 5s --max-timeout 5m --device-id "$DEVICE_ID" --token "$DEVICE_ONE_TIME_PASSWORD" 2>/dev/null; then
        echo "Failed to get device certificate." >&2
        exit 1
    fi

    # Restart thin-edge.io
    if [ "$RESTART" = 1 ]; then
        echo "Reconnecting mapper" >&2
        tedge reconnect c8y
    fi

    # Open device management page for the device
    if [ "$OPEN_WEBSITE" = 1 ]; then
        if command -V c8y >/dev/null 2>&1; then
            c8y identity get -n --name "$DEVICE_ID" --retries 5 | c8y applications open --application devicemanagement --page "device-info"
        fi
    fi
}

#
# Main
#

# Set defaults
if [ "$DEBUG" = 1 ]; then
    set -x
fi

if [ -z "$C8Y_HOST" ]; then
    C8Y_HOST=$(tedge config get c8y.url)
fi
# Normalize it by stripping the scheme
# shellcheck disable=SC2001
C8Y_HOST=$(echo "$C8Y_HOST" | sed 's|.*://||g')

if [ -z "$C8Y_HOST" ]; then
    echo "Error: You need to set the tedge c8y.url first, or pass the --url flag" >&2
    exit 1
fi

if [ $# -gt 0 ]; then
    DEVICE_ID="$1"
fi

if [ -z "$DEVICE_ID" ]; then
    DEVICE_ID=$(tedge-identity 2>/dev/null || hostname)
fi

if [ -z "$DEVICE_ONE_TIME_PASSWORD" ]; then
    DEVICE_ONE_TIME_PASSWORD=$(get_random_code)
    DISPLAY_DEVICE_CODE=1
fi

main
