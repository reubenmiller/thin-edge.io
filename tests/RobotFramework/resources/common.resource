*** Settings ***
# Library    Cumulocity
# Library    ThinEdgeIO

*** Variables ***

# Adapter settings
${DEVICE_ADAPTER}    docker
&{SSH_CONFIG}        hostname=%{SSH_CONFIG_HOSTNAME= }    username=%{SSH_CONFIG_USERNAME= }    password=%{SSH_CONFIG_PASSWORD= }    skip_bootstrap=True
&{DOCKER_CONFIG}     image=%{DOCKER_CONFIG_IMAGE=debian-systemd}    bootstrap_script=/setup/bootstrap.sh
