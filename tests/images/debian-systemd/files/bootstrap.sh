#!/bin/bash

set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

CONNECT=1
CHILDREN=0
INSTALL=1
while [ $# -gt 0 ]
do
    case "$1" in
        --no-install)
            INSTALL=0
            ;;

        --no-connect)
            CONNECT=0
            ;;

        --children)
            CHILDREN="$2"
            shift
            ;;
    esac
    shift
done

if [ "$INSTALL" == 1 ]; then
    echo "Installing thin-edge.io"
    "$SCRIPT_DIR/install-tedge.sh" "script"
fi

echo ----------------------------------------------------------
echo Bootstraping device
echo ----------------------------------------------------------
echo

PREFIX=${PREFIX:-tedge}

if [ -n "$PREFIX" ]; then
    PREFIX="${PREFIX}-"
fi

get_device_id() {
    if [ -n "$DEVICE_ID" ]; then
        echo "$DEVICE_ID"
        return
    fi

    if [ -n "$HOSTNAME" ]; then
        echo "${PREFIX}${HOSTNAME}"
        return
    fi
    if [ -n "$HOST" ]; then
        echo "${PREFIX}${HOST}"
        return
    fi
    echo "${PREFIX}unknown-device"
}

if [ -n "$C8Y_BASEURL" ]; then
    C8Y_HOST="$C8Y_BASEURL"
fi

if [ -z "$C8Y_HOST" ]; then
    echo "Missing Cumulocity Host url: C8Y_HOST" >&2
    exit 1
fi

# Strip any http/s prefixes
C8Y_HOST=$(echo "$C8Y_HOST" | sed -E 's|^.*://||g' | sed -E 's|/$||g')

echo "Setting c8y.url to $C8Y_HOST"
tedge config set c8y.url "$C8Y_HOST"

if ! tedge cert show >/dev/null 2>&1; then
    CERT_COMMON_NAME=$(get_device_id)
    echo "Creating certificate: $CERT_COMMON_NAME"
    tedge cert create --device-id "$CERT_COMMON_NAME"

    
    if [ -n "$C8Y_PASSWORD" ]; then
        echo "Uploading certificate to Cumulocity using tedge"
        C8YPASS="$C8Y_PASSWORD" tedge cert upload c8y --user "$C8Y_USER"
    fi
else
    echo "Certificate already exists"
fi

create_child() {
    local parent="$1"
    local name="$2"
    local fqdn="$parent-$name"
    local child_path="/etc/tedge/operations/c8y/$fqdn"
    mkdir -p "$child_path"
    touch "$child_path/c8y_Command"
    touch "$child_path/c8y_Restart"
}

if [[ "$CHILDREN" -gt 0 ]]; then
    echo "Adding $CHILDREN children to device"
    parent=$(tedge config get device.id)

    TOTAL_CHILDREN=0
    while [[ "$TOTAL_CHILDREN" -lt "$CHILDREN" ]]; do
        create_child "$parent" "child-$TOTAL_CHILDREN"
        TOTAL_CHILDREN=$((TOTAL_CHILDREN+1))
    done
fi


if [[ "$CONNECT" == 1 ]]; then
    tedge connect c8y
fi

# Add additional tools
systemctl start ssh

if id -u docker >/dev/null 2>&1; then
    useradd -ms /bin/bash docker && echo "docker:docker" | chpasswd && adduser docker sudo
fi

echo "Setting sudoers.d config"
echo '%sudo ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/all
echo 'tedge  ALL = (ALL) NOPASSWD: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown' > /etc/sudoers.d/tedge
