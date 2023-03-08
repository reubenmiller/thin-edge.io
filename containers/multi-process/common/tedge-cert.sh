#!/bin/sh

set -e

CERTIFICATE=${CERTIFICATE:-}
C8Y_URL=${C8Y_URL:-}
DEVICE_ID=${DEVICE_ID:-}
CSR_PATH=${CSR_PATH:-}

log() {
    level="$1"
    shift
    echo "$level  $*" >&2
}
info() { log "INFO" "$@";}
error() { log "ERROR" "$@";}
warn() { log "WARN" "$@";}

usage() {
    echo "
thin-edge.io certificate management script. The script is used to show and set the certificate and Cumulocity c8y url used for cloud connection

USAGE
    $0 show
    $0 csr --device-id <device_id> --out-csr /tmp/mycsr.csr
    $0 set --c8y-url <url> --certificate <path_to_cert>

SUBCOMMANDS
    show        Show device id
    csr         Create a certificate signing request and write to the give file output
    set         Set/update the certificate and/or the Cumulocity IoT Url

FLAGS
    --c8y-url <url>         Cumulocity IoT URL to the tenant which thin-edge should connect to
    --certificate <path>    Path to the public certificate to use when connecting to Cumulocity IoT
    --out-csr <path>        Path to write the csr file to. The directory must exist, otherwise the command will fail

EXAMPLES
    $0 show
    # Show the current device id

    $0 csr --device-id mydeviceid --out-csr /tmp/mycsr.csr
    # Create a CSR file with the given device id

    $0 set --c8y-url example.cumulocity.io --certificate /tmp/certgen/device-cert.pem
    # Set the Cumulocity IoT url and certificate. It will reconnect the cloud connection if necessary
    "
}

if [ $# -lt 1 ]; then
    error "Missing required positional argument"
    usage
    exit 0
fi

COMMAND="$1"
shift

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

        --certificate)
            CERTIFICATE="$2"
            shift
            ;;

        --out-csr)
            CSR_PATH="$2"
            shift
            ;;

        --help|-h)
            usage
            exit 0
            ;;

        -*)
            error "Unknown option. $1"
            usage
            exit 1
            ;;
    esac
    shift
done


get_device_id() {
    EXISTING_DEVICE_ID=$(tedge config get device.id 2>/dev/null ||:)
    if [ -n "$EXISTING_DEVICE_ID" ]; then
        echo "$EXISTING_DEVICE_ID"
        return
    fi

    if [ -n "$DEVICE_ID" ]; then
        echo "$DEVICE_ID"
        return
    fi

    error "Device id is not set. Either create an initial certificate or set the DEVICE_ID environment variable"
    exit 1
}

validate() {
    if [ -z "$CERTIFICATE" ]; then
        info "No new certificate provided. Skipping reconfiguration"
        exit 0
    fi

    if [ ! -f "$CERTIFICATE" ]; then
        error "Incoming certificate does not exist. path=$CERTIFICATE"
        exit 1
    fi
}

update_cert() {
    EXISTING_CERT_PATH=$(tedge config get device.cert.path)

    info "Update c8y cert"
    EXISTING_C8Y_URL=$(tedge config get c8y.url)

    NEEDS_RECONNECT=0

    if [ -z "$EXISTING_C8Y_URL" ] && [ -z "$C8Y_URL" ]; then
        error "There is no existing or new Cumulocity IoT URL defined. Aborting"
        exit 1
    fi

    if [ "$EXISTING_C8Y_URL" != "$C8Y_URL" ]; then
        info "Disconnecting mapper"
        tedge disconnect c8y ||:

        NEEDS_RECONNECT=1
        tedge config set c8y.url "$C8Y_URL"
    fi

    if [ "$EXISTING_CERT_PATH" != "$CERTIFICATE" ]; then
        info "Moving new certificate from $CERTIFICATE to $EXISTING_CERT_PATH"
        mv "$INCOMING_CERT" "$EXISTING_CERT_PATH"
    fi

    if [ "$NEEDS_RECONNECT" = "1" ]; then
        info "Connecting mapper"
        tedge connect c8y ||:
    fi
}

generate_csr() {
    outfile="$1"
    EXISTING_PRIVATE_CERT_PATH=$(tedge config get device.key.path)

    if [ -z "$DEVICE_ID" ]; then
        info "Trying to read device.id from any existing public certificates"
        DEVICE_ID=$(tedge config get device.id >/dev/null)
    fi

    if [ ! -f "$EXISTING_PRIVATE_CERT_PATH" ]; then
        if [ -z "$DEVICE_ID" ]; then
            error "Can not create certificates without having a DEVICE_ID! Please provide the DEVICE_ID and try again"
            exit 1
        fi
        tedge cert create --device-id "$DEVICE_ID"
    fi

    openssl req -new -subj "/C=ZZ/CN=$DEVICE_ID" -key "$EXISTING_PRIVATE_CERT_PATH" -out "$outfile"
}

main() {
    if ! command -v tedge >/dev/null 2>&1; then
        error "Could not find the tedge command. Please install it and try again"
        exit 1
    fi

    if ! command -v openssl >/dev/null 2>&1; then
        error "Could not find the openssl command. Please install it and try again"
        exit 1
    fi

    case "$COMMAND" in
        show)
            get_device_id
            ;;

        csr)
            generate_csr "$CSR_PATH"
            ;;

        set)
            validate
            update_cert
            ;;

        *)
            usage
            exit 1
            ;;
    esac
}

main
