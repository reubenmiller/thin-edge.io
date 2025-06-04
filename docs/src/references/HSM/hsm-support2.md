---
title: Using thin-edge.io with a HSM 2
tags: [Reference, Security]
description: Using a HSM with %%te%%.
---

import UserContext from '@site/src/components/UserContext';
import UserContextForm from '@site/src/components/UserContextForm';

:::tip
#### User Context {#user-context}

You can customize the documentation and commands shown on this page by providing
relevant settings which will be reflected in the instructions. It makes it even
easier to explore and use %%te%%.

<UserContextForm settings="C8Y_PROFILE_NAME,C8Y_PROFILE_URL,C8Y_URL,DEVICE_ID" />

The user context will be persisted in your web browser's local storage.
:::

This section describes how to use %%te%% with a Hardware Security Module (HSM) by way of an PKCS#11 interface.

A Hardware Security Module (HSM) is a physical computing device that safeguards and manages digital keys for strong authentication and provides cryptoprocessing. It is used to protect sensitive data and cryptographic operations by providing a secure, tamper-resistant environment. HSMs are commonly used in applications requiring high levels of security, such as digital signatures, encryption, and key management.

Using certificates for authentication offers several advantages over passwords, especially in the context of device security and IoT:

1.  **Stronger Security:** Certificates use public-key cryptography, which is significantly more secure than password-based authentication. Passwords can be guessed, brute-forced, or compromised through phishing attacks. Certificates rely on cryptographic keys that are much harder to crack.

2.  **No Shared Secrets (for private key):** With certificates, the private key remains on the device (or in the HSM). Only the public key is shared. This means there's no password that needs to be transmitted over the network, reducing the risk of interception.

3.  **Tamper Resistance (with HSM):** When used with an HSM, the private key is stored in a secure, tamper-resistant hardware module. This prevents the private key from being extracted or copied, even if the device's software is compromised. Passwords, even if hashed, are often stored in software and are more vulnerable.

4.  **Automated Management:** Certificate lifecycle management (issuance, renewal, revocation) can be automated, reducing the manual effort and potential for human error associated with managing passwords for a large number of devices.

5.  **Identity Verification:** Certificates provide a verifiable identity for the device, signed by a trusted Certificate Authority (CA). This allows the cloud platform to verify the authenticity of the device connecting to it. Passwords only prove knowledge of a secret, not the identity of the entity possessing it.

6.  **Reduced Attack Surface:** By eliminating the need to store and transmit passwords, the attack surface for credential theft is significantly reduced.

7.  **Compliance:** Many industry regulations and security standards require the use of strong authentication methods like certificates, especially for sensitive data or critical infrastructure.

In summary, using certificates, particularly in conjunction with an HSM, provides a more robust, scalable, and secure method for authenticating devices compared to traditional password-based approaches.

## Overview

%%te%% supports HSM via the PKCS#11 interface (a.k.a. cryptoki) to allow the usage of private keys generated and stored by HSMs to be used when establishing the connection to the cloud.

All HSMs which support the [PKCS#11 interface](https://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/os/pkcs11-base-v2.40-os.html) can be supported, these included:
* Nitrokey HSM 2
* Yubikey 5C (with PIV)
* TPM 2.0
* Arm TrustZone (via OP-TEE)

## Pre-requisites

* the mapper must be configured to use the built-in bridge (e.g. `mqtt.bridge.built_in` should be set to `true`)
* the `tedge` user MUST have access to the HSM (sometimes this involves adding the `tedge` user to a specific group on the device)

## Configuration

### Step 1: Install tedge-p11-server

The **tedge-p11-server** is an optional component which needs to be installed which will provides an interface for %%te%% to use to access a private key stored in a HSM via the PKCS#11 interface.

The package can be installed from the public %%te%% Cloudsmith.io repository using:

```sh
sudo apt-get install tedge-p11-server
```

The package includes a SystemD socket definition, called `tedge-p11-server.socket`. When %%te%% tries to use the unix socket, the `tedge-p11-server.service` will be automatically started by SystemD (this is called [socket activation](https://0pointer.de/blog/projects/socket-activation.html)). You can list the details of the socket by using the following command:

```sh
sudo systemctl status tedge-p11-server.socket
```

You will need to configure the tedge-p11-server with your desired PKCS11 module:

```plain title="file: /etc/tedge/plugins/tedge-p11-server.conf"
TEDGE_DEVICE_CRYPTOKI_MODULE_PATH=/usr/lib/aarch64-linux-gnu/pkcs11/libtpm2_pkcs11.so
TEDGE_DEVICE_CRYPTOKI_PIN=123456
TPM2_PKCS11_STORE="/etc/tedge/device-certs"
```

### Step 2: Set the cryptoki mode

Activate the `cryptoki` mode so that %%te%% knows that it should be using the PKCS#11 (cryptoki) interface to access the device certificate's private key instead of from the file system.

```sh
tedge config set device.cryptoki.mode socket
```

### Step 3: Enable the build-in bridge

Configure %%te%% to use the built-in bridge (instead of the mosquitto bridge):

```sh
tedge config set mqtt.bridge.built_in true
```

### Step 4: Create the private key

Each HSM might have a slightly different way to configure it, so if you having any problems with initializing your HSM, then please consult the manufacturers documentation.

### Step 5: Get a device certificate

A public certificate can either issued by either creating a self-signed certificate and then uploading it to Cumulocity, or by creating a Certificate Signing Request, and sending the request to a Certificate Authority which will then return a valid certificate.

### Step 6: Connect to Cumulocity

Once the HSM have been configured, and you have a device certificate, then you can connect to the cloud:

```sh
tedge connect c8y
```

<UserContext title="Output">

```text
# tedge connect c8y
Connecting to Cumulocity with config:
	device id: $DEVICE_ID
	cloud profile: <none>
	cloud host: $C8Y_URL:8883
	auth type: Certificate
	certificate file: /etc/tedge/device-certs/tedge-certificate.pem
	cryptoki: true
	bridge: built-in
	service manager: systemd
	mosquitto version: 2.0.11
Creating device in Cumulocity cloud... ✓
Restarting mosquitto... ✓
Waiting for mosquitto to be listening for connections... ✓
Enabling tedge-mapper-c8y... ✓
Verifying device is connected to cloud... ✓
Enabling tedge-agent... ✓
```

</UserContext>


## Useful commands

### List tokens on the device

```sh
p11tool --list-tokens
```

## Examples

### Start tedge-p11-server

```sh
tedge config set device.cryptoki.socket_path "/tmp/tedge-p11-server.sock"
tedge-p11-server --module-path "/opt/homebrew/lib/libykcs11.dylib" --socket-path "/tmp/tedge-p11-server.sock"
```

### Enroll the device

```sh
export DEVICE_ID=demo10005
ENROL_TOKEN=$(c8y deviceregistration register-ca --id "$DEVICE_ID" --one-time-password "$(c8y template execute --template '_.Password(31)')" --select password -o csv)
```

### Create new certificate

```sh
/Users/reubenmiller/dev/projects/thin-edge.io/code/tedge-rugpi-core/recipes/setup-pkcs11/files/init-pkcs11.sh --type yubikey --create --token "$ENROL_TOKEN"
```

### Renew certificate

```
/Users/reubenmiller/dev/projects/thin-edge.io/code/tedge-rugpi-core/recipes/setup-pkcs11/files/init-pkcs11.sh --type yubikey --renew
```


## HSM Specific instructions

### SoftHSM2

:::note
SoftHSM2 is a software-based implementation of a Hardware Security Module (HSM) that can be used for testing and development purposes. It should not be used in production as it does not use any hardware to secure private keys.
:::

1. Install SoftHSM2 (to create the token and key) and `p11tool` (to view the [PKCS #11 URI][p11uri]
   of the key).

    ```sh
    sudo apt-get install -y softhsm2 gnutls-bin
    ```

    [p11uri]: https://www.rfc-editor.org/rfc/rfc7512

For SoftHSM configuration, see [SoftHSM README](https://github.com/softhsm/softHSMv2?tab=readme-ov-file#configure-1).

2. Add tedge and current user to `softhsm` group. Only users belonging to `softhsm` group can view
   and manage SoftHSM tokens. After adding your own user, remember to logout and login for changes
   to take effect. Alternatively, you can just run `softhsm2-util` and `p11tool` with `sudo`.

   ```sh
    sudo usermod -a -G softhsm tedge
    sudo usermod -a -G softhsm $(id -un)
   ```

3. Create a new SoftHSM token. You'll be prompted for a PIN for a regular user and security officer
   (SO). The rest of the guide assumes PIN=123456, but you're free to use a different one.

    ```sh
    softhsm2-util --init-token --slot 0 --label my-token
    ```

4. Import the private key to the created token. Make sure to use the correct PIN value for a regular
   user from the previous step.

    ```sh
    PUB_PRIV_KEY=$(
        cat "$(tedge config get device.key_path)" && cat "$(tedge config get device.cert_path)"
    )
    softhsm2-util \
        --import <(echo "$PUB_PRIV_KEY") \
        --token my-token \
        --label my-key \
        --id 01 \
        --pin 123456 \
    ```

5. Get the URI of the key

    First, see what tokens are available

    ```sh
    p11tool --list-tokens
    ```

    ```sh title="Output"
    ...
    Token 2:
        URL: pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token
        Label: my-token
        Type: Generic token
        Flags: RNG, Requires login
        Manufacturer: SoftHSM project
        Model: SoftHSM v2
        Serial: 83f9cf49039c051a
        Module: /usr/lib/x86_64-linux-gnu/softhsm/libsofthsm2.so
    ...
    ```

    Now check if the private key object is in the token. You may need to login, provide the regular
    user PIN and also provide token URL(URI) if multiple tokens are connected:

    ```sh
    p11tool --login --set-pin=123456 --list-privkeys "pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token"
    ```
    ```sh title="Output"
    Object 0:
        URL: pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token;id=%01;object=my-key;type=private
        Type: Private key (EC/ECDSA-SECP256R1)
        Label: my-key
        Flags: CKA_PRIVATE; CKA_SENSITIVE;
        ID: 01
    ```

### Yubikey

1. Make sure the Yubikey is connected

1. Install the [ykman CLI](https://docs.yubico.com/software/yubikey/tools/ykman/)

1. Create a private key along with the associated public key

    ```sh
    ykman piv keys generate --algorithm ECCP256 9a /etc/tedge/device-certs/public.key
    ```

1. Create the device certificate

    **Self-signed**

    If you're using a self-signed certificate, then you can create the certificate directly using:

    <UserContext>

    ```sh
    ykman piv certificates generate \
        --subject "CN=$DEVICE_ID,OU=Test Device,O=Thin Edge" \
        9a /etc/tedge/device-certs/public.key - > "$(tedge config get device.cert_path)"
    ```

    </UserContext>

    Then upload the self-signed certificate to Cumulocity

    ```sh
    tedge cert upload c8y
    ```

    **Certificate Signing Request**

    If you are using a certificate authority, then you will need to create a Certificate Signing Request, and then send it to get signed by the certificate-authority:

    <UserContext>

    ```sh
    ykman piv certificates request \
        --subject "CN=$DEVICE_ID,OU=Test Device,O=Thin Edge" \
        9a public.key - > "$(tedge config get device.csr_path)"
    ```

    </UserContext>

    <UserContext>

    ```sh
    tedge cert renew c8y --csr-path "$(tedge config get device.csr_path)"
    tedge reconnect c8y
    ```

    </UserContext>
