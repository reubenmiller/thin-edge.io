#!/bin/sh
set -e

TYPE=full
TMPDIR=/tmp/tedge
LOGFILE=/tmp/tedge/install.log
# REPO="tedge-release"
REPO="tedge-dev"

# Set shell used by the script (can be overwritten during dry run mode)
sh_c='sh -c'

usage() {
    cat <<EOF
USAGE:
    $0 [<VERSION>] [--minimal]

ARGUMENTS:
    <VERSION>     Install specific version of thin-edge.io - if not provided installs latest minor release

OPTIONS:
    --minimal   Install only basic set of components - tedge cli and tedge mappers
    --dry-run   Don't install anything, just let me know what it does

EOF
}

log() {
    echo "$@" | tee -a "$LOGFILE"
}

debug() {
    echo "$@" >> "$LOGFILE" 2>&1
}

print_debug() {
    echo
    echo "--------------- machine details ---------------------"
    echo "date:           $(date || true)"
    echo "tedge:          $VERSION"
    echo "Machine:        $(uname -a || true)"
    echo "Architecture:   $(dpkg --print-architecture 2>/dev/null || true)"
    if command_exists "lsb_release"; then
        DISTRIBUTION=$(lsb_release -a 2>/dev/null | grep "Description" | cut -d: -f2- | xargs)
        echo "Distribution:   $DISTRIBUTION"
    fi
    echo
    echo "--------------- error details ------------------------"

    if [ -f "$LOGFILE" ]; then
        cat "$LOGFILE"
    fi

    echo "------------------------------------------------------"
    echo
}

# Enable print of info if something unexpected happens
trap print_debug EXIT

fail() {
    exit_code="$1"
    shift

    log "Failed to install thin-edge.io"
    echo
    log "Reason: $*"
    log "Please create a ticket using the following link and include the console output"
    log "    https://github.com/thin-edge/thin-edge.io/issues/new?assignees=&labels=bug&template=bug_report.md"

    exit "$exit_code"
}

command_exists() {
	command -v "$@" > /dev/null 2>&1
}

is_dry_run() {
	if [ -z "$DRY_RUN" ]; then
		return 1
	else
		return 0
	fi
}

check_prerequisites() {
    if ! command_exists bash; then
        fail 1 "Missing prerequisite: bash"
    fi

    if ! command_exists curl; then
        fail 1 "Missing prerequisite: curl"
    fi
}

configure_shell() {
    # Check if has sudo rights or if it can be requested
    user="$(id -un 2>/dev/null || true)"
    sh_c='sh -c'
    if [ "$user" != 'root' ]; then
        if command_exists sudo; then
            sh_c='sudo -E sh -c'
        elif command_exists su; then
            sh_c='su -c'
        else
            cat >&2 <<-EOF
Error: this installer needs the ability to run commands as root.
We are unable to find either "sudo" or "su" available to make this happen.
EOF
            exit 1
        fi
    fi

    if is_dry_run; then
        sh_c="echo"
    fi
}

install_minimal() {
    if command_exists apt-get; then
        export DEBIAN_FRONTEND=noninteractive
        $sh_c apt-get install -y tedge-minimal
    elif command_exists apk; then
        $sh_c apk add tedge-minimal
    elif command_exists dnf; then
        $sh_c dnf install -y tedge-minimal
    else
        # TODO: Support downloading the archive directly
        fail 1 "TODO: Support downloading the archived directly"
    fi
}

install_full() {
    if command_exists apt-get; then
        export DEBIAN_FRONTEND=noninteractive
        apt-get install -y tedge-full
    elif command_exists apk; then
        apk add tedge-full
    elif command_exists dnf; then
        dnf install -y tedge-full
    else
        # TODO: Support downloading the archive directly
        fail 1 "TODO: Support downloading the archived directly"
    fi
}

main() {
    if [ -d "$TMPDIR" ]; then
        rm -Rf "$TMPDIR"
    fi
    mkdir -p "$TMPDIR"

    check_prerequisites
    configure_shell

    echo "Thank you for trying thin-edge.io!"
    echo

    ARCH=$(uname -m)
    case "$ARCH" in
        *armv6*)
            PACKAGE_REPO="${REPO}-armv6"
            ;;
        *)
            PACKAGE_REPO="${REPO}"
            ;;
    esac

    if command_exists apt-get; then
        script_name="setup.deb.sh"
    elif command_exists apk; then
        script_name="setup.apk.sh"
    elif command_exists dnf; then
        script_name="setup.rpm.sh"
    elif command_exists microdnf; then
        script_name="setup.rpm.sh"
    elif command_exists zypper; then
        script_name="setup.rpm.sh"
    fi
    SETUP_URL="https://dl.cloudsmith.io/public/thinedge/${PACKAGE_REPO}/${script_name}"

    # TODO: Should non-bash systems also be supported?
    curl -1sLf \
        "$SETUP_URL" \
        | sudo -E bash

    case "$TYPE" in
    minimal) install_minimal ;;
    full) install_full ;;
    *)
        log "Unsupported argument type."
        exit 1
        ;;
    esac

    if is_dry_run; then
        echo
        echo "Dry run complete"
    # Test if tedge command is there and working
    elif tedge help >/dev/null 2>&1; then
        # remove error handler
        trap - EXIT

        # Only delete when everything was ok to help with debugging
        rm -Rf "$TMPDIR"

        echo
        echo "thin-edge.io is now installed on your system!"
        echo
        echo "You can go to our documentation to find next steps: https://thin-edge.github.io/thin-edge.io/start/getting-started"
    else
        echo "Something went wrong in the installation process please try the manual installation steps instead:"
        echo "https://thin-edge.github.io/thin-edge.io/install/"
    fi
}

DRY_RUN=${DRY_RUN:-}
VERSION=

if [ $# -lt 4 ]; then
    while :; do
        case $1 in
        --minimal)
            TYPE="minimal"
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        *)
            if [ -z "$1" ]; then
                break
            fi

            if [ -n "$VERSION" ]; then
                break
            fi            

            VERSION="$1"
            
            shift $(( $# > 0 ? 1 : 0 ))
            break
            ;;
        esac
    done
else
    usage
    exit 0
fi

# wrapped up in a function so that we have some protection against only getting
# half the file during "curl | sh"
main
