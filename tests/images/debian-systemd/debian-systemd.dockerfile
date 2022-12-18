FROM debian:11-slim

ARG DEVICEID=tedge_alpine
ARG C8YURL=mqtt.cumulocity.com
ARG AZURL=example.azure-devices.net

# We need curl to get root certificates
RUN apt-get -y update \
    && apt-get -y install \
        wget \
        curl \
        gnupg2 \
        sudo \
        systemd \
        apt-transport-https \
        ca-certificates \
        ssh \
        vim.tiny

# Install additional tools
# RUN curl https://reubenmiller.github.io/go-c8y-cli-repo/debian/PUBLIC.KEY | gpg --dearmor > /usr/share/keyrings/go-c8y-cli-archive-keyring.gpg \
#     && echo 'deb [signed-by=/usr/share/keyrings/go-c8y-cli-archive-keyring.gpg] http://reubenmiller.github.io/go-c8y-cli-repo/debian stable main' > /etc/apt/sources.list.d/go-c8y-cli.list \
#     && apt-get update && export DEBIAN_FRONTEND=noninteractive \
#     && apt-get -y install --no-install-recommends go-c8y-cli

# Remove unnecessary systemd services
RUN rm -f /lib/systemd/system/multi-user.target.wants/* \
    /etc/systemd/system/*.wants/* \
    /lib/systemd/system/local-fs.target.wants/* \
    /lib/systemd/system/sockets.target.wants/*udev* \
    /lib/systemd/system/sockets.target.wants/*initctl* \
    /lib/systemd/system/sysinit.target.wants/systemd-tmpfiles-setup* \
    /lib/systemd/system/systemd-update-utmp*

WORKDIR /setup
COPY files/install-tedge.sh .
COPY files/bootstrap.sh .
COPY files/system.toml /etc/tedge/
COPY files/c8y-configuration-plugin.toml /etc/tedge/c8y/

# Custom mosquitto config
COPY files/mosquitto.conf /etc/mosquitto/conf.d/

# Reference: https://developers.redhat.com/blog/2019/04/24/how-to-run-systemd-in-a-container#enter_podman
# STOPSIGNAL SIGRTMIN+3 (=37)
STOPSIGNAL 37

ENV CI=true
CMD ["/lib/systemd/systemd"]
