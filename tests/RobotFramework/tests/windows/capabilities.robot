*** Settings ***
Resource            windows.resource

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           platform:windows    theme:capabilities


*** Test Cases ***
shell_execute is not supported on Windows
    ${date_from}=    Get Unix Timestamp
    Restart Agent And Wait Until Up
    # The agent publishes its capabilities on startup
    Should Have MQTT Messages    te/device/main///cmd/log_upload    date_from=${date_from}
    Should Not Have MQTT Messages    te/device/main///cmd/shell_execute    date_from=${date_from}
    Should Not Have Retained MQTT Messages    te/device/main///cmd/shell_execute


*** Keywords ***
Custom Setup
    Windows Setup
    Ensure Service Is Running    tedge-agent
