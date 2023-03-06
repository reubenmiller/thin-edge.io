#!/bin/sh

set -e

CMD="$1"
CMD_BAK="$1"
shift

set_mqtt_broker() {
    echo "Setting mqtt.client.host to $MQTT_BROKER:${MQTT_BROKER_PORT:-1883}"
    tedge config set mqtt.client.host "$MQTT_BROKER"
    tedge config set mqtt.client.port "${MQTT_BROKER_PORT:-1883}"
}

#
# Run the initializations required by each component
#
case "$CMD" in
    mosquitto)
        # FIXME: Requires: /etc/ssl/certs to exist, and it fails with only an out of context error reason: 'No such file or directory (os error 2)'
        mkdir -p /etc/ssl/certs

        # FIXME: Initializations should be handled by the process itself
        tedge --init

        # FIXME: Why does the mapper care if this is done or not
        tedge config set c8y.url "$C8Y_BASEURL"

        # FIXME: Create separate location for device certificate
        mkdir -p /device-certs
        tedge config set device.cert.path /device-certs/tedge-certificate.pem

        # FIXME: Remove need for the mapper to know about the device id
        if [ -n "$DEVICE_ID" ]; then
            if tedge cert create --device-id "$DEVICE_ID"; then
                echo "Certificate already exists"
            fi
        fi

        if [ -n "$C8Y_USER" ] && [ -n "$C8Y_PASSWORD" ]; then
            C8YPASS="$C8Y_PASSWORD" tedge cert upload c8y --user "$C8Y_USER"

            # FIXIME: Have a better way to know if the cert is trusted or not,
            # e.g. check if the broker has a problem with the cert?
            # Create magic file to tell that the cert is trusted
            # 
            touch "/etc/tedge-bootstrapped"
        fi

        tedge connect c8y ||:

        # TODO: Workout better "waiting mechanism" that other
        # Manually start the mosquitto broker, and check the bridge health?
        # while :; do
        #     if [ ! -f "/etc/tedge-bootstrapped" ]; then
        #         printf "\nWaiting for bootstrapping. Please manually upload the certificate using:\n\n"
        #         printf "\t* docker compose exec <service_name> tedge cert upload c8y --user '<username>' && touch /etc/tedge-bootstrapped\n\n"
        #         printf "\t* docker exec -it <container_name> tedge cert upload c8y --user '<username>' && touch /etc/tedge-bootstrapped\n\n"
        #     else
        #         echo "certificate is readon"
        #         break
        #     fi
        #     sleep 10
        # done

        # FIXME: Should there be an option to use the "listener 1883" rather than specifiying an interface, as in docker
        # this does not really make any sense.
        # HACK: Work out why tedge is setting `listen 1883 127.0.0.1` which means that
        # the mqtt endpoint is not reachable for other containers. Just removing the 127.0.0.1 fixes it
        sed -i 's/^listener .*/listener 1883/g' /etc/tedge/mosquitto-conf/tedge-mosquitto.conf ||:
        ;;

    tedge-mapper*)
        CMD="tedge-mapper"
        set_mqtt_broker

        mkdir -p /device-certs
        tedge config set device.cert.path /device-certs/tedge-certificate.pem

        # FIXME: Why does the mapper care if this is done or not
        tedge config set c8y.url "$C8Y_BASEURL"
        ;;

    tedge-agent)
        set_mqtt_broker

        # FIXME: Default binding should be 0.0.0.0 for containers
        BIND_IP="0.0.0.0"
        echo "Setting mqtt.external.bind_address to $BIND_IP"
        tedge config set mqtt.external.bind_address "$BIND_IP"
        tedge config set mqtt.external.port "1883"


        mkdir -p /device-certs
        tedge config set device.cert.path /device-certs/tedge-certificate.pem

        # CHECK: does a nested folder in the sm-plugins cause problems?
        # mkdir -p /etc/tedge/sm-plugins/apt
        mkdir -p /etc/tedge/sm-plugins

        # apt plugin does not make sense for a container, but lets still use it if apt is actually installed
        if command -v apt >/dev/null 2>&1; then
            cp "$(which tedge-apt-plugin)" /etc/tedge/sm-plugins/apt
        fi

        # Restore any other container plugins
        if [ -d /sm-plugins ]; then
            # TODO: use a symlink, or remove the problem with the shared volume
            find /sm-plugins -type f -exec cp {} /etc/tedge/sm-plugins/ \;
        fi

        ;;

    c8y-*-plugin)
        mkdir -p /etc/ssl/certs
        set_mqtt_broker

        mkdir -p /device-certs
        tedge config set device.cert.path /device-certs/tedge-certificate.pem

        # FIXME: Why do the plugins require this setting?
        tedge config set c8y.url "$C8Y_BASEURL"
        ;;
    *)
        echo "Unknown init command"
        exit 1
        ;;
esac

# Launch binary
FULL_CMD=$(which "$CMD")

# Run initializations, then launch binary, not 100% sure if this is really required though :/
case "$FULL_CMD" in
    *mosquitto*)
        # don't do anything for mosquitto
        ;;
    */tedge-mapper)
        # Mappers are initialized via the mosquitto container
        # as the broker needs to be modified with the bridge connections
        # before the broker starts (because containers don't have restarts ;))
        # Though this requirement might change once https://github.com/eclipse/mosquitto/pull/1926
        # is included in mosquitto (e.g. dynamicalling adding/deleting bridge configuration via $CONTROL/ topic)
        #
        # FIXME: Intialize the mappers as it will created the supported operations for each cloud provider
        case "$CMD_BAK" in
            *c8y)
                echo "Initializing c8y mapper"
                "$CMD" --init c8y
                ;;
            *az)
                echo "Initializing az mapper"
                "$CMD" --init az
                ;;
            *aws)
                echo "Initializing aws mapper"
                "$CMD" --init aws
                ;;
        esac
        ;;
    *)
        echo "Running init (ignoring any errors): $FULL_CMD --init"
        "$FULL_CMD" --init ||:
        ;;
esac

echo "Executing: $FULL_CMD $*"
exec "$FULL_CMD" "$@"
