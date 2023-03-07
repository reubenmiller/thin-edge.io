FROM alpine:3.17

# Notes: ca-certificates is required for the initial connection with c8y, otherwise the c8y cert is not trusted
# to test out the connection. But this is only needed for the initial connection, so it seems unnecessary
RUN apk update \
    && apk upgrade \
    && apk add --no-cache \
        ca-certificates \
        sudo \
        docker \
        docker-cli-compose

# Copy all binaries to make the image generic (only if space is not a big deal)
COPY ./bin/* /usr/bin/
VOLUME [ "/etc/tedge" ]
VOLUME [ "/device-certs" ]

COPY ./common/configure.sh ./common/init.sh /usr/local/bin/
# HACK: Initialize the file systems under /etc/tedge however it will be overridden by the later mounted volume
RUN /usr/local/bin/configure.sh tedge tedge-agent c8y-log-plugin c8y-configuration-plugin c8y-firmware-plugin
# USER tedge

ADD https://github.com/reubenmiller/tedge-container-plugin/releases/download/0.1.7/tedge-container-plugin_0.1.7_noarch.apk /tmp/
RUN apk add --allow-untrusted /tmp/tedge-container-plugin_*_noarch.apk \
    # FIXME: Backup the plugin before it gets overwritten by the shared volume
    && mkdir -p /sm-plugins/ \
    && cp /etc/tedge/sm-plugins/* /sm-plugins/

ENTRYPOINT ["/usr/local/bin/init.sh"]
CMD [ "tedge-agent" ]
