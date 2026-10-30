# syntax=docker/dockerfile:1
# One immutable Linux amd64 artifact; API and workers use different runtime secrets.
FROM node:26.2.0-trixie-slim@sha256:1e738cb88890a15c71880323fbc35a739b7bbc703d72e8bfd1613128f8182f78 AS web
WORKDIR /build/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

FROM rust:1.98.0-trixie@sha256:620dbcd124499c59e2406d3741574b5c5838cf9eb9656f0c3a03948f79b02959 AS rust
ARG BUILD_JOBS=2
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY src/ ./src/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --locked --release --bin ayati -j "$BUILD_JOBS" && cp target/release/ayati /ayati

FROM debian:trixie-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad AS goose
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl libstdc++6 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY tools/install-goose.sh ./tools/install-goose.sh
RUN ./tools/install-goose.sh

FROM rust:1.98.0-trixie@sha256:620dbcd124499c59e2406d3741574b5c5838cf9eb9656f0c3a03948f79b02959 AS prepared-goose
ARG BUILD_JOBS=2
RUN apt-get update && apt-get install -y --no-install-recommends python3 git clang pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY tools/build-prepared-goose.py ./tools/build-prepared-goose.py
COPY tools/patches/goose-1.54.0-preparation.patch ./tools/patches/goose-1.54.0-preparation.patch
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/.cache/goose-prepared-build/target \
    python3 tools/build-prepared-goose.py --jobs "$BUILD_JOBS"

FROM debian:trixie-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad AS bridge
RUN apt-get update && apt-get install -y --no-install-recommends python3-venv ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY integrations/aws_bridge/requirements.lock /tmp/bridge-requirements.lock
RUN python3 -m venv /opt/ayati/bridge \
    && /opt/ayati/bridge/bin/pip install --no-cache-dir --disable-pip-version-check \
       -r /tmp/bridge-requirements.lock

FROM debian:trixie-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad AS runtime
# Pass the commit or image tag: --build-arg AYATI_RELEASE=$(git rev-parse --short HEAD)
ARG AYATI_RELEASE=unknown
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libstdc++6 bubblewrap poppler-utils util-linux python3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 ayati \
    && useradd --uid 10001 --gid 10001 --home-dir /var/lib/ayati --no-create-home ayati \
    && mkdir -p /opt/ayati/web /var/lib/ayati \
    && chown 10001:10001 /var/lib/ayati
WORKDIR /opt/ayati
COPY --from=rust /ayati ./ayati
COPY --from=web /build/web/dist ./web/dist
COPY --from=goose /build/.cache/goose/v1.54.0/goose ./goose
COPY --from=prepared-goose /build/.cache/goose/v1.54.0-ayati.1/ ./prepared-goose/
COPY --from=bridge /opt/ayati/bridge ./bridge/
COPY integrations/aws_bridge/ ./integrations/aws_bridge/
COPY --chmod=755 deploy/container-entrypoint.sh ./entrypoint.sh
ENV PYTHONPATH=/opt/ayati/integrations PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1 \
    AYATI_DATA_DIR=/var/lib/ayati AYATI_GOOSE_BIN=/opt/ayati/goose \
    AYATI_PREPARED_GOOSE_BIN=/opt/ayati/prepared-goose/goose \
    AYATI_PORT=8765 AYATI_DOTENV=0 AYATI_LOG_FORMAT=json AYATI_RELEASE=${AYATI_RELEASE}
USER 10001:10001
EXPOSE 8765
ENTRYPOINT ["/opt/ayati/entrypoint.sh"]
CMD ["identity"]
