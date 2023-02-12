#!/bin/bash
#
# Setup the testing environment by configuring python and
# building the test container device images
#

set -e

ADAPTER=

# Only install selected device adapter (to minimize dependencies)
if [ $# -gt 1 ]; then
    ADAPTER="$1"

    case "$ADAPTER" in
        local|docker|ssh)
            echo "Install device test adapter: $ADAPTER"
            ;;
        *)
            echo "Invalid device test adapter. Only 'local', 'docker' or 'ssh' are supported"
            exit 1
            ;;
    esac
fi

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
PROJECT_DIR=$( cd --  "$SCRIPT_DIR/../../../" && pwd )
pushd "$SCRIPT_DIR/.." >/dev/null || exit 1

# Required to prevent dbus errors on raspberry pi
export PYTHON_KEYRING_BACKEND=keyring.backends.null.Keyring

#
# Setup python virtual environment and install dependencies
#
# Add local bin to path
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

# Install dependences (including develop)
if [ ! -d .venv ]; then
    python3 -m venv .venv
fi

echo "Activating virtual environment"
source .venv/bin/activate
pip3 install --upgrade pip

REQUIREMENTS=(
    -r
    "requirements.txt"
    -r
    "requirements.dev.txt"
)
if [ -n "$ADAPTER" ]; then
    REQUIREMENTS+=(
        -r
        "requirements.adapter-${ADAPTER}.txt"
    )
else
    # include all adapters
    REQUIREMENTS+=(
        -r
        "requirements.adapter.txt"
    )
fi


pip3 install "${REQUIREMENTS[@]}"
python3 bin/appendpath.py


#
# Setup dotenv file
#
DOTENV_FILE="$PROJECT_DIR/.env"
DOTENV_TEMPLATE="$PROJECT_DIR/tests/RobotFramework/devdata/env.template"

show_dotenv_help () {
    echo
    echo "Please edit your .env file with the secrets which are required for testing"
    echo
    echo "  $DOTENV_FILE"
    echo
}

if [ ! -f "$DOTENV_FILE" ]; then
    echo
    echo
    echo "Creating the .env file from the template"
    cp "$DOTENV_TEMPLATE" "$DOTENV_FILE"
    show_dotenv_help
elif ! grep "# Testing" "$DOTENV_FILE" >/dev/null; then
    echo
    echo
    echo "Adding required Testing variables to your existing .env file"
    cat "$DOTENV_TEMPLATE" >> "$DOTENV_FILE"
    show_dotenv_help
else
    echo
    echo
    echo "Your .env file already contains the '# Testing' section"
    echo "If test tests are still not working then check for any newly added settings in the template .env file"
    echo
    echo "  Current file:  $DOTENV_FILE"
    echo "  Template file: $DOTENV_TEMPLATE"
    echo
fi

#
# Create a symlink to the local folder due to support
# running via the Robocorp extensions, or running the commands
# manually on the command line
# Note: This is not ideal but it works
#
if [ ! -f .env ]; then
    if [ ! -L .env ]; then
        echo "Creating symlink to project .env file"
        ln -s "$DOTENV_FILE" ".env"
    fi
fi

#
# Build docker images (required for container devices)
#
if ! invoke build >/dev/null 2>&1; then
    echo "Failed to build container image. Please try running 'invoke build' manually to debug the output"
fi

popd >/dev/null || exit 1
