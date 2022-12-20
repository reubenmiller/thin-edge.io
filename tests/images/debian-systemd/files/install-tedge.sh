#!/bin/bash

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
        tedge-watchdog \
        tedge-agent \
        tedge-apt-plugin \
        c8y-log-plugin \
        c8y-configuration-plugin
}

install_via_script() {
    apt-get update
    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s    
}

# Add testing repository to install other artifacts
configure_repos

case "$1" in
    apt)
        echo "Installing thin-edge.io using apt"
        install_via_apt
        ;;
    
    *)
        echo "Installing thin-edge.io using the 'get-thin-edge_io.sh' script"
        # Remove system.toml as the latest official release does not support custom reboot command
        rm -f /etc/tedge/system.toml
        install_via_script
        ;;
esac
