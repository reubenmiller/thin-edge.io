*** Settings ***
Documentation       Cumulocity integration on Windows, using the built-in bridge.
...                 The suite is skipped when the Cumulocity credentials (C8Y_BASEURL) are not set,
...                 for example on forks.

Resource            windows.resource
Library             DateTime
Library             OperatingSystem

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown
Test Teardown       Get Logs

Test Tags           platform:windows    theme:c8y


*** Test Cases ***
Device is registered in Cumulocity
    Cumulocity.Device Should Exist    ${DEVICE_SN}
    Bridge Should Be Up    c8y
    Service Health Status Should Be Up    tedge-mapper-c8y

Restart tedge-mapper-c8y service
    ${pid_before}=    Service Should Be Running    tedge-mapper-c8y
    Restart Service    tedge-mapper-c8y
    ${pid_after}=    Service Should Be Running    tedge-mapper-c8y
    Should Not Be Equal    ${pid_before}    ${pid_after}    msg=tedge-mapper-c8y should have been restarted
    Service Health Status Should Be Up    tedge-mapper-c8y

Stop tedge-mapper-c8y service publishes health down
    ${date_from}=    Get Unix Timestamp
    Stop Service    tedge-mapper-c8y
    Service Should Be Stopped    tedge-mapper-c8y
    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-c8y/status/health
    ...    date_from=${date_from}
    ...    message_pattern="status":\\s*"down"
    Service Health Status Should Be Down    tedge-mapper-c8y
    [Teardown]    Run Keywords
    ...    Ensure Service Is Running    tedge-mapper-c8y
    ...    AND    Service Health Status Should Be Up    tedge-mapper-c8y
    ...    AND    Get Logs

Measurement is sent to Cumulocity
    Execute Command    tedge mqtt pub te/device/main///m/robot_windows '{"temperature": 21.5}'
    Cumulocity.Device Should Have Measurements    type=robot_windows    minimum=1    maximum=1

Software list is sent to Cumulocity
    Cumulocity.Device Should Have Installed Software
    ...    {"softwareType": "winlist", "name": "robot-windows-package", "version": "1.2.3"}

Configuration snapshot
    ${operation}=    Cumulocity.Get Configuration    tedge.toml
    Operation Should Be SUCCESSFUL    ${operation}    timeout=120

Log upload
    Cumulocity.Should Contain Supported Operations    c8y_LogfileRequest
    ${start_timestamp}=    Get Current Date    UTC    -1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"robot_log::robotlog","searchText":"","maximumLines":100}}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    ${event_url_parts}=    Split String    ${operation["c8y_LogfileRequest"]["file"]}    separator=/
    Cumulocity.Event Should Have An Attachment
    ...    ${event_url_parts}[-2]
    ...    expected_pattern=.*Robot log content.*
    ...    encoding=utf-8


*** Keywords ***
Custom Setup
    ${base_url}=    Get Environment Variable    C8Y_BASEURL    default=${EMPTY}
    ${required}=    Get Environment Variable    C8Y_REQUIRED    default=false
    IF    not $base_url.strip() and $required.lower() == 'true'
        Fail    Cumulocity credentials (C8Y_BASEURL) are not set, but the Cumulocity tests are required (C8Y_REQUIRED)
    END
    Skip If    not $base_url.strip()    Cumulocity credentials (C8Y_BASEURL) are not set
    # Imported here, as the library cannot be initialized without the credentials
    Import Library    Cumulocity
    ${DEVICE_SN}=    Windows Setup

    ThinEdgeIO.Transfer To Device    ${CURDIR}/plugins/winlist.ps1    ${SM_PLUGINS_DIR}\\winlist.ps1
    ThinEdgeIO.Transfer To Device    ${CURDIR}/plugins/robotlog.cmd    ${LOG_PLUGINS_DIR}\\robotlog.cmd
    Restart Agent And Wait Until Up

    # Debug logs of the mapper, to analyze failures from the CI logs
    Execute Command
    ...    cmd=if (-not (Select-String -Quiet -Path '${TEDGE_DIR}\\system.toml' -Pattern 'tedge-mapper-c8y' -ErrorAction SilentlyContinue)) { Add-Content -Path '${TEDGE_DIR}\\system.toml' -Value @('', '[log]', 'tedge-mapper-c8y = "debug"') }
    Execute Command    tedge disconnect c8y    ignore_exit_code=${True}
    Execute Command    tedge cert remove    ignore_exit_code=${True}
    Execute Command    tedge config set mqtt.bridge.built_in true
    Register Device With Cumulocity CA    ${DEVICE_SN}
    Connect Mapper    c8y
    Cumulocity.Device Should Exist    ${DEVICE_SN}

Custom Teardown
    ${device_sn}=    Get Variable Value    $DEVICE_SN    ${EMPTY}
    IF    not $device_sn    RETURN
    Execute Command    tedge disconnect c8y    ignore_exit_code=${True}
    Remove Device File    ${SM_PLUGINS_DIR}\\winlist.ps1
    Remove Device File    ${LOG_PLUGINS_DIR}\\robotlog.cmd
    Get Suite Logs
