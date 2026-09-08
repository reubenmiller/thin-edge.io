*** Settings ***
Documentation       Test thin-edge.io MQTT client authentication using a Hardware Security Module (HSM).
...
...                 This suite focuses on testing selection and connecting to the cloud using different types of private
...                 keys stored in PKCS#11 tokens.
...
...                 Uses SoftHSM2 to simulate a hardware security module for testing purposes. In real production
...                 environments, a dedicated hardware device would be used.

# it would be good to explain here why we use the tedge-p11-server exclusively and not the module mode
Resource            ../pkcs11_common.resource


*** Variables ***
${KEY_URI}      ${EMPTY}


*** Keywords ***
Use Private Key in SoftHSM2 using tedge-p11-server
    Tedge Reconnect Should Succeed

Select Private key using tedge-p11-server URI
    [Documentation]    Make sure that we can select different keys and tokens using a PKCS#11 URI.
    ...    The URI can either point to a specific key, or to a specific token where we will attempt to find a key.
    ...
    ...    To ensure that correct key is selected and reduce the need to generate and upload different keys and
    ...    certificates, we'll only be using one key and we'll only import it to the chosen token, keeping other tokens
    ...    empty.
    ...
    ...    We set the URI on tedge-p11-server, which means that all connecting clients will use the selected key until
    ...    tedge-p11-server is restarted with a different URI.

    Unset tedge-p11-server Uri
    Tedge Reconnect Should Succeed

    # expect failure if we try to use a token that doesn't exist
    Set tedge-p11-server Uri    value=pkcs11:token=asdf
    Tedge Reconnect Should Fail With    Failed to find a signing key: Didn't find a slot to use

    # create tokens with no keys on them, so key selection fails if wrong token is selected
    Execute Command    softhsm2-util --init-token --free --label token1 --pin "123456" --so-pin "123456"

    Set tedge-p11-server Uri    value=pkcs11:token=token1
    Tedge Reconnect Should Fail With    Failed to find a signing key

    Set tedge-p11-server Uri    value=pkcs11:token=tedge
    Tedge Reconnect Should Succeed

    # import another private key to the primary token (one that has valid tedge key) so we can select a key
    Execute Command
    ...    cmd=p11tool --set-pin=123456 --login --generate-privkey ECDSA --curve=secp256r1 --label "key2" "pkcs11:token=tedge"

    Set tedge-p11-server Uri    value=pkcs11:token=tedge;object=key2
    Tedge Reconnect Should Fail With    HandshakeFailure

    # but when URI has correct label, we expect valid key to be used again
    Set tedge-p11-server Uri    value=pkcs11:token=tedge;object=tedge
    Tedge Reconnect Should Succeed
    [Teardown]    Unset tedge-p11-server Uri

Select Private key using a request URI
    [Documentation]    Like above, we select the key using a URI, but this time we include it in a request, which means
    ...    we can select different keys without restarting tedge-p11-server.

    Execute Command    cmd=tedge config set device.key_uri pkcs11:token=token123
    ${stderr}=    Tedge Reconnect Should Fail With    Failed to find a signing key
    Should Contain    ${stderr}    item=cryptoki: socket (key: pkcs11:token=token123)

    Execute Command    cmd=tedge config unset device.key_uri
    Execute Command    cmd=tedge config set device.key_uri pkcs11:token=token123
    ${stderr}=    Tedge Reconnect Should Fail With    Failed to find a signing key
    Should Contain    ${stderr}    item=cryptoki: socket (key: pkcs11:token=token123)

    Execute Command    cmd=tedge config set device.key_uri "pkcs11:token=tedge;object=tedge"
    ${stderr}=    Tedge Reconnect Should Succeed
    Should Contain    ${stderr}    item=cryptoki: socket (key: pkcs11:token=tedge;object=tedge)

Management commands act on the requested URI
    [Documentation]    `device.cryptoki.uri` narrows the tokens a client may sign with, so a signing
    ...    request that names another token does not escape it. The `tedge hsm` commands are instead
    ...    told which token to act on, so the same conflict has to resolve the other way: redirecting
    ...    them to the configured token makes them report one token and act on another.
    ...
    ...    Both settings are pointed at different tokens here so it is unambiguous which one won.

    # a second token, holding no keys, so a command that acted on the wrong one is visible, and a
    # third one that no setting names at all
    Execute Command    softhsm2-util --init-token --free --label spare --pin "123456" --so-pin "123456"
    Execute Command    softhsm2-util --init-token --free --label other --pin "123456" --so-pin "123456"

    Execute Command    cmd=tedge config set device.key_uri "pkcs11:token=tedge;object=tedge"
    Set tedge-p11-server Uri    value=pkcs11:token=spare

    # signing: the server URI still wins, leaving the key on `tedge` out of reach
    Tedge Reconnect Should Fail With    Failed to find a signing key

    # management: the same conflict resolves the other way round
    ${stdout}=    Execute Command    cmd=tedge hsm list-keys "pkcs11:token=tedge"
    Should Contain    ${stdout}    item=Label: tedge

    # and a command given no URI of its own still falls back to the configured token
    ${stderr}=    Execute Command    cmd=tedge hsm list-keys    stdout=${False}    stderr=${True}
    Should Contain    ${stderr}    item=No keys were found on the token

    # a URI naming only an object also lands on the configured token, as the --label flags do
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm list-keys "pkcs11:object=tedge"
    ...    stdout=${False}
    ...    stderr=${True}
    Should Contain    ${stderr}    item=No keys matching the given filters were found on the token

    # create-key looks for an existing key on the requested token, not on the configured one: the
    # key labelled `tedge` exists on `tedge`, so it is reused rather than duplicated
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm create-key --type ecdsa --label tedge "pkcs11:token=tedge"
    ...    stdout=${False}
    ...    stderr=${True}
    Should Contain    ${stderr}    item=already exists on the token; reusing it
    ${stdout}=    Execute Command    cmd=tedge hsm list-keys --label tedge "pkcs11:token=tedge"
    Should Contain X Times    ${stdout}    Private key    1

    # changing the PIN of a token other than the one tedge signs with leaves the configured PIN alone
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm change-pin --new-pin 654321 --current-pin 123456 "pkcs11:token=other"
    ...    stdout=${False}
    ...    stderr=${True}
    Should Contain    ${stderr}    item=was changed
    Should Contain    ${stderr}    item=`device.cryptoki.pin` was left unchanged
    ${pin}=    Execute Command    tedge config get device.cryptoki.pin    strip=${True}
    Should Be Equal    ${pin}    123456
    [Teardown]    Restore Default Key Selection

Restore Default Key Selection
    Execute Command    cmd=tedge config unset device.key_uri
    Unset tedge-p11-server Uri

Connects to C8y using an RSA key
    [Documentation]    Test that we can connect to C8y using an RSA private keys of all sizes.
    [Setup]    Unset tedge-p11-server Uri
    Connect to C8y using new RSA keypair    bits=4096
    Connect to C8y using new RSA keypair    bits=3072
    Connect to C8y using new RSA keypair    bits=2048

Connects to C8y supporting all TLS13 ECDSA signature algorithms
    [Documentation]    Check that we support all ECDSA sigschemes used in TLS1.3, i.e: ecdsa_secp256r1_sha256,
    ...    ecdsa_secp384r1_sha384, ecdsa_secp521r1_sha512.
    [Setup]    Unset tedge-p11-server Uri

    Connect to C8y using new ECDSA keypair    curve=p256
    Connect to C8y using new ECDSA keypair    curve=p384
    # rcgen doesn't support p521
    # https://github.com/rustls/rcgen/issues/60
    # Connect to C8y using new ECDSA keypair    curve=p521

Custom Setup
    [Arguments]    ${tedge_p11_server_version}
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    ${DEVICE_SN}

    # initialize the soft hsm
    Execute Command    sudo /usr/bin/tedge-init-hsm.sh --type softhsm2 --pin 123456
    # tests expect that the device.key_uri is initially unset
    Execute Command    cmd=tedge config unset device.key_uri

    # configure tedge
    Set Cumulocity URLs
    ThinEdgeIO.Register Device With Cumulocity CA    ${DEVICE_SN}

    Install tedge-p11-server    ${tedge_p11_server_version}

    Unset tedge-p11-server Uri
