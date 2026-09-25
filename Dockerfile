# Multi-stage build: reproducible release binary → distroless runtime.
# AI features are deliberately excluded from the default image (spec 11:
# feature-gated builds only; an `ai` variant is built on tag request).

FROM rust:1-slim AS build
WORKDIR /src
# Cache dependencies: manifest first, then a stub, then real sources.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs && \
    mkdir benchmarks && echo "#[cfg(test)] mod tests {}" > benchmarks/pipeline.rs
COPY src src
COPY benchmarks benchmarks
RUN touch src/main.rs src/lib.rs benchmarks/pipeline.rs && \
    cargo build --release && strip target/release/deltu

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
