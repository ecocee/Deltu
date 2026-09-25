# Topic — HTTP

## Question

How should Deltu expose its HTTP API so it is simple, language-agnostic,
validating, and reliable under continuous load?

## Findings

* **Server architecture**: the API is a thin boundary layer over the core
  engine. Handlers should (1) receive the request, (2) validate input,
  (3) call a core interface, (4) return a predictable response. Handlers
  never contain processing logic (invariant 7 of `architecture.md`).
* **Request validation**: validate content type, payload size limits, and
  schema before touching the engine. Rejected requests return structured
  errors — the same error shape every time.
* **JSON serialization**: a single serialization crate used consistently
  across events, config, and API responses avoids schema drift. The event
  API request/response shapes must be documented and versioned when changed.
* **Error responses**: consistent envelope with a machine-readable code and
  a human-readable message; HTTP status codes chosen deliberately
  (400 validation, 404 unknown, 413 too large, 429 rate/queue full,
  5xx internal). Errors never expose sensitive internals.
* **Health endpoints**: a liveness endpoint (`/health`) that answers without
  touching engine internals, plus a status endpoint reporting runtime state.
  Health checks must not themselves create load.
* **Connection handling and timeouts**: explicit connection, header/body
  read, and idle timeouts prevent slow-client resource exhaustion. Keep-alive
  defaults should be reviewed for long-lived edge deployments.
* **Rust HTTP crates**: `axum` (tokio ecosystem, tower middleware, current
  0.8.x line) and `actix-web` are the mainstream choices; `axum`'s
  extractor model maps naturally to validation-at-the-boundary. `hyper`
  directly is lower-level than needed. Decision deferred to the HTTP unit's
  spec (Unit 07); the core engine stays HTTP-free until then.

## Sources

* axum documentation — https://docs.rs/axum
* actix-web documentation — https://actix.rs/
* RFC 9110 (HTTP semantics) — https://httpwg.org/specs/rfc9110.html
* RFC 9112 (HTTP/1.1) — https://httpwg.org/specs/rfc9112.html
* tokio timeouts — https://docs.rs/tokio/latest/tokio/time/

## Impact on Deltu

* The HTTP unit (Unit 07) defines routes, validation rules, error envelope,
  and timeouts in its own spec before implementation; nothing HTTP-related is
  added to the core engine before that.
* Health and status endpoints are designed from the start to be cheap to
  answer, since they will be used by Docker and edge deployments.
* Until Unit 07, no HTTP crate appears in `Cargo.toml`.

## Decision

Deltu exposes a small HTTP/JSON API through a thin handler layer with
validation at the boundary, a single consistent error envelope, explicit
timeouts, and cheap health/status endpoints. The HTTP framework choice
(currently leaning `axum`) is finalized in the Unit 07 specification.
