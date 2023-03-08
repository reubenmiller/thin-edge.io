#!/bin/sh

set -e

CMD="$1"
CMD_BAK="$1"
shift

common_init() {
    # FIXME: Check if this can be moved to the image
    mkdir -p /device-certs

    # FIXME: Requires: /etc/ssl/certs to exist, and it fails with only an out of context error reason: 'No such file or directory (os error 2)'
    mkdir -p /etc/ssl/certs
}

#
# Run the initializations required by each component
#
common_init

case "$CMD" in
    bootstrap)
        # FIXME: Currently this must be run as root
        # https://github.com/thin-edge/thin-edge.io/issues/1795
        tedge --init
        tedge-agent --init
        c8y-log-plugin --init
        c8y-configuration-plugin --init
        c8y-firmware-plugin --init

        # Change ownership of all tedge folders
        if [ -d /etc/tedge ]; then
            chown -R tedge:tedge /etc/tedge
        fi

        if [ -d /var/tedge ]; then
            chown -R tedge:tedge /var/tedge
        fi

        if [ -d /var/log/tedge ]; then
            chown -R tedge:tedge /var/log/tedge
        fi

        exit 0
        ;;

    mosquitto)
        # FIXME: Initializations should be handled by the process itself
        tedge --init

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
            touch "/bootstrap/tedge"
        fi

        # TODO: Workout better "waiting mechanism" that other
        # Manually start the mosquitto broker, and check the bridge health?
        while :; do
            if [ ! -f "/bootstrap/tedge" ]; then
                printf "\nWaiting for bootstrapping. Please manually upload the certificate using:\n\n"
                printf "\t* docker compose exec %s sh -c \"tedge cert upload c8y --user '%s' && touch /bootstrap/tedge\"\n\n" "${SERVICE_NAME:-<service_name>}" "${C8Y_USER:-<username>}"
                printf "\t* docker exec -it <container_name> sh -c \"tedge cert upload c8y --user '%s' && touch /bootstrap/tedge\"\n\n" "${C8Y_USER:-<username>}"
            else
                echo "tedge has been bootstrapped :)"
                break
            fi
            sleep 10
        done

        tedge connect c8y ||:
        ;;

    tedge-mapper*)
        CMD="tedge-mapper"
        ;;

    tedge-agent)
        # apt plugin does not make sense for a container, but lets still use it if apt is actually installed
        if command -v apt >/dev/null 2>&1 && command -v tedge-apt-plugin >/dev/null 2>&1; then
            mkdir -p /etc/tedge/sm-plugins
            cp "$(which tedge-apt-plugin)" /etc/tedge/sm-plugins/apt
        fi

        # Restore any other container plugins
        if [ -d /sm-plugins ]; then
            mkdir -p /etc/tedge/sm-plugins
            # TODO: use a symlink, or remove the problem with the shared volume
            find /sm-plugins -type f -exec cp {} /etc/tedge/sm-plugins/ \;
        fi
        ;;

    c8y-*-plugin)
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
