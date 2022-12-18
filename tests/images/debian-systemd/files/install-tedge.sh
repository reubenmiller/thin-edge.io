#!/bin/bash

install_via_apt() {
    echo 'deb [trusted=yes] https://thinedgeio.jfrog.io/artifactory/stable stable main' > /etc/apt/sources.list.d/tedge.list
    apt-get update
    apt-get install -y mosquitto
    apt-get install -y tedge-full
}

install_via_script() {
    apt-get update
    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s

    # Add testing repository to install other artifacts
    echo 'deb [trusted=yes] https://thinedgeio.jfrog.io/artifactory/stable stable main' > /etc/apt/sources.list.d/tedge.list
}

case "$1" in
    apt)
        install_via_apt
        ;;
    
    *)
        # Remove system.toml as the latest official release does not support custom reboot command
        rm -f /etc/tedge/system.toml
        install_via_script
        ;;
esac
