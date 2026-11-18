*** Settings ***
Resource            windows.resource
Library             DateTime
Library             OperatingSystem

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown
Test Teardown       Get Logs

Test Tags           platform:windows    theme:plugins


*** Variables ***
${LOG_URL}      http://127.0.0.1:8000/te/v1/files/main/log_upload/robot-win-1


*** Test Cases ***
PowerShell software management plugin is registered without its extension
    # The agent publishes its capabilities shortly after it is up
    Wait Until Keyword Succeeds    30s    1s
    ...    Should Have Retained MQTT Messages
    ...    te/device/main///cmd/software_list
    ...    message_pattern=.*"winlist".*
    Should Not Have Retained MQTT Messages With Text    te/device/main///cmd/software_list    winlist.ps1

Software list uses the PowerShell plugin
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/software_list/robot-win-1 '{"status":"init"}'
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///cmd/software_list/robot-win-1
    ...    message_pattern="status":\\s*"successful"
    ...    minimum=1
    Should Match Regexp    ${messages}[-1]    "type":\\s*"winlist"
    Should Contain    ${messages}[-1]    "robot-windows-package"
    Should Contain    ${messages}[-1]    "1.2.3"
    [Teardown]    Run Keywords
    ...    Execute Command    tedge mqtt pub --retain te/device/main///cmd/software_list/robot-win-1 ''
    ...    AND    Get Logs

Batch log plugin is registered without its extension
    Wait Until Keyword Succeeds    30s    1s
    ...    Should Have Retained MQTT Messages
    ...    te/device/main///cmd/log_upload
    ...    message_pattern=.*"robot_log::robotlog".*

Log upload uses the batch log plugin
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/log_upload/robot-win-1 '{"status":"init","tedgeUrl":"${LOG_URL}","type":"robot_log::robotlog","dateFrom":"2020-01-01T00:00:00Z","dateTo":"2100-01-01T00:00:00Z","lines":10}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/log_upload/robot-win-1
    ...    message_pattern="status":\\s*"successful"
    ...    minimum=1
    ${contents}=    Execute Command    curl.exe -sSf '${LOG_URL}'
    Should Contain    ${contents}    Robot log content
    [Teardown]    Run Keywords
    ...    Execute Command    tedge mqtt pub --retain te/device/main///cmd/log_upload/robot-win-1 ''
    ...    AND    Get Logs

Windows Event Log is offered as a log type
    Wait Until Keyword Succeeds    30s    1s
    ...    Should Have Retained MQTT Messages
    ...    te/device/main///cmd/log_upload
    ...    message_pattern=.*"System::eventlog".*

Log upload of the System event log
    ${date_from}=    Get Current Date    UTC    - 1 day    result_format=%Y-%m-%dT%H:%M:%SZ
    ${date_to}=    Get Current Date    UTC    + 1 hour    result_format=%Y-%m-%dT%H:%M:%SZ
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/log_upload/robot-win-1 '{"status":"init","tedgeUrl":"${LOG_URL}","type":"System::eventlog","dateFrom":"${date_from}","dateTo":"${date_to}","lines":100}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/log_upload/robot-win-1
    ...    message_pattern="status":\\s*"successful"
    ...    minimum=1
    # The tests start and stop services, which the Service Control Manager records in the System log
    ${contents}=    Execute Command    curl.exe -sSf '${LOG_URL}'
    Should Contain    ${contents}    Service Control Manager
    [Teardown]    Run Keywords
    ...    Execute Command    tedge mqtt pub --retain te/device/main///cmd/log_upload/robot-win-1 ''
    ...    AND    Get Logs

Diagnostic collection runs the Windows plugins
    ${output_dir}=    Set Variable    ${TEDGE_DIR}\\tmp\\robot-diag
    Execute Command    tedge diag collect --name robot-diag --keep-dir --output-dir '${output_dir}'
    OperatingSystem.File Should Exist    ${output_dir}\\robot-diag.tar.gz
    OperatingSystem.File Should Exist    ${output_dir}\\robot-diag\\01_tedge\\tedge-config-list.log
    OperatingSystem.File Should Exist    ${output_dir}\\robot-diag\\01_tedge\\tedge-agent.log
    OperatingSystem.File Should Exist    ${output_dir}\\robot-diag\\02_os\\output.log
    OperatingSystem.File Should Exist    ${output_dir}\\robot-diag\\03_mqtt\\tedge-mqtt-sub-retained-only.log
    OperatingSystem.File Should Exist    ${output_dir}\\robot-diag\\07_mosquitto\\mosquitto\\mosquitto.conf
    OperatingSystem.File Should Exist    ${output_dir}\\robot-diag\\08_truststore\\windows-root-certificates.txt
    ${os}=    OperatingSystem.Get File    ${output_dir}\\robot-diag\\02_os\\output.log
    Should Contain    ${os}    Windows
    [Teardown]    Run Keywords
    ...    OperatingSystem.Remove Directory    ${output_dir}    recursive=${True}
    ...    AND    Get Logs


*** Keywords ***
Custom Setup
    Windows Setup
    ThinEdgeIO.Transfer To Device    ${CURDIR}/plugins/winlist.ps1    ${SM_PLUGINS_DIR}\\winlist.ps1
    ThinEdgeIO.Transfer To Device    ${CURDIR}/plugins/robotlog.cmd    ${LOG_PLUGINS_DIR}\\robotlog.cmd
    Restart Agent And Wait Until Up

Custom Teardown
    Remove Device File    ${SM_PLUGINS_DIR}\\winlist.ps1
    Remove Device File    ${LOG_PLUGINS_DIR}\\robotlog.cmd
    Restart Agent And Wait Until Up
    Get Suite Logs

Should Not Have Retained MQTT Messages With Text
    [Arguments]    ${topic}    ${text}
    ${output}=    Execute Command
    ...    tedge mqtt sub '${topic}' --retained-only --no-topic --duration 1s
    ...    ignore_exit_code=${True}
    Should Not Contain    ${output}    ${text}
