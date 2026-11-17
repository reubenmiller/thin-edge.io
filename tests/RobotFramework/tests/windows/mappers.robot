*** Settings ***
Documentation       Mappers on Windows which do not need cloud credentials:
...                 the flows engine and the user-defined `local` mapper running as a Windows Service.

Resource            windows.resource

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown
Test Teardown       Get Logs

Test Tags           platform:windows    theme:flows


*** Variables ***
${FLOWS_DIR}        ${TEDGE_DIR}\\mappers\\local\\flows
${FLOW_DIR}         ${FLOWS_DIR}\\greeting
${HEALTH_TOPIC}     te/device/main/service/tedge-mapper-local/status/health
${STATUS_TOPIC}     te/device/main/service/tedge-mapper-local/status/flows


*** Test Cases ***
Flows translate a measurement to Cumulocity JSON
    ${output}=    Execute Command
    ...    tedge flows test te/device/main///m/environment '{"time":"2025-06-27T08:11:05.301Z","temperature":258,"location":{"latitude":32.54,"longitude":-117.67}}'
    ...    strip=${True}
    Should Be Equal
    ...    ${output}
    ...    [c8y/measurement/measurements/create] {"type":"environment","time":"2025-06-27T08:11:05.301Z","temperature":{"temperature":{"value":258}},"location":{"latitude":{"value":32.54},"longitude":{"value":-117.67}}}

Local mapper service runs a user-defined flow
    ${start}=    Get Unix Timestamp
    Start Service    tedge-mapper-local
    Service Should Be Running    tedge-mapper-local
    Should Have MQTT Messages    ${HEALTH_TOPIC}    date_from=${start}    message_pattern="status":\\s*"up"
    Should Have MQTT Messages    ${STATUS_TOPIC}    date_from=${start}    message_pattern=.*greeting.*hello[.]toml.*

    Publish And Expect Greeting    Have a nice day!

Local mapper reloads a flow when its parameters change
    Ensure Service Is Running    tedge-mapper-local
    Publish And Expect Greeting    Have a nice day!
    ${start}=    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/greeting/updated-params.toml    ${FLOW_DIR}\\params.toml
    Should Have MQTT Messages    ${STATUS_TOPIC}    date_from=${start}    message_pattern=.*greeting.*
    Wait Until Keyword Succeeds    20s    2s    Publish And Expect Greeting    Enjoy your day!
    [Teardown]    Run Keywords
    ...    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/greeting/params.toml    ${FLOW_DIR}\\params.toml
    ...    AND    Get Logs

Stop local mapper service publishes health down
    Ensure Service Is Running    tedge-mapper-local
    Service Health Status Should Be Up    tedge-mapper-local
    ${start}=    Get Unix Timestamp
    Stop Service    tedge-mapper-local
    Service Should Be Stopped    tedge-mapper-local
    Should Have MQTT Messages    ${HEALTH_TOPIC}    date_from=${start}    message_pattern="status":\\s*"down"
    # The flow no longer runs
    ${start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub hello/in 'after stop'
    Sleep    2s
    Should Not Have MQTT Messages    hello/out    date_from=${start}


*** Keywords ***
Custom Setup
    Windows Setup
    Remove Device File    ${FLOW_DIR}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/greeting/hello.js    ${FLOW_DIR}\\hello.js
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/greeting/hello.toml    ${FLOW_DIR}\\hello.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/greeting/params.toml    ${FLOW_DIR}\\params.toml

Custom Teardown
    ThinEdgeIO.Stop Service    tedge-mapper-local
    Remove Device File    ${FLOW_DIR}
    Get Suite Logs

Publish And Expect Greeting
    [Arguments]    ${greeting}
    ${start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub hello/in 'hi'
    Should Have MQTT Messages    hello/out    date_from=${start}    message_contains=${greeting}    minimum=1
