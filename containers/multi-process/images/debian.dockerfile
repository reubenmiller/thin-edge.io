ARG DEBIAN_IMAGE="debian:11-slim"
FROM "${DEBIAN_IMAGE}"

ARG DEVICE_ID=
ARG INSTALL=1
ARG CONNECT=0
ARG BOOTSTRAP=0
ARG TEDGE_MAPPER=c8y
ARG REPO_CHANNEL=main
ARG INSTALL_METHOD=apt
ARG TEST_USER=customeradmin

# Install
RUN apt-get -y update \
    && DEBIAN_FRONTEND=noninteractive apt-get -y --no-install-recommends install \
        wget \
        curl \
        gnupg2 \
        sudo \
        ssh \
        apt-transport-https \
        ca-certificates \
        systemd \
        procps \
        mosquitto \
        mosquitto-clients \
        vim.tiny

# Remove unnecessary systemd services
RUN rm -f /lib/systemd/system/multi-user.target.wants/* \
    /etc/systemd/system/*.wants/* \
    /lib/systemd/system/local-fs.target.wants/* \
    /lib/systemd/system/sockets.target.wants/*udev* \
    /lib/systemd/system/sockets.target.wants/*initctl* \
    /lib/systemd/system/sysinit.target.wants/systemd-tmpfiles-setup* \
    /lib/systemd/system/systemd-update-utmp*

# Custom ssh settings
COPY config/strict-ssh.conf /etc/ssh/sshd_config.d/

COPY common/bootstrap.sh common/tedge-cert.sh /usr/bin/
RUN TEST_USER=${TEST_USER} /usr/bin/bootstrap.sh && \
    rm -rf /var/lib/apt/lists/*


# Support device restart operation
COPY config/system.toml /etc/tedge/
STOPSIGNAL 37

# bootstrap defaults (during containers)
ENV INSTALL=0

CMD ["/lib/systemd/systemd"]
