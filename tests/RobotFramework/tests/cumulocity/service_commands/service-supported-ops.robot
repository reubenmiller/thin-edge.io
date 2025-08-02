*** Settings ***
Resource            ../../../resources/common.resource
Library             DateTime
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:operation


*** Test Cases ***
Users can add support for service commands
    # Note: The nginx service is installed as part of the system test image
    ${service_xid}    Register Service    nginx    systemd
    ${config_dir}    Create Agent For Service    device/main/service/nginx
    Transfer To Device    ${CURDIR}/files/service_command.toml    ${config_dir}/operations/
    Cumulocity.Should Contain Supported Operations    c8y_ServiceCommand

    # NOTE: The default set of actions is START, STOP, RESTART, but can be extended
    # Execute Command    tedge mqtt pub --retain te/device/main/service/nginx/twin/c8y_SupportedServiceCommands '["START","STOP","RESTART"]'

    ${pid_before}    Execute Command    sudo systemctl show --property MainPID nginx
    ${operation}    Cumulocity.Create Operation
    ...    {"c8y_ServiceCommand":{"serviceType":"systemd","serviceName":"nginx","command":"RESTART"}}
    ...    description=Restart Service
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}
    ${pid_after}    Execute Command    sudo systemctl show --property MainPID nginx
    Should Not Be Equal    ${pid_before}    ${pid_after}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Transfer To Device    ${CURDIR}/files/tedge-agent@.service    /lib/systemd/system/
    Transfer To Device    ${CURDIR}/files/c8y_ServiceCommand.template    /etc/tedge/operations/c8y/
    Transfer To Device    ${CURDIR}/files/execute-service-action.sh    /usr/bin/
    Execute Command    cmd=chmod 755 /usr/bin/execute-service-action.sh
    Execute Command
    ...    cmd=echo 'tedge ALL = (ALL) NOPASSWD:SETENV: /usr/bin/systemctl start *, /usr/bin/systemctl stop *, /usr/bin/systemctl restart *' > /etc/sudoers.d/tedge-service-ctrl

Register Service
    [Arguments]    ${name}    ${service_type}=service
    ${external_id}    Set Variable    ${DEVICE_SN}:device:main:service:${name}
    Execute Command
    ...    tedge http post /te/v1/entities '{"@topic-id": "device/main/service/${name}","@id":"${external_id}", "@type": "service","name":"${name}", "type":"${service_type}"}'

    Cumulocity.Set Managed Object    ${DEVICE_SN}
    Cumulocity.Should Have Services    name=${name}    service_type=${service_type}    status=up
    External Identity Should Exist    ${external_id}    show_info=${False}
    RETURN    ${external_id}

Create Agent For Service
    # Create a new instance of the tedge-agent, and use it to handle operations for a service
    # using the workflow engine.
    [Arguments]    ${topic_id}
    ${CONFIG_DIR}    Set Variable    /etc/tedge-agents/${topic_id}
    Execute Command    cmd=systemctl start "tedge-agent@$(systemd-escape "${topic_id}")"
    RETURN    ${CONFIG_DIR}
