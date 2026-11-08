# Cuckoo's Linux image for agent computers: one image for every provider.
# Agents get a browser, files and programs, not a desktop: Chromium runs headless and
# Cuckoo's helper drives it over CDP on 127.0.0.1 only. See
# runtime/agent-computer-providers.md for why each setting is here.
FROM debian:bookworm

RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    ca-certificates curl sudo procps \
    chromium fonts-noto-core fonts-noto-color-emoji \
    python3 python3-pip python3-openpyxl poppler-utils \
    nodejs npm \
 && rm -rf /var/lib/apt/lists/*

# Cuckoo's helper talks to Chromium over CDP with this WebSocket client.
RUN mkdir -p /opt/cuckoo && cd /opt/cuckoo && npm install --omit=dev ws@8 && npm cache clean --force

# Where each request's files live; the agent's home folder is what Cuckoo copies.
RUN mkdir -p /home/user/requests /home/user/shared
