*** Settings ***
Documentation       Update thin-edge.io itself with a software_update command of type "tedge".
...                 The new tedge.exe is the installed one with another version number,
...                 so the agent detects the update and is restarted by the Service Control Manager.

Resource            windows.resource

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown
Test Teardown       Get Logs

Test Tags           platform:windows    theme:software


*** Variables ***
${INSTALL_DIR}      C:\\Program Files\\tedge\\bin
${BACKUP_EXE}       ${TEDGE_DIR}\\tmp\\robot-tedge-original.exe
${UPDATE_EXE}       ${TEDGE_DIR}\\tmp\\robot-tedge-update.exe
${UPDATE_URL}       http://127.0.0.1:8000/te/v1/files/main/robot/tedge.exe


*** Test Cases ***
thin-edge.io is listed as software of type tedge
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/software_list/robot-self-1 '{"status":"init"}'
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///cmd/software_list/robot-self-1
    ...    message_pattern="status":\\s*"successful"
    ...    minimum=1
    Should Match Regexp    ${messages}[-1]    "type":\\s*"tedge"
    Should Contain    ${messages}[-1]    ${CURRENT_VERSION}
    [Teardown]    Run Keywords
    ...    Execute Command    tedge mqtt pub --retain te/device/main///cmd/software_list/robot-self-1 ''
    ...    AND    Get Logs

Update thin-edge.io from a software_update command
    ${pid_before}=    Service Should Be Running    tedge-agent
    Execute Command    curl.exe -sSf -X PUT --data-binary '@${UPDATE_EXE}' '${UPDATE_URL}'
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/software_update/robot-self-2 '{"status":"init","updateList":[{"type":"tedge","modules":[{"name":"tedge","version":"${NEW_VERSION}","url":"${UPDATE_URL}","action":"install"}]}]}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/software_update/robot-self-2
    ...    message_pattern="status":\\s*"successful"
    ...    minimum=1
    ...    timeout=120
    ${installed}=    Execute Command    & '${INSTALL_DIR}\\tedge.exe' --version    strip=${True}
    Should Be Equal    ${installed}    tedge ${NEW_VERSION}
    # The agent exits once it detects the update, and is restarted by the Service Control Manager
    ${pid_after}=    Wait Until Keyword Succeeds    90s    3s    Agent Should Run Another Process    ${pid_before}
    Wait Until Keyword Succeeds    60s    3s    Service Health Status Should Be Up    tedge-agent
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/software_list/robot-self-3 '{"status":"init"}'
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///cmd/software_list/robot-self-3
    ...    message_pattern="status":\\s*"successful"
    ...    minimum=1
    Should Contain    ${messages}[-1]    ${NEW_VERSION}
    [Teardown]    Run Keywords
    ...    Execute Command    tedge mqtt pub --retain te/device/main///cmd/software_update/robot-self-2 ''
    ...    AND    Execute Command    tedge mqtt pub --retain te/device/main///cmd/software_list/robot-self-3 ''
    ...    AND    Get Logs


*** Keywords ***
Custom Setup
    Windows Setup
    Ensure Service Is Running    tedge-agent
    ${output}=    Execute Command    tedge --version    strip=${True}
    ${current_version}=    Fetch From Right    ${output}    ${SPACE}
    # Another version of the same length, so the executable can be patched in place
    ${first}=    Get Substring    ${current_version}    0    1
    ${new_first}=    Set Variable If    '${first}' == '9'    8    9
    ${new_version}=    Catenate    SEPARATOR=    ${new_first}    ${current_version}[1:]
    Set Suite Variable    ${CURRENT_VERSION}    ${current_version}
    Set Suite Variable    ${NEW_VERSION}    ${new_version}
    Execute Command    Copy-Item '${INSTALL_DIR}\\tedge.exe' '${BACKUP_EXE}' -Force
    # Latin-1 maps each byte to one character, so the bytes other than the version are kept as is
    Execute Command
    ...    cmd=$latin1 = [System.Text.Encoding]::GetEncoding(28591); $content = $latin1.GetString([System.IO.File]::ReadAllBytes('${BACKUP_EXE}')); [System.IO.File]::WriteAllBytes('${UPDATE_EXE}', $latin1.GetBytes($content.Replace('${current_version}', '${new_version}')))
    ${patched}=    Execute Command    & '${UPDATE_EXE}' --version    strip=${True}
    Should Be Equal    ${patched}    tedge ${new_version}

Custom Teardown
    # Restore the original tedge.exe for the next suites
    # The running tedge.exe cannot be overwritten: the services running it are stopped, then started again
    Execute Command
    ...    cmd=$running = @(Get-Service tedge* | Where-Object Status -eq Running); $running | Stop-Service -Force; Copy-Item '${BACKUP_EXE}' '${INSTALL_DIR}\\tedge.exe' -Force; $running | Start-Service
    Execute Command
    ...    Remove-Item '${INSTALL_DIR}\\tedge.exe*.old', '${UPDATE_EXE}', '${BACKUP_EXE}' -Force -ErrorAction SilentlyContinue
    ...    ignore_exit_code=${True}
    Ensure Service Is Running    tedge-agent
    Get Suite Logs

Agent Should Run Another Process
    [Arguments]    ${pid_before}
    ${pid}=    Service Should Be Running    tedge-agent
    Should Not Be Equal    ${pid}    ${pid_before}
    RETURN    ${pid}
