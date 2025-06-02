---
title: Hardware Security Module (HSM) support
tags: [Reference, Security]
description: Hardware Security Module background and overview
draft: true
---

## Background

Before the %%te%% integration is detailed, it is first useful to know some background around the motivation of using a Hardware Security Module (HSM).

In the background, the following questions will be answered:

1. What is a HSM and why use it?

1. What is it used for?

1. What are the different types of HSMs?

### What is a HSM

A Hardware Security Module (HSM) is a dedicated, tamper-resistant hardware device designed to securely store and manage cryptographic keys and perform cryptographic operations on those keys. Think of it as a highly secure vault for your digital secrets and the engine for secure transactions.

### Why is it used?

The main reasons why HSM's are generally a good idea are listed below:

* Secure storage of the private keys
* Prevent copying the private key (an online attacker can not steal the private key)
* Strong key generation (e.g. improved randomness as HSMs generally include a True Random Number Generator (TRNG))

:::note limitations
Technically an attacker could compromise the device, and execute cryptographic operations like "verify", however the attacker can't copy/clone the private key, so the chance of exposing the attacker is much higher.
:::

### What is it used for?

In the context of %%te%%, a HSM is used to:

* Store the private key (related to the client's x509 certificate)
* Provide an API to execute cryptographic operations on the stored private key without exposing the private key. These cryptographic operations are then exposed via a PKCS#11 interface (a.k.a. cryptoki). These operations are typically required during the TLS 1.3 handshake when establishing an MQTT client connection with the cloud

:::note
%%te%% does not take responsibility for the storage of the private key in the HSM as this generally should happen during the commissioning phase, or at the factory.

The GnuTLS toolkit (which includes cli commands such as `p11toll` and `gnutls-certtool`) can be used for initialization of the HSMs, and the creation of the x509 Certificate Lifecycle actions such as the creation of the Certificate Signing Request. Though some HSMs like Yubikey require their own tooling (e.g. `ykman`) as it is not fully PKCS#11 compliant.
:::

### What are the different types of HSMs?

The following lists a few examples of the different types of HSMs out there, though technically any HSM that has support for the PKCS#11 interface can be supported (provided that it also supports the Algorithms / Ciphers required by Cloud you are planning on using).

* TPM 2.0 (Trusted Platform Module)
* Arm TrustZone (used with [OP-TEE](https://optee.readthedocs.io/en/latest/general/about.html))
* USB based devices (e.g. *removable*)
    * Nitrokey HSM 2
    * Yubikey 5C (with PIV)


## Using a HSM with thin-edge.io

### Overview

The following diagram shows an overview of the different components that make up the %%te%% HSM support, however it must be noted that generally devices will only have 1 HSM selected, however any PKCS#11 compliant module can be used with %%te%%. The following are examples of some supported HSMs that have a compatible PKCS#11 interface.

* Yubikey
* Nitrokey HSM 2
* TPM 2.0
* OP-TEE (used to access Arm TrustZone)

<p align="center">
    <img
        src={require('../../references/HSM/tedge-hsm-architecture.drawio.png').default}
        alt="thin-edge.io integration with HSM"
        width="100%"
    />
</p>

The above diagram shows a new component, called **tedge-p11-server** which is a service which provides a unix socket interface which is used by the tedge-mapper when establishing an MQTT client connection to the configured cloud (when certificate based authentication is being used).

:::note
For the technically inclined, the following roughly describes the interaction between the Cloud and %%te%%:

When the **tedge-mapper** is establishing a connection with the MQTT server, the server will ask the client to proof that it is in possession of the certificate's private key (as dictated by the [Certificate Verify](https://datatracker.ietf.org/doc/html/rfc8446#section-4.4.3) phase of the TLS 1.3 handshake). The **tedge-mapper** will request that the **tedge-p11-server** verify the possession of the private key but requesting to the configured PKCS#11 module to sign the given piece of data. The response (which is a signature), is then returned to the cloud and verified. If the signature does not the given certificate, then the MQTT client is disconnected.
:::

The tedge-mapper will contact the **tedge-p11-server** during the TLS 1.3 handshake when establishing the MQTT connection with the cloud, where the server will check if the client is in possession of the certificate's private key (at the [Certificate Verify](https://datatracker.ietf.org/doc/html/rfc8446#section-4.4.3) stage of the TLS 1.3 handshake).

The diagram below shows how the certificate's private key is not directly accessed when using a HSM, however the certificate's public key is stored in the file system (as it was before). Below shows a diagram showing the differences between using %%te%% with and without a HSM.

<p align="center">
    <img
        src={require('../../references/HSM/tedge-hsm-setups.drawio.png').default}
        alt="thin-edge.io integration with HSM"
        width="100%"
    />
</p>

## Operating a HSM

### Initializing the HSM

* Each HSM can have a slightly different way of initializing the module
    * Some require additional files to be stored on the device (e.g. TPM 2.0)
    * Some need specific users access to be able to interact with it


## Integration Examples

The following section detail the 

### Running natively on the device

```                                                                       
 ┌──────────────────────────────────────────────────────────────────┐  
 │                Running thin-edge.io natively                     │  
 │ ┌─────────────────────┐                                          │  
 │ │                     │                                          │  
 │ │         HSM         │                                          │  
 │ │                     │                                          │  
 │ └─────────────────────┘                                          │  
 │            ▲                                                     │  
 │            │                                                     │  
 │ ┌──────────┴──────────┐                                          │  
 │ │        PKCS11       │                                          │  
 │ │    Dynamic Object   │                                          │  
 │ └─────────────────────┘                                          │  
 │            ▲                                                     │  
 │            │                                                     │  
 │            │                                                     │  
 │ ┌──────────┴──────────┐                  ┌──────────────────┐    │  
 │ │                     │                  │                  │    │  
 │ │  tedge-p11-server   │◄─────────────────┼ tedge-mapper c8y │    │  
 │ │                     │                  │                  │    │  
 │ └─────────────────────┘                  └──────────────────┘    │  
 │                                          ┌──────────────────┐    │  
 │                                          │                  │    │  
 │                                          │    tedge-agent   │    │  
 │                                          │                  │    │  
 │                                          └──────────────────┘    │  
 │                                                                  │  
 └──────────────────────────────────────────────────────────────────┘  
```


### Running in a container

The **tedge-p11-server** typically needs to be run on the host so that it can access the HSM directly. Whilst it might be possible to map in specific devices, generally this requires granting root access to the container which then reduces some of the benefits of running the solution in a container.

```
                                                                          
   ┌──────────────────────────────────────────────────────────────────┐   
   │                Running thin-edge.io in a container               │   
   │ ┌─────────────────────┐                                          │   
   │ │                     │                                          │   
   │ │         HSM         │                                          │   
   │ │                     │                                          │   
   │ └─────────────────────┘                                          │   
   │            ▲                                                     │   
   │            │                                                     │   
   │ ┌──────────┴──────────┐                                          │   
   │ │        PKCS11       │                                          │   
   │ │    Dynamic Object   │                                          │   
   │ └─────────────────────┘                                          │   
   │            ▲                                                     │   
   │            │                           ┌──────────────────────┐  │   
   │            │                           │      container       │  │   
   │ ┌──────────┴──────────┐                │ ┌──────────────────┐ │  │   
   │ │                     │                │ │                  │ │  │   
   │ │  tedge-p11-server   │◄───────────────┼─┼ tedge-mapper c8y │ │  │   
   │ │                     │                │ │                  │ │  │   
   │ └─────────────────────┘                │ └──────────────────┘ │  │   
   │                                        │ ┌──────────────────┐ │  │   
   │                                        │ │                  │ │  │   
   │                                        │ │    tedge-agent   │ │  │   
   │                                        │ │                  │ │  │   
   │                                        │ └──────────────────┘ │  │   
   │                                        └──────────────────────┘  │   
   └──────────────────────────────────────────────────────────────────┘   
```

When running thin-edge.io inside a container, the **tedge-mapper** needs access to the unix socket which is created by the **tedge-p11-server**. This can be simply achieved by mounting the socket's parent folder into the container using a container volume mount.

When deploying thin-edge.io in a container, there are some additional considerations that need to be accounted for to enable:

* The **tedge** user and group id (which will be the owner of the unix socket started by the **tedge-p11-server**) MUST use the same User ID (UID) and Group ID (GID) used inside the container, otherwise the **tedge-mapper** running inside the container might not be able to access the socket. It is up to you to decide whether you want to align the host's tedge user/group to the UID/GID used inside the container, or the other way around. Typically the UID/GID can be adjusted by using `usermod` and `groupmod` Linux commands.

* **tedge-p11-server's** unix socket needs to be accessible within the container by using a volume mount, however the socket's directory should be mounted inside the container instead of the unix socket path so that the socket will continue to function if the **tedge-p11-server** service is restarted.

## Tips

### What to consider when selecting a HSM?

* Not all HSMs are the same!
    * Check if compatibility to PKCS#11
    * Check which algorithms are supported (not all algorithms are supported by each device)
* Prefer a HSM which can't be removed (e.g. not USB key if you can help it)
