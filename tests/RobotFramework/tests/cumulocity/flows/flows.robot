*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:troubleshooting    theme:plugins


*** Test Cases ***
Flow service is enabled by default
    ThinEdgeIO.Service Should Be Enabled    tedge-flows
    ThinEdgeIO.Service Should Be Running    tedge-flows

Install a flow for unit conversion
    Install Flow    ${CURDIR}/flows/simple/simple.mjs    ${CURDIR}/flows/simple/simple.toml
    Sleep    1s
    Execute Command    cmd=tedge mqtt pub sensor/raw/temperature '{"temperature": 95}'
    ${measurements}=    Device Should Have Measurements    type=temperature
    Log    ${measurements}

Install a flow to create alerts
    Install Flow    ${CURDIR}/flows/alerts/alerts.mjs    ${CURDIR}/flows/alerts/alerts.toml
    # TODO: How to know when a flow is ready before sending data to it?
    Sleep    4s
    Execute Command    cmd=tedge mqtt pub -q 1 te/device/main///m/temperature '{"temperature": 86}'
    Device Should Have Alarm/s    type=overheat    minimum=1

    Execute Command    cmd=tedge mqtt pub -q 1 te/device/main///m/temperature '{"temperature": 64}'
    Device Should Have Alarm/s    type=overheat    maximum=0    minimum=0    resolved=${False}

*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    # Restarting is only required as the mqtt client credentials might have changed
    Restart Service    tedge-flows

Install Flow
    [Arguments]    ${file}    ${definition}
    Transfer To Device    ${file}    /usr/share/tedge/flows/
    Transfer To Device    ${definition}    /etc/tedge/flows/
