FROM alpine:3.17

# Notes: ca-certificates is required for the initial connection with c8y, otherwise the c8y cert is not trusted
# to test out the connection. But this is only needed for the initial connection, so it seems unnecessary
RUN apk update \
    && apk upgrade \
    && apk add --no-cache \
        ca-certificates \
        sudo

# Copy all binaries to make the image generic (only if space is not a big deal)
COPY ./bin/* /usr/bin/
VOLUME [ "/device-certs" ]

COPY ./common/configure.sh ./common/init.sh /usr/bin/
# HACK: Initialize the file systems under /etc/tedge however it will be overridden by the later mounted volume
RUN /usr/bin/configure.sh tedge tedge-agent c8y-log-plugin c8y-configuration-plugin c8y-firmware-plugin
USER tedge

VOLUME [ "/etc/tedge" ]
VOLUME [ "/var/tedge" ]
VOLUME [ "/var/log/tedge" ]

ENTRYPOINT ["/usr/bin/init.sh"]
CMD [ "tedge-agent" ]
