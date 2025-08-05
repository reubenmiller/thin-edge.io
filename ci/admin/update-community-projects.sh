#!/usr/bin/env bash
set -ex

TMPDIR=$(mktemp -d)
ENABLE_WORKFLOWS=${ENABLE_WORKFLOWS:-1}

PROJECTS=(
    thin-edge/tedge-demo-container
    thin-edge/tedge-rugix-image
    thin-edge/tedge-standalone
    thin-edge/tedge-container-bundle
    thin-edge/tedge-actia-tgur
)

if [ $# -gt 0 ]; then
    echo "Using user-defined project list"
    PROJECTS=()
    while [ $# -gt 0 ]; do
        PROJECTS+=("$1")
    done
fi

run_task_in_project() {
    repo="$1"
    name="$(basename "$repo")"
    gh repo clone "$repo" "$TMPDIR/$name"
    (cd "$TMPDIR/$name" && just release)
}

for project in "${PROJECTS[@]}"; do
    echo "Running task in $project"
    run_task_in_project "$project"
done

# trigger workflows which take care of releases
if [ "$ENABLE_WORKFLOWS" = 1 ]; then
    gh workflow run -R thin-edge/meta-tedge check_updates.yaml
    gh workflow run -R thin-edge/homebrew-tedge check_updates.yaml
fi

rm -rf "$TMPDIR"
