*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             DateTime
Library             ThinEdgeIO
Library             String

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:log

*** Test Cases ***

Custom config_update workflow with post processor
    Cumulocity.Should Support Configurations    custom1    includes=${True}
    ${BINARY_URL}=    Create Inventory Binary    custom1    custom1    file=${CURDIR}/custom1.toml
    ${operation}=    Cumulocity.Set Configuration    typename=custom1    url=${BINARY_URL}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}
    Execute Command    cat


*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Setup LogFiles

Setup LogFiles
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-configuration-plugin.toml    /etc/tedge/plugins/

    # Custom workflow and handler script
    ThinEdgeIO.Transfer To Device    ${CURDIR}/config_update.toml    /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/config_update.sh    /usr/bin/

    ThinEdgeio.Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y
