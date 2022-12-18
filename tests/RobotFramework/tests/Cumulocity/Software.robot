*** Settings ***
Resource    ../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Teardown    Get Logs

*** Test Cases ***
Software list should be populated during startup
    ${DEVICE_SN}=                            Setup
    Device Should Exist                      ${DEVICE_SN}
    Device Should Have Installed Software    tedge

Install software via Cumulocity
    ${DEVICE_SN}=                            Setup
    Device Should Exist                      ${DEVICE_SN}
    ${OPERATION}=    Install Software        c8y-remoteaccess-plugin
    Operation Should Be SUCCESSFUL           ${OPERATION}
    Device Should Have Installed Software    c8y-remoteaccess-plugin
