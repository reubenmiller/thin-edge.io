*** Variables ***

# Adapter settings
${DEVICE_ADAPTER}    docker
&{SSH_CONFIG}        hostname=%{SSH_CONFIG_HOSTNAME= }    username=%{SSH_CONFIG_USERNAME= }    password=%{SSH_CONFIG_PASSWORD= }    skip_bootstrap=False    bootstrap_script=%{SSH_CONFIG_BOOTSTRAP_SCRIPT= }
&{DOCKER_CONFIG}     image=%{DOCKER_CONFIG_IMAGE=debian-systemd}    bootstrap_script=%{DOCKER_CONFIG_BOOTSTRAP_SCRIPT=/setup/bootstrap.sh}

# Cumulocity settings
&{C8Y_CONFIG}        host=%{C8Y_BASEURL= }    username=%{C8Y_USER= }    password=%{C8Y_PASSWORD= }    tenant=%{C8Y_TENANT= }
