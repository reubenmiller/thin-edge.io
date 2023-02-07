*** Settings ***
Resource    ../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:telemetry
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


Child devices support sending custom alarms #1699
    [Tags]    \#1699
    Execute Command    tedge mqtt pub tedge/alarms/critical/myCustomAlarmType/${CHILD_SN} '{ "text": "Some test alarm", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${alarms}=    Device Should Have Alarm/s    expected_text=Some test alarm    severity=CRITICAL    minimum=1    maximum=1    type=myCustomAlarmType
    Should Be Equal    ${alarms[0]["someOtherCustomFragment"]["nested"]["value"]}    extra info
    Log    ${alarms}


Child devices support sending events using message fragment instead of text
    [Tags]    MQTT-API-INCONSISTENCY
    Skip    Message fragment is not converted to 'text' for child devices. This is inconsistent with the parent device functionality
    Execute Command    tedge mqtt pub tedge/alarms/minor/parentAlarmType1 '{ "message": "Some test alarm" }'
    Cumulocity.Set Device    ${DEVICE_SN}
    ${alarms}=    Device Should Have Alarm/s    expected_text=Some test alarm    severity=MINOR    minimum=1    maximum=1    type=parentAlarmType1
    Log    ${alarms}


Child devices support sending alarms using message fragment instead of text
    [Tags]    MQTT-API-INCONSISTENCY
    Skip    Message fragment is not converted to 'text' for child devices. This is inconsistent with the parent device functionality
    Execute Command    tedge mqtt pub tedge/alarms/critical/childAlarmType1/${CHILD_SN} '{ "message": "Some test alarm" }'
    ${alarms}=    Device Should Have Alarm/s    expected_text=Some test alarm    severity=CRITICAL    minimum=1    maximum=1    type=childAlarmType1
    Log    ${alarms}


Child devices support sending inventory data via c8y topic
    Execute Command    tedge mqtt pub "c8y/inventory/managedObjects/update/${CHILD_SN}" '{"custom":{"fragment":"yes"}}'
    ${mo}=    Device Should Have Fragments    custom
    Should Be Equal    ${mo["custom"]["fragment"]}    yes


Main device support sending inventory data via c8y topic
    Execute Command    tedge mqtt pub "c8y/inventory/managedObjects/update/${DEVICE_SN}" '{"parentInfo":{"nested":{"name":"complex"}},"type":"customType"}'
    Cumulocity.Set Device    ${DEVICE_SN}
    ${mo}=    Device Should Have Fragments    parentInfo    type
    Should Be Equal    ${mo["parentInfo"]["nested"]["name"]}    complex
    Should Be Equal    ${mo["type"]}    customType

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Set Suite Variable    $CHILD_SN    ${DEVICE_SN}_child1
    Execute Command    mkdir -p /etc/tedge/operations/c8y/${CHILD_SN}
    Restart Service    tedge-mapper-c8y
    Device Should Exist                      ${DEVICE_SN}
    Device Should Exist                      ${CHILD_SN}
