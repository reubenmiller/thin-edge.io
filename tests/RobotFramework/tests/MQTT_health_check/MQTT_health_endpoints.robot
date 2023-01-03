#Command to execute:    robot -d \results --timestampoutputs --log inotify_crate.html --report NONE --variable HOST:192.168.1.130 /thin-edge.io/tests/RobotFramework/MQTT_health_check/MQTT_health_endpoints.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO
Suite Setup       Setup
Suite Teardown    Get Logs


*** Test Cases ***
    
c8y-log-plugin health status
    Execute Command    sudo systemctl start c8y-log-plugin.service
    ${pid}=    Execute Command    pgrep -f "c8y[_-]log[_-]plugin"    strip=True

    Execute Command    sudo tedge mqtt pub 'tedge/health-check/c8y-log-plugin' ''
    @{messages}=    Should Have MQTT Messages    tedge/health/c8y-log-plugin    minimum=1    maximum=1
    Should Contain    @{messages}    "pid":${pid}
    Should Contain    @{messages}    "status":"up"

c8y-configuration-plugin health status
    Execute Command    sudo systemctl start c8y-configuration-plugin.service
    ${pid}=    Execute Command    pgrep -f "c8y[_-]configuration[_-]plugin"    strip=True
   
    Sleep    5s    # TODO: It fails without this! It needs a better way of queing requests
    Execute Command    sudo tedge mqtt pub 'tedge/health-check/c8y-configuration-plugin' ''
    @{messages}=    Should Have MQTT Messages    tedge/health/c8y-configuration-plugin    minimum=1    maximum=1
    Should Contain    @{messages}    "pid":${pid}
    Should Contain    @{messages}    "status":"up"
