ARG PLAYWRIGHT_VERSION=1.59.1
FROM mcr.microsoft.com/playwright:v${PLAYWRIGHT_VERSION}-noble

ARG DEBIAN_FRONTEND=noninteractive
ARG PLAYWRIGHT_VERSION=1.59.1

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        adb \
        bash \
        ca-certificates \
        cmake \
        curl \
        diffutils \
        fd-find \
        file \
        findutils \
        gcc \
        g++ \
        gh \
        git \
        golang-go \
        iproute2 \
        openjdk-21-jdk-headless \
        jq \
        less \
        libssl-dev \
        make \
        patch \
        p7zip-full \
        pkg-config \
        python3 \
        python3-pip \
        python3-venv \
        ripgrep \
        rsync \
        cargo \
        rustc \
        sqlite3 \
        unzip \
        yq \
        zstd \
    && ln -sf /usr/bin/fdfind /usr/local/bin/fd \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @openai/codex pnpm "playwright@${PLAYWRIGHT_VERSION}" \
    && rm -rf /root/.npm

COPY bin/agent-host /usr/local/bin/agent-host
COPY bin/wrappers/systemctl /usr/local/bin/systemctl
COPY bin/wrappers/journalctl /usr/local/bin/journalctl
RUN chmod 0755 /usr/local/bin/agent-host /usr/local/bin/systemctl /usr/local/bin/journalctl

ENV PATH="/usr/local/bin:/usr/bin:/bin"
ENV PLAYWRIGHT_BROWSERS_PATH="/ms-playwright"

CMD ["bash"]
