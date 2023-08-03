FROM fedora:38

# Install
RUN dnf install -y \
    curl \
    gnupg2 \
    sudo \
    ca-certificates \
    systemd \
    openssh \
    openssh-server \
    mosquitto \
    nginx

# Remove unnecessary systemd services
RUN rm -f /lib/systemd/system/multi-user.target.wants/* \
    /etc/systemd/system/*.wants/* \
    /lib/systemd/system/local-fs.target.wants/* \
    /lib/systemd/system/sockets.target.wants/*udev* \
    /lib/systemd/system/sockets.target.wants/*initctl* \
    /lib/systemd/system/sysinit.target.wants/systemd-tmpfiles-setup* \
    /lib/systemd/system/systemd-update-utmp*

# Install base files to help with bootstrapping and common settings
WORKDIR /setup
# COPY files/bootstrap.sh .
# COPY files/system.toml /etc/tedge/
# COPY files/c8y-configuration-plugin.toml /etc/tedge/c8y/
# COPY files/packages/ /setup/packages/

# COPY files/mqtt-logger.service /etc/systemd/system/
# COPY files/mqtt-logger /usr/bin/
# RUN systemctl enable mqtt-logger.service

# Custom mosquitto config
# COPY files/mosquitto.conf /etc/mosquitto/conf.d/
# COPY files/secure-listener.conf .

# Install nginx server to provide some dummy test files (e.g. with speed limiting options)
# COPY files/http-server/nginx.conf /etc/nginx/nginx.conf
# COPY files/http-server/*.sh /usr/bin/
# RUN systemctl disable nginx

# Reference: https://developers.redhat.com/blog/2019/04/24/how-to-run-systemd-in-a-container#enter_podman
# STOPSIGNAL SIGRTMIN+3 (=37)
STOPSIGNAL 37

CMD ["/lib/systemd/systemd"]
