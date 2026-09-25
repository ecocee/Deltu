# Changelog

## 0.1.0 — 2026-09-25

Initial release.

- `DeltuClient(url, { apiKey, timeoutMs, retries, backoffMs })` against the
  public v1 API: `sendEvents`, `sendEvent`, `health`, `status`.
- `Event` builder with local pre-flight validation mirroring spec 02.
- Typed errors: `InvalidEventError` (400), `PayloadTooLargeError` (413),
  `QueueFullError` (429), `EngineUnavailableError` (503), `NetworkError`.
- Opt-in retries with exponential backoff for 429/503 only.
- `StatusSnapshot` mirrors the `/v1/status` export (spec 10).
