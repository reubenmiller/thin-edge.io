*** Settings ***
Documentation       Replace the TPM 2.0 key used by tedge-p11-server from another process, like an external
...                 certificate manager renewing the device certificate with a new key, and check that the
...                 running tedge-p11-server uses the new key without being restarted.
...
...                 tpm2-pkcs11 only reads its key store when the module is initialized, so a long-running
...                 tedge-p11-server used to keep signing with the replaced key, and Cumulocity rejected every
...                 TLS handshake with "received fatal alert: HandshakeFailure" until the service was restarted.
...
...                 The TPM is a software TPM (swtpm) behind tpm2-abrmd, which is the default TCTI used by
...                 tpm2-pkcs11 when tpm2-abrmd is installed.

Resource            ./pkcs11_common.resource

Test Setup          Setup TPM2 Device
Test Teardown       Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Variables ***
${TOKEN}                        tedge
${KEY_ID}                       40
${PIN}                          123456
${TPM2_PKCS11_STORE}            /etc/tedge/hsm
# tpm2-pkcs11 tools run outside of tedge-p11-server need to use the same key store
${STORE_ENV}                    env TPM2_PKCS11_STORE=${TPM2_PKCS11_STORE}
# tedge-p11-server reloads the PKCS #11 module when it was loaded longer ago than this
${MODULE_REFRESH_INTERVAL}      31s


*** Test Cases ***
Use a TPM key replaced by another process without restarting tedge-p11-server
    ${pid_before}=    Get tedge-p11-server PID

    Replace TPM Key And Certificate
    Sleep    ${MODULE_REFRESH_INTERVAL}    reason=Let the loaded module age past the refresh interval

    Tedge Reconnect Should Succeed

    ${pid_after}=    Get tedge-p11-server PID
    Should Be Equal    ${pid_after}    ${pid_before}    msg=tedge-p11-server should not have been restarted

Create a CSR for a TPM key replaced by another process
    [Documentation]    A CSR created after the key was replaced must be for the new key, otherwise the renewed
    ...    certificate does not match the key used for the TLS handshakes
    Replace TPM Key
    Sleep    ${MODULE_REFRESH_INTERVAL}    reason=Let the loaded module age past the refresh interval

    Execute Command    cmd=tedge cert create-csr c8y --device-id "${DEVICE_SN}" --output-path /tmp/rotated.csr
    ${csr_key}=    Execute Command    cmd=openssl req -in /tmp/rotated.csr -noout -pubkey    strip=${True}
    ${token_key}=    Get TPM Public Key
    Should Be Equal    ${csr_key}    ${token_key}


*** Keywords ***
Setup TPM2 Device
    ${device_sn}=    Setup    register=${False}
    VAR    ${DEVICE_SN}=    ${device_sn}    scope=TEST

    Execute Command    cmd=systemctl start tpm2-abrmd
    ${module}=    Execute Command    cmd=find /usr/lib -name libtpm2_pkcs11.so | head -n1    strip=${True}
    VAR    ${MODULE}=    ${module}    scope=TEST

    Execute Command    cmd=mkdir -p ${TPM2_PKCS11_STORE} && ${STORE_ENV} tpm2_ptool init
    Execute Command
    ...    cmd=${STORE_ENV} tpm2_ptool addtoken --pid=1 --label=${TOKEN} --userpin=${PIN} --sopin=12345678

    # Use the TPM via tpm2-abrmd (there is no /dev/tpmrm0 in the container)
    Execute Command    cmd=echo 'TPM2_PKCS11_TCTI=tabrmd' >> /etc/tedge/plugins/tedge-p11-server.conf
    Execute Command    cmd=tedge config set device.cryptoki.mode socket
    Execute Command    cmd=tedge config set device.cryptoki.module_path ${MODULE}
    Execute Command    cmd=tedge config set device.cryptoki.pin ${PIN}
    Execute Command    cmd=tedge config set device.key_uri "pkcs11:token=${TOKEN};id=%${KEY_ID}"
    Execute Command    cmd=tedge config set mqtt.bridge.built_in true
    Set Cumulocity URLs

    # The initial key and certificate
    Replace TPM Key And Certificate
    ThinEdgeIO.Restart Service    tedge-p11-server
    Execute Command    cmd=tedge connect c8y

Replace TPM Key And Certificate
    [Documentation]    Replace the key and issue a self-signed certificate for it, using other processes than
    ...    tedge-p11-server (like an external certificate manager would), and trust the certificate in Cumulocity
    Replace TPM Key
    ${cert_path}=    Execute Command    cmd=tedge config get c8y.device.cert_path    strip=${True}
    Execute Command
    ...    cmd=printf 'cn = "${DEVICE_SN}"\\nunit = "Test Device"\\norganization = "Thin Edge"\\nca\\nsigning_key\\ntls_www_client\\n' | ${STORE_ENV} GNUTLS_PIN=${PIN} certtool --generate-self-signed --template /dev/stdin --provider ${MODULE} --load-privkey "pkcs11:token=${TOKEN};id=%${KEY_ID};type=private" --load-pubkey "pkcs11:token=${TOKEN};id=%${KEY_ID};type=public" --outfile "${cert_path}"
    Execute Command    cmd=chown tedge:tedge "${cert_path}"

    ${pem}=    Execute Command    cmd=cat "${cert_path}"
    Cumulocity.Upload Certificate    name=${DEVICE_SN}    pem_cert=${pem}
    ThinEdgeIO.Register Certificate For Cleanup

Replace TPM Key
    [Documentation]    Delete the key and create a new one with the same id and label, without tedge-p11-server
    VAR    ${pkcs11_tool}=
    ...    ${STORE_ENV} pkcs11-tool --module ${MODULE} --token-label ${TOKEN} --login --pin ${PIN}
    Execute Command    cmd=${pkcs11_tool} --delete-object --type privkey --id ${KEY_ID} || true
    Execute Command    cmd=${pkcs11_tool} --delete-object --type pubkey --id ${KEY_ID} || true
    Execute Command    cmd=${pkcs11_tool} --keypairgen --key-type EC:prime256v1 --id ${KEY_ID} --label tedge

Get TPM Public Key
    [Documentation]    Read the public key of the TPM key from a new process, in PEM format
    ${pem}=    Execute Command
    ...    cmd=${STORE_ENV} pkcs11-tool --module ${MODULE} --token-label ${TOKEN} --read-object --type pubkey --id ${KEY_ID} | openssl pkey -pubin -inform DER
    ...    strip=${True}
    RETURN    ${pem}

Get tedge-p11-server PID
    ${pid}=    Execute Command    cmd=systemctl show --property MainPID --value tedge-p11-server    strip=${True}
    Should Not Be Equal    ${pid}    0    msg=tedge-p11-server is not running
    RETURN    ${pid}
