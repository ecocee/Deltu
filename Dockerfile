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

FROM gcr.io/distroless/cc-debian12 AS runtime
# Non-root: distroless `nonroot` user (uid 65532).
USER nonroot:nonroot
COPY --from=build /src/target/release/deltu /deltu
# Config comes from a mounted file + env overrides only (spec 14).
HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/deltu", "health", "--url", "http://127.0.0.1:8080"]
EXPOSE 8080
ENTRYPOINT ["/deltu"]
CMD ["run"]
