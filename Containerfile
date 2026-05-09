ARG PLAYWRIGHT_VERSION=1.59.1
ARG TEMURIN_MAJOR=26
FROM mcr.microsoft.com/playwright:v${PLAYWRIGHT_VERSION}-noble

ARG DEBIAN_FRONTEND=noninteractive
ARG PLAYWRIGHT_VERSION=1.59.1
ARG TEMURIN_MAJOR

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

RUN curl -fsSL -o /tmp/temurin.tar.gz \
        "https://api.adoptium.net/v3/binary/latest/${TEMURIN_MAJOR}/ga/linux/x64/jdk/hotspot/normal/eclipse?project=jdk" \
    && mkdir -p /opt/temurin /usr/lib/jvm \
    && tar -xzf /tmp/temurin.tar.gz -C /opt/temurin --strip-components=1 \
    && ln -s /opt/temurin "/usr/lib/jvm/java-${TEMURIN_MAJOR}-openjdk" \
    && ln -s /opt/temurin /usr/lib/jvm/java-latest-openjdk \
    && rm -f /tmp/temurin.tar.gz

RUN npm install -g @openai/codex pnpm "playwright@${PLAYWRIGHT_VERSION}" \
    && go install github.com/a-h/templ/cmd/templ@latest \
    && install -m 0755 /root/go/bin/templ /usr/local/bin/templ \
    && rm -rf /root/.cache/go-build /root/go/pkg/mod/cache /root/.npm

RUN pip3 install --break-system-packages --no-cache-dir gallery-dl

COPY target/release/agent-sandbox /usr/local/bin/agent-sandbox
COPY bin/wrappers/agent-open-url /usr/local/bin/agent-open-url
COPY bin/wrappers/systemctl /usr/local/bin/systemctl
COPY bin/wrappers/journalctl /usr/local/bin/journalctl
RUN chmod 0755 /usr/local/bin/agent-sandbox /usr/local/bin/agent-open-url /usr/local/bin/systemctl /usr/local/bin/journalctl \
    && ln -sf /usr/local/bin/agent-sandbox /usr/local/bin/agent-host \
    && ln -sf /usr/local/bin/agent-open-url /usr/local/bin/xdg-open \
    && ln -sf /usr/local/bin/agent-open-url /usr/local/bin/sensible-browser \
    && ln -sf /usr/local/bin/agent-open-url /usr/local/bin/x-www-browser

ENV JAVA_HOME="/usr/lib/jvm/java-${TEMURIN_MAJOR}-openjdk"
ENV PATH="${JAVA_HOME}/bin:/usr/local/bin:/usr/bin:/bin"
ENV NODE_PATH="/usr/lib/node_modules"
ENV PLAYWRIGHT_BROWSERS_PATH="/ms-playwright"
ENV BROWSER="/usr/local/bin/agent-open-url"

CMD ["bash"]
