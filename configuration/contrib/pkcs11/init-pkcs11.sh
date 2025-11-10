#!/bin/sh
set -e

TOKEN_URL="${TOKEN_URL:-}"

export GNUTLS_PIN="${GNUTLS_PIN:-}"
export GNUTLS_SO_PIN="${GNUTLS_SO_PIN:-}"
export TOKEN_LABEL="${TOKEN_LABEL:-tedge}"
export TEDGE_CONFIG_DIR="${TEDGE_CONFIG_DIR:-/etc/tedge}"
export PUBLIC_KEY="${PUBLIC_KEY:-${TEDGE_CONFIG_DIR}/device-certs/tedge.pub}"

# Only used for TPM 2.0
export TPM2_PKCS11_STORE="${TPM2_PKCS11_STORE:-/etc/tedge/hsm}"

PKCS11_MODULE="${PKCS11_MODULE:-}"

ACTION="create"

HSM_TYPE="${HSM_TYPE:-}"

usage() {
    cat <<EOT
# Note: self-signed certificates are not supported!

$0 [OPTIONS]

ARGUMENTS
  --type <string>           Type of HSM (using the PKCS#11 interface) to use. Available values: [softhsm2, yubikey, nitrokey, tpm2]
  --token-url <url>         Token PKCS#11 URL which is to be used for initialization.
  --label <string>          Token label to be associated with the created key pair. Defaults to tedge
  --pin <string>            Pin used to access the HSM
  --so-pin <string>         Special pin
  --module <path>           Path to the PKCS#11 module to use
  --debug                   Enable debugging
  -h, --help                Show this help

EXAMPLES

## Initialization

### Nitrokey

$0 --type nitrokey --token-url 'pkcs11:model=PKCS%2315%20emulated;manufacturer=www.CardContact.de;serial=DENK0400089;token=SmartCard-HSM%20%28UserPIN%29'
# Initialize private key using nitrokey, where you have to specify the slot where the nitrokey is accessible from


### SoftHSM2

$0 --type softhsm2
# Initialize private key using softhsm2, and use the Cumulocity CA to request a certificate


### TPM2

$0 --type tpm2 --token-url 'pkcs11:model=SLB9672%00%00%00%00%00%00%00%00%00;manufacturer=Infineon;serial=0000000000000000;token='
# Initialize private key using a tpm 2.0 module, and use the Cumulocity CA to request a certificate

EOT
}

#
# Parse arguments
#
while [ $# -gt 0 ]; do
    case "$1" in
        --label)
            TOKEN_LABEL="$2"
            shift
            ;;
        --token-url)
            TOKEN_URL="$2"
            shift
            ;;
        --pin)
            GNUTLS_PIN="$2"
            shift
            ;;
        --so-pin)
            GNUTLS_SO_PIN="$2"
            shift
            ;;
        --module)
            PKCS11_MODULE="$2"
            shift
            ;;
        --type)
            HSM_TYPE="$2"
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

if [ -z "$PKCS11_MODULE" ]; then
    VALUE=$(tedge config get device.cryptoki.module_path 2>/dev/null ||:)
    if [ -n "$VALUE" ]; then
        if [ -f "$VALUE" ]; then
            PKCS11_MODULE="$VALUE"
        else
            tedge config unset device.cryptoki.module_path 2>/dev/null ||:
        fi
    fi
fi

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

if [ "$(id -u)" -ne 0 ]; then
    fail "Script must be run as root"
fi

# Set module defaults
find_pkcs11_module() {
    if [ -n "$PKCS11_MODULE" ]; then
        # module is already set
        return
    fi

    case "$HSM_TYPE" in
        yubikey)
            PKCS11_MODULE=$(find /usr/lib -name libykcs11.so | head -n1)
            ;;
        softhsm2|softhsm)
            PKCS11_MODULE=$(find /usr/lib -name libsofthsm2.so | head -n1)
            ;;
        nitrokey)
            PKCS11_MODULE=$(find /usr/lib -name opensc-pkcs11.so | head -n1)
            ;;
        tpm2)
            PKCS11_MODULE=$(find /usr/lib -name libtpm2_pkcs11.so | head -n1)
            ;;
        *)
            # Don't use an explicit pkcs11 module, let the tooling choose the default
            ;;
    esac
}

#
# Enable usage with thin-edge.io
#

configure_tedge() {
    tedge config set mqtt.bridge.built_in true
    tedge config set device.cryptoki.mode socket
    tedge config set device.cryptoki.module_path "$PKCS11_MODULE"
    tedge config set device.cryptoki.pin "$GNUTLS_PIN"
}

init_private_key() {
    # set common arguments to ensure p11tool finds the correct module if there are multiple
    P11_TOOL_ARGS=
    PKCS11_MODULE=$(tedge config get device.cryptoki.module_path ||:)
    if [ -n "$PKCS11_MODULE" ]; then
        P11_TOOL_ARGS="--provider=$PKCS11_MODULE"
    fi

    get_token() {
        p11tool $P11_TOOL_ARGS --list-tokens 2>/dev/null | grep "token=$TOKEN_LABEL" | awk '{ print $2 }' | head -n1
    }

    run_p11tool() {
        PKCS11_MODULE=$(tedge config get device.cryptoki.module_path ||:)
        p11tool --provider="$PKCS11_MODULE" "$@"
    }

    case "$1" in
        yubikey)
            # Note: yubikey doesn't support creating a private key using the pkcs11 interface, so ykman needs to be used
            # For most pkcs11 compatible HSM's, certtool can get the public key automatically, but for Yubikey
            # you need to manually export the key using 'ykman piv keys export 9a "<path>"'
            PUBLIC_KEY="${PUBLIC_KEY:-$TEDGE_CONFIG_DIR/device-certs/tedge.pub}"
            ykman piv keys generate --algorithm ECCP256 9a "$PUBLIC_KEY"
            ;;
        nitrokey)
            # run_p11tool --initialize-pin "$TOKEN_URL"
            p11tool $P11_TOOL_ARGS --initialize-pin "$TOKEN_URL"
            ;;
        tpm2)
            # TODO: Should the store be removed if the user wants to re-initialize it?
            # rm -rf "$TPM2_PKCS11_STORE"
            usermod -a -G tss tedge

            mkdir -p "$TPM2_PKCS11_STORE"
            chown -R tedge:tedge "$TPM2_PKCS11_STORE"

            if ! grep -q '^TPM2_PKCS11_STORE=.\+' "$TEDGE_CONFIG_DIR/plugins/tedge-p11-server.conf"; then
                cat <<EOT > "$TEDGE_CONFIG_DIR/plugins/tedge-p11-server.conf"
# TPM specific settings
TPM2_PKCS11_STORE="$TPM2_PKCS11_STORE"
EOT
            fi

            # must be run as the tedge user
            sudo -u tedge p11tool $P11_TOOL_ARGS --initialize --label "$TOKEN_LABEL" --set-pin "$GNUTLS_PIN" --set-so-pin "$GNUTLS_SO_PIN" "$TOKEN_URL"

            # refresh as there should be a new token created
            # TODO: Is this step necessary, e.g. does a newly created token need to be initialized again?
            # TOKEN_URL=$(p11tool $P11_TOOL_ARGS --list-token-urls | grep "token=$TOKEN_LABEL" | head -n 1)
            # p11tool $P11_TOOL_ARGS --initialize-pin "$TOKEN_URL"
            ;;
        softhsm2|softhsm)
            usermod -a -G softhsm tedge
            softhsm2-util --init-token --free --label "$TOKEN_LABEL" --pin "$GNUTLS_PIN" --so-pin "$GNUTLS_SO_PIN"

            # TODO: How to limit changing ownership to the token which was created, as each
            # token is stored in a subfolder, so we should only change the one that was just created
            # chown -R tedge:softhsm /var/lib/softhsm/tokens/*
            ;;
        *)
            echo "Warning: Unknown HSM type (name=$1). Trying to initialize using standard p11tool commands" >&2
            p11tool $P11_TOOL_ARGS --initialize-pin "$TOKEN_URL"
            ;;
    esac

    # Restart the existing tedge-p11-server instance so it can reload the new key (used later on)
    if command -V systemctl >/dev/null 2>&1; then
        systemctl restart tedge-p11-server.socket ||:
    fi

    # TODO: Check if the private key has already been created
    echo "Creating a private key" 2>&1
    TOKEN_URL="pkcs11:token=$TOKEN_LABEL"
    tedge cert create-key-hsm \
        --label "$TOKEN_LABEL" \
        --outfile-pubkey="$PUBLIC_KEY" \
        "$TOKEN_URL"
}


#
# Main
#
case "$ACTION" in
    create)
        # TODO: check if a private key already exists
        echo "Using Token URL: $TOKEN_URL" >&2

        systemctl enable tedge-p11-server.socket ||:

        find_pkcs11_module
        configure_tedge
        init_private_key "$HSM_TYPE"
        ;;
    *)
        echo "No action given by the user" >&2
        ;;
esac
