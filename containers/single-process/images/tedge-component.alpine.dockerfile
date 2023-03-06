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
VOLUME [ "/etc/tedge" ]

COPY ./common/configure.sh ./common/init.sh /usr/local/bin/
RUN /usr/local/bin/configure.sh tedge tedge-agent

ENTRYPOINT ["/usr/local/bin/init.sh"]
CMD [ "tedge-agent" ]
