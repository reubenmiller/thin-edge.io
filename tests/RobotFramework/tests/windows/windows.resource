*** Settings ***
Documentation       Shared settings and keywords of the Windows suite.
...                 The tests run on the Windows host itself, using the local adapter.

Resource            ../../resources/common.resource
Library             Collections
Library             String
Library             ThinEdgeIO


*** Variables ***
${TEDGE_DIR}            C:\\ProgramData\\tedge
${SM_PLUGINS_DIR}       ${TEDGE_DIR}\\sm-plugins
${LOG_PLUGINS_DIR}      ${TEDGE_DIR}\\log-plugins
${OPERATIONS_DIR}       ${TEDGE_DIR}\\operations


*** Keywords ***
Windows Setup
    [Documentation]    Use the local Windows host as the device under test.
    ...    The Cumulocity registration is done by the suites that need it.
    ${DEVICE_SN}=    ThinEdgeIO.Setup    adapter=local    skip_bootstrap=${True}
    VAR    ${DEVICE_SN}=    ${DEVICE_SN}    scope=SUITE
    RETURN    ${DEVICE_SN}

Restart Agent And Wait Until Up
    ${date_from}=    Get Unix Timestamp
    ThinEdgeIO.Restart Service    tedge-agent
    ThinEdgeIO.Service Should Be Running    tedge-agent
    Should Have MQTT Messages
    ...    te/device/main/service/tedge-agent/status/health
    ...    date_from=${date_from}
    ...    message_pattern="status":\\s*"up"

Ensure Service Is Running
    [Arguments]    ${name}
    ThinEdgeIO.Start Service    ${name}
    ThinEdgeIO.Service Should Be Running    ${name}

Remove Device File
    [Arguments]    ${path}
    Execute Command
    ...    Remove-Item -LiteralPath '${path}' -Force -Recurse -ErrorAction SilentlyContinue
    ...    ignore_exit_code=${True}
