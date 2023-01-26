#Command to execute:    robot -d \results --timestampoutputs --log c8y_child_alarms_rpi.html --report NONE --variable HOST:192.168.1.120 /thin-edge.io/tests/RobotFramework/child_alarms/c8y_child_alarms_rpi.robot
#IMPORTANT: NO CHILD DEVICE SHOULD EXIST
*** Settings ***

Resource    ../../resources/common.resource
Library    ThinEdgeIO
Library    Cumulocity
Suite Setup            Custom Setup
Suite Teardown         Get Logs


*** Variables ***

${DEVICE_SN}
${CHILD_SN}


*** Test Cases ***

Define Child device 1 ID
    ${name}=    Get Random Name    prefix=${EMPTY}
    Set Suite Variable    $CHILD_SN    ${DEVICE_SN}-${name}-child01

Normal case when the child device does not exist on c8y cloud
    # Device Should Not Exist    ${child_device_name}        # TODO - create a new keyword

    # Sending child alarm
    Execute Command    sudo tedge mqtt pub 'tedge/alarms/critical/temperature_high/${CHILD_SN}' '{ "message": "Temperature is very high", "time": "2021-01-01T05:30:45+00:00" }' -q 2 -r
    # Check Child device creation
    Set Device    ${DEVICE_SN}
    Should Be A Child Device Of Device    ${CHILD_SN}

    # Check created alarm
    Set Device    ${CHILD_SN}
    ${alarms}=    Device Should Have Alarm/s    minimum=1    maximum=1    # Should be the only alarm there
    ${alarms}=    Device Should Have Alarm/s    minimum=1    maximum=1    expected_text=Alarm of type \'temperature_high\' raised    type=temperature_high    severity=CRITICAL
    # TODO:    Check date of the alarm
    # TODO:    Validate the contents of the alarm on the client side, e.g. JSON Comparisons

Normal case when the child device already exists
#Sending child alarm again
    Execute Command    sudo tedge mqtt pub 'tedge/alarms/critical/temperature_high/${CHILD_SN}' '{ "message": "Temperature is very high", "time": "2021-01-02T05:30:45+00:00" }' -q 2 -r

#Check created second alarm
    ${alarms}=    Device Should Have Alarm/s    minimum=1    maximum=1    updated_after=2021-01-02
    ${alarms}=    Device Should Have Alarm/s    minimum=1    maximum=1    expected_text=Alarm of type 'temperature_high' raised    type=temperature_high    severity=CRITICAL    updated_after=2021-01-02
    # TODO: Compare json structure to check that that the count/occurance is set to two, and not that there are two different alarms
    # Should Contain    ${alarm}    CRITICAL
    # Should Contain    ${alarm}    2 Alarm of type 'temperature_high' raised
    # Should Contain    ${alarm}    2 Jan 2021, 06:30:45
    # Should Contain    ${alarm}    ${child_device_name}

Reconciliation when the new alarm message arrives, restart the mapper
    Execute Command    sudo systemctl stop tedge-mapper-c8y.service
    Execute Command    sudo tedge mqtt pub 'tedge/alarms/critical/temperature_high/${CHILD_SN}' '{ "message": "Temperature is very high", "time": "2021-01-03T05:30:45+00:00" }' -q 2 -r
    Execute Command    sudo systemctl start tedge-mapper-c8y.service

    # Check created second alarm
    ${alarms}=    Device Should Have Alarm/s    minimum=1    maximum=1    updated_after=2021-01-03
    ${alarms}=    Device Should Have Alarm/s    minimum=1    maximum=1    expected_text=.*Alarm of type 'temperature_high' raised    type=temperature_high    severity=CRITICAL    updated_after=2021-01-03
    # Should Contain    ${alarms}    CRITICAL
    # Should Contain    ${alarms}    Alarm of type 'temperature_high' raised
    # Should Contain    ${alarms}    Jan 2021, 06:30:45
    # Should Contain    ${alarms}    ${child_device_name}

Reconciliation when the alarm that is cleared
    Execute Command    sudo systemctl stop tedge-mapper-c8y.service
    Execute Command    sudo tedge mqtt pub 'tedge/alarms/critical/temperature_high/${CHILD_SN}' '' -q 2 -r
    Execute Command    sudo systemctl start tedge-mapper-c8y.service
    Device Should Not Have Alarm/s


*** Keywords ***

Custom Setup
    ${device_sn}=    Setup
    Set Suite Variable    $DEVICE_SN    ${device_sn}