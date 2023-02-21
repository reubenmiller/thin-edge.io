#!/bin/bash
set -e

usage() {
    echo "
Convert debian packages to other linux package formats.

The script uses fpm to convert the debian packages to other package formats. Since fpm has a lot of heavy dependencies (ruby, tar, rpmbuild etc),
it is run using a container.

Checkout the website for more details. https://fpm.readthedocs.io/en/latest/index.html

USAGE
    $0 <source_dir> [<output_dir>]

POSITIONAL ARGS
    source_dir       The source directory where to find the debian (.deb) files to convert
    output_dir       Directory where the produced linux packages should be stored. Defaults to the same location as the file

ENVIRONMENT
    CONTAINER_CLI   Set an explicit container cli which should be used to run the container (e.g. docker, podman, nerdctl). By default
                    the container engine will be auto detected. Though the cli must be compatible with the docker cli for it to work.

EXAMPLES
    $0 ./target/armv7-unknown-linux-musleabihf ./output
    # Search for all .deb files and save the converted files to the ./output folder

    "
}

if [ $# -ne 2 ]; then
    echo "Invalid number of arguments provided. Expected 2 position arguments." >&2
    usage
    exit 1
fi

SOURCE_PATH="$1"
OUTPUT_DIR="$2"

# Check which container engine is available (don't try to force any particular engine)
CONTAINER_CLI=${CONTAINER_CLI:-}
CONTAINER_CLI_OPTIONS=(docker podman nerdctl)

for cli in "${CONTAINER_CLI_OPTIONS[@]}"; do
    if command -V "$cli" >/dev/null 2>&1; then
        CONTAINER_CLI="$cli"
        break
    fi
done

if [ -z "$CONTAINER_CLI" ]; then
    echo "Could not find a cli to create a container to run fpm. Please install one of the following and try again. ${CONTAINER_CLI_OPTIONS[*]}"
    exit 1
fi


resolve_dir() {
    echo "$(cd "$1" ; pwd)"
}

get_abs_filename() {
  # $1 : relative filename
  echo "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
}

convert_package() {
    output_type="$1"

    file="$(get_abs_filename "$2")"
    deb_filename="$(basename "$file")"
    input_dir="$(dirname "$file")"

    output_dir=$(resolve_dir "$3")

    # Extract package meta data (from filename)
    PACKAGE_NAME="$(echo "$deb_filename" | cut -d'_' -f1)"
    PACKAGE_VERSION_FULL="$(echo "$deb_filename" | cut -d'_' -f2)"
    PACKAGE_VERSION="$(echo "$deb_filename" | cut -d'_' -f2 | cut -d'-' -f1)"
    PACKAGE_ITERATION="$(echo "$deb_filename" | cut -d'_' -f2 | cut -d'-' -f2- | tr - _)"
    PACKAGE_ARCH="$(echo "$deb_filename" | cut -d'_' -f3 | cut -d'.' -f1)"

    # Tar file requires setting the package name explicitly
    # otherwise it will be called something like "tedge.tar", which will cause
    # naming conflicts between versions and architectures
    OPT_FILENAME=""
    if [ "$output_type" = "tar" ]; then
        OPT_FILENAME="${PACKAGE_NAME}_${PACKAGE_VERSION_FULL}_${PACKAGE_ARCH}.tar"
    fi

    #
    # Run fpm (using docker image). Since this command is expecting a file as input, and
    # producing a file, input/output directories need to be mounted inside the container
    # (hence the annoying path manipulation logic)
    #
    "$CONTAINER_CLI" run -t \
        -v "$input_dir":/input \
        -v "$output_dir":/output \
        ghcr.io/thin-edge/fpm:1.15.1 \
            --input-type deb \
            --output-type "$output_type" \
            --name "$PACKAGE_NAME" \
            --version "$PACKAGE_VERSION" \
            --iteration "$PACKAGE_ITERATION" \
            --package "/output/$OPT_FILENAME" \
            --force \
            "/input/$deb_filename"

    # Compress the tar file if present
    if [ "$output_type" = "tar" ] && [ -f "$OUTDIR/$OPT_FILENAME" ]; then
        gzip "$OUTDIR/$OPT_FILENAME"
    fi
}

mkdir -p "$OUTPUT_DIR"

find "${SOURCE_PATH}" -name "*.deb" -print0 | while read -r -d $'\0' file
do
    echo
    echo "Processing package: $file"

    # apk will be supported once init.d/open-rc is supported
    # convert_package apk "$file" "$OUTPUT_DIR"
    convert_package rpm "$file" "$OUTPUT_DIR"
    convert_package tar "$file" "$OUTPUT_DIR"
done
