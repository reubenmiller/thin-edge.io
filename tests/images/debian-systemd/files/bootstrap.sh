#!/bin/bash

set -e

# SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

CONNECT=1
INSTALL=1
INSTALL_METHOD=apt
INSTALL_SOURCEDIR=${INSTALL_SOURCEDIR:-.}
MAX_CONNECT_ATTEMPTS=2

while [ $# -gt 0 ]
do
    case "$1" in
        --no-install)
            INSTALL=0
            ;;

        --no-connect)
            CONNECT=0
            ;;

        --install-method)
            # Either "apt", "script" or "local". Unknown options will use "script"
            INSTALL_METHOD="$2"
            shift
            ;;

        --install-sourcedir)
            # Source install directory if install method "local" is used. Location of the .deb files
            INSTALL_SOURCEDIR="$2"
            shift
            ;;
    esac
    shift
done

# ---------------------------------------
# Install helpers
# ---------------------------------------
configure_repos() {
    if [ ! -f /etc/apt/keyrings/thin-edge.io.gpg ]; then
        mkdir -p /etc/apt/keyrings
        curl -fsSL https://thinedgeio.jfrog.io/artifactory/api/security/keypair/thin-edge/public | sudo gpg --dearmor -o /etc/apt/keyrings/thin-edge.io.gpg
    fi

    if [ ! -f /etc/apt/sources.list.d/tedge.list ]; then
        echo 'deb [signed-by=/etc/apt/keyrings/thin-edge.io.gpg] https://thinedgeio.jfrog.io/artifactory/stable stable main' > /etc/apt/sources.list.d/tedge.list
    fi

    if [ ! -f /etc/apt/sources.list.d/tedge-main.list ]; then
        echo 'deb [signed-by=/etc/apt/keyrings/thin-edge.io.gpg] https://thinedgeio.jfrog.io/artifactory/debian-development tedge-main main' > /etc/apt/sources.list.d/tedge-main.list
    fi
}

install_via_apt() {
    apt-get update
    apt-get install -y mosquitto

    apt-get install -y \
        tedge \
        tedge-mapper \
        tedge-agent \
        tedge-apt-plugin \
        c8y-configuration-plugin \
        c8y-log-plugin \
        tedge-watchdog
}

install_via_script() {
    apt-get update
    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s    
}

install_via_local_files() {
    apt-get update
    apt-get install -y mosquitto

    find "$INSTALL_SOURCEDIR" -name "tedge_[0-9]*.deb" -exec dpkg -i {} \;
    find "$INSTALL_SOURCEDIR" -name "tedge[_-]mapper_*.deb" -exec dpkg -i {} \;
    find "$INSTALL_SOURCEDIR" -name "tedge[_-]agent_*.deb" -exec dpkg -i {} \;
    find "$INSTALL_SOURCEDIR" -name "tedge[_-]apt[_-]plugin_*.deb" -exec dpkg -i {} \;
    find "$INSTALL_SOURCEDIR" -name "c8y[_-]configuration[_-]plugin_*.deb" -exec dpkg -i {} \;
    find "$INSTALL_SOURCEDIR" -name "c8y[_-]log[_-]plugin_*.deb" -exec dpkg -i {} \;
    find "$INSTALL_SOURCEDIR" -name "tedge[_-]watchdog_*.deb" -exec dpkg -i {} \;
}


# ---------------------------------------
# Install
# ---------------------------------------
if [ "$INSTALL" == 1 ]; then
    echo ----------------------------------------------------------
    echo Installing thin-edge.io
    echo ----------------------------------------------------------
    echo
    configure_repos

    INSTALL_METHOD=${INSTALL_METHOD:-script}

    case "$INSTALL_METHOD" in
        apt)
            echo "Installing thin-edge.io using apt"
            install_via_apt
            ;;

        local)
            if [ $# -gt 1 ]; then
                LOCAL_PATH="$2"
            fi
            echo "Installing thin-edge.io using local files (from path=$LOCAL_PATH)"
            install_via_local_files
            ;;

        *)
            echo "Installing thin-edge.io using the 'get-thin-edge_io.sh' script"
            # Remove system.toml as the latest official release does not support custom reboot command
            rm -f /etc/tedge/system.toml
            install_via_script
            ;;
    esac
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

        # Grace period for the server to process the certificate
        sleep 1
    fi
else
    echo "Certificate already exists"
fi

if [[ "$CONNECT" == 1 ]]; then
    # retry connection attempts
    CONNECT_ATTEMPT=0
    while true; do
        CONNECT_ATTEMPT=$((CONNECT_ATTEMPT + 1))
        if tedge connect c8y; then
            break
        else
            if [ "$CONNECT_ATTEMPT" -ge "$MAX_CONNECT_ATTEMPTS" ]; then
                echo "Failed after $CONNECT_ATTEMPT connection attempts. Giving up"
                exit 2
            fi
        fi

        echo "WARNING: Connection attempt failed ($CONNECT_ATTEMPT of $MAX_CONNECT_ATTEMPTS)! Retrying to connect in 2s"
        sleep 2
    done
fi

# Add additional tools
systemctl start ssh

if id -u docker >/dev/null 2>&1; then
    useradd -ms /bin/bash docker && echo "docker:docker" | chpasswd && adduser docker sudo
fi

echo "Setting sudoers.d config"
echo '%sudo ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/all
echo 'tedge  ALL = (ALL) NOPASSWD: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown' > /etc/sudoers.d/tedge
