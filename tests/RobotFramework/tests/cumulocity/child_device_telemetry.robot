*** Settings ***
Resource    ../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***
Child devices support sending simple measurements
    Execute Command    tedge mqtt pub tedge/measurements/${CHILD_SN} '{ "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=ThinEdgeMeasurement    value=temperature    series=temperature
    Log    ${measurements}


Child devices support sending custom measurements
    Execute Command    tedge mqtt pub tedge/measurements/${CHILD_SN} '{ "current": {"L1": 9.5, "L2": 1.3} }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=ThinEdgeMeasurement    value=current    series=L1
    Log    ${measurements}


Child devices support sending custom events
    Execute Command    tedge mqtt pub tedge/events/myCustomType/${CHILD_SN} '{ "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s    expected_text=Some test event    with_attachment=False    minimum=1    maximum=1    type=myCustomType    fragment=someOtherCustomFragment
    Log    ${events}


Child devices support sending custom events overriding the type
    Execute Command    tedge mqtt pub tedge/events/myCustomType/${CHILD_SN} '{"type": "otherType", "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s    expected_text=Some test event    with_attachment=False    minimum=1    maximum=1    type=otherType    fragment=someOtherCustomFragment
    Log    ${events}


Child devices support sending custom alarms
    [Documentation]    Custom fragments are not yet supported. Any custom fragments are silently ignored when creating them in Cumulocity
    Execute Command    tedge mqtt pub tedge/alarms/critical/myCustomAlarmType/${CHILD_SN} '{ "text": "Some test alarm", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    # Execute Command    tedge mqtt pub tedge/alarms/critical/myCustomAlarmType/${CHILD_SN} '{ "text": "Some test alarm", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${alarms}=    Device Should Have Alarm/s    expected_text=Some test alarm    severity=CRITICAL    minimum=1    maximum=1    type=myCustomAlarmType
    Log    ${alarms}


*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Set Suite Variable    $CHILD_SN    ${DEVICE_SN}_child1
    Execute Command    mkdir -p /etc/tedge/operations/c8y/${CHILD_SN}
    Device Should Exist                      ${DEVICE_SN}
    Device Should Exist                      ${CHILD_SN}
