#!/bin/bash
#
# Configure python virtual environment
# * Add workspace path to site-packages so the roo
#
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
pushd "$SCRIPT_DIR/.." >/dev/null || exit 1

python3 -m venv env

# shellcheck source=/dev/null
source env/bin/activate

python3 -m pip install -r requirements.txt
python3 -m pip install -r requirements-dev.txt


SITE_LIB=$(python3 -c 'import sysconfig; print(sysconfig.get_paths()["purelib"])')
PROJECT_DIR=$( cd --  "$SCRIPT_DIR/../../../" && pwd )

echo "Creating .pth file for workspace: $SITE_LIB/workspace.pth"
# Add one path per line
cat << EOT > "$SITE_LIB/workspace.pth"
$PROJECT_DIR/tests/RobotFramework
$PROJECT_DIR/tests/RobotFramework/libraries
EOT

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
    echo "Creating the .env file from the template"
    cp "$DOTENV_TEMPLATE" "$DOTENV_FILE"
    show_dotenv_help
elif grep -v "# Testing" "$DOTENV_FILE" >/dev/null; then
    echo "Adding required Testing variables to your existing .env file"
    cat "$DOTENV_TEMPLATE" >> "$DOTENV_FILE"
    show_dotenv_help
else
    echo "Your .env file already containes the '# Testing' section"
    echo "If test tests are still not working then check for any newly added settings in the template .env file"
    echo
    echo "  $DOTENV_TEMPLATE"
    echo
fi

popd >/dev/null || exit 1
