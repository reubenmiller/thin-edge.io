ARG VARIANT="bullseye"
FROM mcr.microsoft.com/devcontainers/rust:1-${VARIANT}

RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
        # apt dependencies
        apt-transport-https ca-certificates curl gnupg-agent software-properties-common lsb-release \
        # Debian packaging utilities
        equivs \
        dpkg-dev \
        fakeroot \
        # Testing dependencies
        docker \
        python3 \
        python3-pip

# Install docker cli
RUN mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null \
    && apt-get update \
    && apt-get install -y docker-ce-cli

# Install cargo packages under non-root user
USER vscode
RUN cargo install cargo-deb --version 1.41.3 \
    && cargo install cross

# Enable cross-rs to compile using docker-in-docker
ENV CROSS_CONTAINER_IN_CONTAINER=true
