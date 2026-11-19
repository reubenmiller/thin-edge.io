*** Settings ***
Resource            windows.resource

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown
Test Teardown       Get Logs

Test Tags           platform:windows    theme:workflows


*** Variables ***
${COMMAND_TOPIC}    te/device/main///cmd/robot_win_sleep/robot-1
${MARKER}           tedge-robot-wf-timeout


*** Test Cases ***
Workflow script and its child processes are killed on timeout
    ${start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub --retain ${COMMAND_TOPIC} '{"status":"init"}'
    ${messages}=    Should Have MQTT Messages
    ...    ${COMMAND_TOPIC}
    ...    date_from=${start}
    ...    message_pattern="status":\\s*"failed"
    ...    minimum=1
    Should Contain    ${messages}[-1]    timeout
    ${end}=    Get Unix Timestamp
    # The script sleeps for 600s, so the command must fail long before that
    Should Be True    ${end} - ${start} < 60    msg=The workflow timeout was not enforced
    Wait Until Keyword Succeeds    15s    1s    Marked Processes Should Not Be Running
    [Teardown]    Run Keywords
    ...    Execute Command    tedge mqtt pub --retain ${COMMAND_TOPIC} ''
    ...    AND    Get Logs


*** Keywords ***
Custom Setup
    Windows Setup
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/workflows/robot_win_sleep.toml
    ...    ${OPERATIONS_DIR}\\robot_win_sleep.toml
    Restart Agent And Wait Until Up

Custom Teardown
    Remove Device File    ${OPERATIONS_DIR}\\robot_win_sleep.toml
    Execute Command
    ...    Get-CimInstance Win32_Process | Where-Object { $_.ProcessId -ne $PID -and $_.CommandLine -like '*${MARKER}*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    ...    ignore_exit_code=${True}
    Restart Agent And Wait Until Up
    Get Suite Logs

Marked Processes Should Not Be Running
    ${count}=    Execute Command
    ...    @(Get-CimInstance Win32_Process | Where-Object { $_.ProcessId -ne $PID -and $_.CommandLine -like '*${MARKER}*' }).Count
    ...    strip=${True}
    Should Be Equal As Integers    ${count}    0    msg=Processes of the timed out workflow script are still running
