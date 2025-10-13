*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:troubleshooting    theme:plugins


*** Variables ***
${PARENT_IP}    ${EMPTY}
${PARENT_SN}    ${EMPTY}


*** Test Cases ***
Run Parameter Update handler as a plugin
    Symlink Should Exist    /etc/tedge/operations/c8y/c8y_ParameterUpdate
    Cumulocity.Should Contain Supported Operations    c8y_ParameterUpdate

    ${operation}=    Cumulocity.Create Operation
    ...    description=Set auto updater parameters
    ...    fragments={"c8y_ParameterUpdate":{},"c8y_ParameterUpdate_AutoUpdater":{},"AutoUpdater":{"enabled":false,"interval":"hourly"}}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}
    Cumulocity.Managed Object Should Have Fragment Values    AutoUpdater.enabled\=false    AutoUpdater.interval\="hourly"


*** Keywords ***
Transfer Configuration Files
    Transfer To Device    ${CURDIR}/c8y_ParameterUpdate.template    /etc/tedge/operations/c8y/
    Transfer To Device    ${CURDIR}/parameter_update.toml    /etc/tedge/operations/
    Transfer To Device    ${CURDIR}/parameter_update.sh    /usr/bin/
    Transfer To Device    ${CURDIR}/AutoUpdater    /usr/share/tedge/parameter-plugins/
    Execute Command    chmod a+x /usr/bin/parameter_update.sh
    Execute Command    chmod a+x /usr/share/tedge/parameter-plugins/AutoUpdater

Custom Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=False
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}

    Set Device Context    ${PARENT_SN}
    Transfer Configuration Files
    Execute Command    tedge config set mqtt.external.bind.address ${PARENT_IP}
    Execute Command    tedge config set mqtt.external.bind.port 1883
    Execute Command    tedge reconnect c8y

    Cumulocity.Device Should Exist    ${PARENT_SN}
