ARG RUST_VERSION=1.94.1
FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-bookworm AS build
ARG BUILDARCH
ARG TARGETARCH
WORKDIR /build
RUN if [ "$BUILDARCH" != "$TARGETARCH" ]; then \
      apt-get update && \
      case "$TARGETARCH" in \
        amd64) apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu libc6-dev-amd64-cross ;; \
        arm64) apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu libc6-dev-arm64-cross ;; \
        *) exit 1 ;; \
      esac && rm -rf /var/lib/apt/lists/*; \
    fi
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY api ./api
COPY docs ./docs
RUN case "$TARGETARCH" in \
      amd64) target=x86_64-unknown-linux-gnu ;; \
      arm64) target=aarch64-unknown-linux-gnu ;; \
      *) exit 1 ;; \
    esac && rustup target add "$target" && \
    cargo build --release --locked --target "$target" && \
    mkdir /out && cp "target/$target/release/mcp-equidia-rs" /out/mcp-equidia-rs

FROM debian:bookworm-slim
ARG VERSION=0.1.0
ARG REVISION=unknown
ARG SOURCE=https://github.com/fcenedes/mcp-equidia-rs
LABEL org.opencontainers.image.title="mcp-equidia-rs" \
    org.opencontainers.image.description="Public read-only Equidia API MCP server and CLI" \
    org.opencontainers.image.version="${VERSION}" \
    org.opencontainers.image.revision="${REVISION}" \
    org.opencontainers.image.source="${SOURCE}" \
    org.opencontainers.image.licenses="MIT"
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home equidia
COPY --from=build /out/mcp-equidia-rs /usr/local/bin/mcp-equidia-rs
USER 10001:10001
EXPOSE 3000
ENV MCP_HTTP_LISTEN=0.0.0.0:3000
ENTRYPOINT ["mcp-equidia-rs", "--no-env-file"]
CMD ["serve", "http"]
