#!/usr/bin/env bash
set -e

TIME_FILTER=(
    "2024-10"
    "2024-11"
    "2024-12"
    "2025-01"
    "2025-02"
)

FORCE_REFRESH=0

if [ "$FORCE_REFRESH" = 1 ] || [ ! -f merge_group_history.json ]; then
    gh run list --workflow build-workflow.yml --event merge_group --limit 500 --json updatedAt,status,conclusion --jq '.[] | select(.status == "completed")' > merge_group_history.json
fi

echo "---------------------------------"
echo "build-workflow.yml History"
echo "---------------------------------"

printf '%s\t%s\t%s\n' "Year/Month" "Pass Rate (%)" "Runs"

for month in "${TIME_FILTER[@]}"; do
    SUCCESS=$(c8y util show --filter "updatedAt like ${month}-*" --filter "conclusion like success" --select conclusion -o csv < merge_group_history.json | wc -l | xargs)
    FAILURE=$(c8y util show --filter "updatedAt like ${month}-*" --filter "conclusion like failure" --select conclusion -o csv < merge_group_history.json | wc -l | xargs)

    TOTAL=$((SUCCESS + FAILURE))

    if [ "$TOTAL" -eq 0 ]; then
        printf '%s\t\t%s%%\t\t%d\n' "$month" "-" "$TOTAL"
    else
        PASS_RATE=$(( SUCCESS * 100 / TOTAL ))
        printf '%s\t\t%s%%\t\t%d\n' "$month" "$PASS_RATE" "$TOTAL"
    fi
done
