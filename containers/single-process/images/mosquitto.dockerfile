FROM eclipse-mosquitto:2.0.14

RUN apk update \
    && apk upgrade \
    && apk add --no-cache ca-certificates

COPY ./config/mosquitto.conf /mosquitto/config/

COPY ./bin/tedge /usr/bin/
COPY ./common/configure.sh ./common/init.sh /usr/bin/
RUN /usr/bin/configure.sh tedge

# Use healthcheck as a holding pattern to prevent the other
# dependent containers from starting until the bridge has been configured
HEALTHCHECK --interval=5s --timeout=1s --start-period=600s \
  CMD test -e /bootstrap/tedge || exit 1

VOLUME [ "/etc/tedge/mosquitto-conf" ]
VOLUME [ "/device-certs" ]
VOLUME [ "/bootstrap" ]
ENTRYPOINT ["/usr/bin/init.sh", "mosquitto", "-c", "/mosquitto/config/mosquitto.conf"]
