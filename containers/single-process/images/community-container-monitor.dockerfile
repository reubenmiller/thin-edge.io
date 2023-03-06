FROM alpine:3.17

RUN apk update \
    && apk upgrade \
    && apk add --no-cache \
        mosquitto-clients \
        docker \
        docker-cli-compose

ADD https://github.com/reubenmiller/tedge-container-plugin/releases/download/0.1.7/tedge-container-plugin_0.1.7_noarch.apk /tmp/
RUN apk add --allow-untrusted /tmp/tedge-container-plugin_*_noarch.apk

ENTRYPOINT ["tedge-container-monitor"]
