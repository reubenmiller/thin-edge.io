FROM eclipse-mosquitto:2.0.14

RUN apk update \
    && apk upgrade \
    && apk add --no-cache ca-certificates

COPY ./config/mosquitto.conf /mosquitto/config/

COPY ./bin/tedge /usr/bin/
COPY ./common/configure.sh ./common/init.sh /usr/local/bin/
RUN /usr/local/bin/configure.sh tedge

VOLUME [ "/etc/tedge/mosquitto-conf" ]
VOLUME [ "/device-certs" ]
ENTRYPOINT ["/usr/local/bin/init.sh", "mosquitto", "-c", "/mosquitto/config/mosquitto.conf"]
