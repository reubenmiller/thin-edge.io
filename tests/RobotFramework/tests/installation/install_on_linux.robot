*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO
Library    Collections

Test Tags    theme:installation

*** Variables ***
# Debian
${APT_SETUP}      apt-get update \
    ...           && apt-get install -y sudo curl mosquitto \
    ...           && curl -1sLf https://dl.cloudsmith.io/public/thinedge/tedge-dev/setup.deb.sh | sudo -E bash
${APT_INSTALL}    apt-get install -y tedge-full

# CentOS/RHEL
${DNF_SETUP}      dnf install -y epel-release \
    ...           && dnf install -y sudo mosquitto \
    ...           && curl -1sLf "https://dl.cloudsmith.io/public/thinedge/tedge-dev/setup.rpm.sh" | sudo -E bash
${DNF_INSTALL}    dnf install -y tedge-full

${SUSE_SETUP}     yzpper install -y sudo curl mosquitto \
    ...           && curl -1sLf "https://dl.cloudsmith.io/public/thinedge/tedge-dev/setup.rpm.sh" | sudo -E codename\=42.2 bash
${SUSE_INSTALL}   yzpper install -y tedge-full


# DNF where the epel-release repo is not required
${DNF2_SETUP}     dnf install -y sudo mosquitto \
    ...           && curl -1sLf "https://dl.cloudsmith.io/public/thinedge/tedge-dev/setup.rpm.sh" | sudo -E bash
${DNF2_INSTALL}   dnf install -y tedge-full

# Microdnf
${MDNF_SETUP}     microdnf install -y epel-release \
    ...           && microdnf install -y sudo tar mosquitto \
    ...           && curl -1sLf "https://dl.cloudsmith.io/public/thinedge/tedge-dev/setup.rpm.sh" | sudo -E bash
${MDNF_INSTALL}   microdnf install -y tedge-full

# Alpine linux
${APK_SETUP}      apk add --no-cache sudo curl bash mosquitto \
    ...           && curl -1sLf "https://dl.cloudsmith.io/public/thinedge/tedge-dev/setup.alpine.sh" | sudo -E bash
${APK_INSTALL}    apk add --no-cache tedge-full

# Other linux distributions
${TAR_INSTALL}    curl -O 'https://dl.cloudsmith.io/public/thinedge/tedge-dev/raw/names/tedge/versions/latest/tedge.tar.gz' \
    ...           && tar xzvf tedge.tar.gz -C /usr/bin

*** Test Cases ***

Install on CentOS/RHEL based images
    [Template]    Install using dnf
    rockylinux:9
    almalinux:8

Install on CentOS/RHEL (microdnf) based images
    [Template]    Install using microdnf
    rockylinux:9-minimal

Install on Fedora based images
    [Template]    Install using fedora dnf
    fedora:38
    fedora:37

Install on OpenSUSE based images
    [Template]    Install using yzpper
    opensuse/leap:15
    opensuse/tumbleweed:latest

Install on Debian based images
    [Template]    Install using apt
    debian:10-slim
    debian:11-slim
    ubuntu:20.04
    ubuntu:22.04
    ubuntu:23.04

Install on Alpine based images
    [Template]    Install using apk
    alpine:3.18
    alpine:3.17
    alpine:3.16

Install on any linux distribution
    [Template]    Install using tarball
    alpine:3.18    apk add sudo curl && addgroup -S tedge && adduser -g "" -H -D tedge -G tedge && mkdir -p /run/lock && chmod 1777 /run/lock

*** Keywords ***

Install using dnf
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=        Setup    skip_bootstrap=${True}
    Execute Command      ${DNF_SETUP}
    Execute Command      ${DNF_INSTALL}

Install using microdnf
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=        Setup    skip_bootstrap=${True}
    Execute Command      ${MDNF_SETUP}
    Execute Command      ${MDNF_INSTALL}

Install using fedora dnf
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=        Setup    skip_bootstrap=${True}
    Execute Command      ${DNF2_SETUP}
    Execute Command      ${DNF2_INSTALL}

Install using apt
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=        Setup    skip_bootstrap=${True}
    Execute Command      ${APT_SETUP}
    Execute Command      ${APT_INSTALL}

Install using apk
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=        Setup    skip_bootstrap=${True}
    Execute Command      ${APK_SETUP}    shell=${True}    sudo=${False}
    Execute Command      ${APK_INSTALL}

Install using yzpper
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=        Setup    skip_bootstrap=${True}
    Execute Command      ${SUSE_SETUP}    shell=${True}    sudo=${False}
    Execute Command      ${SUSE_INSTALL}

Install using tarball
    [Arguments]    ${IMAGE}    ${SETUP_STEP}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=        Setup    skip_bootstrap=${True}
    Execute Command      ${SETUP_STEP}    sudo=${False}
    Execute Command      ${TAR_INSTALL}    timeout=1
    Execute Command      timeout 2 tedge-agent || exit 0

Validate thin-edge
    Log    TODO
