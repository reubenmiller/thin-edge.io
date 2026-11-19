*** Settings ***
Resource            windows.resource

Suite Setup         Windows Setup
Test Teardown       Get Logs

Test Tags           platform:windows    theme:cli


*** Test Cases ***
Set and get a configuration value
    Execute Command    tedge config set device.type robot-windows-type
    ${value}=    Execute Command    tedge config get device.type    strip=${True}
    Should Be Equal    ${value}    robot-windows-type
    [Teardown]    Run Keywords    Execute Command    tedge config unset device.type    AND    Get Logs

Create a self-signed device certificate
    ${device_id}=    Get Random Name    prefix=robot-win
    Execute Command    tedge cert remove    ignore_exit_code=${True}
    Execute Command    tedge cert create --device-id '${device_id}'
    ${output}=    Execute Command    tedge cert show
    Should Contain    ${output}    ${device_id}
    ${cert_path}=    Execute Command    tedge config get device.cert_path    strip=${True}
    File Should Exist    ${cert_path}
    [Teardown]    Run Keywords    Execute Command    tedge cert remove    ignore_exit_code=${True}    AND    Get Logs

Trusted root certificates default to a thin-edge.io directory
    FOR    ${cloud}    IN    c8y    az    aws
        ${root_cert_path}=    Execute Command    tedge config get ${cloud}.root_cert_path    strip=${True}
        Should Be Equal    ${root_cert_path}    ${TEDGE_DIR}\\ca-certificates
    END
    ${exists}=    Execute Command    Test-Path -PathType Container '${TEDGE_DIR}\\ca-certificates'    strip=${True}
    Should Be Equal    ${exists}    True

PowerShell completes tedge commands
    ${matches}=    Execute Command
    ...    tedge completions powershell | Out-String | Invoke-Expression; (TabExpansion2 'tedge con' 9).CompletionMatches.CompletionText
    Should Contain    ${matches}    config
    Should Contain    ${matches}    connect

Publish a message with tedge mqtt pub
    Execute Command    tedge mqtt pub te/device/main///m/robot_windows '{"temperature": 21.5}'
    Should Have MQTT Messages
    ...    te/device/main///m/robot_windows
    ...    message_contains="temperature": 21.5
    ...    minimum=1
    ...    maximum=1

Subscribe to a retained message with tedge mqtt sub
    Execute Command    tedge mqtt pub --retain robot/windows/retained '{"text":"hello"}'
    ${output}=    Execute Command
    ...    tedge mqtt sub robot/windows/retained --no-topic --duration 1s
    ...    ignore_exit_code=${True}
    ...    strip=${True}
    Should Be Equal    ${output}    {"text":"hello"}
    [Teardown]    Run Keywords    Execute Command    tedge mqtt pub --retain robot/windows/retained ''    AND    Get Logs
