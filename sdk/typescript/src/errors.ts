/**
 * Typed errors mirroring the engine's documented HTTP envelope (spec 07):
 * every status code is a distinct error type.
 */

/** Base for all client errors. */
export class DeltuError extends Error {
  readonly code: string;

  constructor(message: string, code: string) {
    super(message);
    this.name = new.target.name;
    this.code = code;
  }
}

/** HTTP 400 — `code === "invalid_event"`. */
export class InvalidEventError extends DeltuError {}

/** HTTP 413 — `code === "payload_too_large"`. */
export class PayloadTooLargeError extends DeltuError {}

/** HTTP 429 — `code === "queue_full"` (retryable). */
export class QueueFullError extends DeltuError {}

/** HTTP 503 — `code === "shutting_down"` (retryable). */
export class EngineUnavailableError extends DeltuError {}

/** The engine could not be reached at all. */
export class NetworkError extends DeltuError {
  constructor(message: string) {
    super(message, "network_error");
  }
}

const STATUS_TO_ERROR: Record<number, new (message: string, code: string) => DeltuError> = {
  400: InvalidEventError,
  413: PayloadTooLargeError,
  429: QueueFullError,
  503: EngineUnavailableError,
};

/** Maps a non-2xx response (already JSON-parsed where possible) to the typed error. */
export function errorForResponse(status: number, body: unknown): DeltuError {
  let message = "the engine rejected the request";
  let code = "unknown";
  if (
    typeof body === "object" &&
    body !== null &&
    "error" in body &&
    typeof (body as { error: unknown }).error === "object" &&
    (body as { error: Record<string, unknown> }).error !== null
  ) {
    const error = (body as { error: Record<string, unknown> }).error;
    if (typeof error.message === "string") message = error.message;
    if (typeof error.code === "string") code = error.code;
  }
  const cls = STATUS_TO_ERROR[status] ?? DeltuError;
  return new cls(message, code);
}
