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
        openjdk-21-jdk-headless \
        jq \
        less \
        libssl-dev \
        make \
        patch \
        pkg-config \
        python3 \
        python3-pip \
        python3-venv \
        ripgrep \
        rsync \
        sqlite3 \
        unzip \
        yq \
    && ln -sf /usr/bin/fdfind /usr/local/bin/fd \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @openai/codex pnpm "playwright@${PLAYWRIGHT_VERSION}" \
    && rm -rf /root/.npm

ENV PATH="/usr/local/bin:/usr/bin:/bin"
ENV PLAYWRIGHT_BROWSERS_PATH="/ms-playwright"

CMD ["bash"]
