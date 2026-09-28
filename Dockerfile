# Multi-stage build: reproducible release binary → distroless runtime.
# AI features are deliberately excluded from the default image (spec 11:
# feature-gated builds only; an `ai` variant is built on tag request).

FROM --platform=$BUILDPLATFORM rust:1-slim AS build
ARG TARGETARCH
# Install cross-compilation toolchains so rustc runs natively and only cross-compiles
RUN apt-get update && apt-get install -y gcc-aarch64-linux-gnu gcc-x86-64-linux-gnu
RUN rustup target add aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu

# Configure linkers
RUN mkdir -p /.cargo && \
    echo '[target.aarch64-unknown-linux-gnu]\nlinker = "aarch64-linux-gnu-gcc"\n\n[target.x86_64-unknown-linux-gnu]\nlinker = "x86_64-linux-gnu-gcc"' > /.cargo/config.toml

WORKDIR /src
# Cache dependencies: manifest first, then a stub, then real sources.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs && \
    mkdir benchmarks && echo "#[cfg(test)] mod tests {}" > benchmarks/pipeline.rs
COPY src src
COPY benchmarks benchmarks

# Build for the requested architecture
RUN touch src/main.rs src/lib.rs benchmarks/pipeline.rs && \
    if [ "$TARGETARCH" = "arm64" ]; then \
      cargo build --release --target aarch64-unknown-linux-gnu && \
      aarch64-linux-gnu-strip target/aarch64-unknown-linux-gnu/release/deltu && \
      mkdir -p target/release && \
      cp target/aarch64-unknown-linux-gnu/release/deltu target/release/deltu ; \
    else \
      cargo build --release --target x86_64-unknown-linux-gnu && \
      x86_64-linux-gnu-strip target/x86_64-unknown-linux-gnu/release/deltu && \
      mkdir -p target/release && \
      cp target/x86_64-unknown-linux-gnu/release/deltu target/release/deltu ; \
    fi

# Runtime: Debian 13 distroless — MUST match the build stage's Debian
# release so glibc versions line up (rust:1-slim is bookworm→trixie based;
# a mismatch produces: GLIBC_2.38 not found at container start).
FROM gcr.io/distroless/cc-debian13 AS runtime
# Non-root: distroless `nonroot` user (uid 65532).
USER nonroot:nonroot
COPY --from=build /src/target/release/deltu /deltu
# Inside a container the engine must serve on all interfaces — the
# published port maps to the container's own network namespace, and the
# loopback default would reset every host connection. The env override is
# applied by the CLI (DELTU_* variables), keeping the binary's documented
# loopback default for non-container runs.
ENV DELTU_HTTP_BIND=0.0.0.0:8080
# Config comes from a mounted file + env overrides only (spec 14).
HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/deltu", "health", "--url", "http://127.0.0.1:8080"]
EXPOSE 8080
ENTRYPOINT ["/deltu"]
CMD ["run"]
