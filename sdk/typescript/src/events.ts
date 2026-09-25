/**
 * Event model mirroring the engine's validation (spec 02) — a cheap
 * pre-flight, not a second validator of record: the server remains
 * authoritative and the SDK surfaces its errors verbatim.
 */

export const MAX_STRUCTURED_DEPTH = 16;

/** Wire payload shapes (tagged with `type`). */
export type WirePayload =
  | { type: "numeric"; value: number }
  | { type: "text"; value: string }
  | { type: "boolean"; value: boolean }
  | { type: "structured"; value: Record<string, unknown> }
  | { type: "null" };

/** One Deltu event. Mirrors `spec 02` field-for-field. */
export class Event {
  readonly id: string;
  readonly source: string;
  readonly kind: string;
  readonly timestamp: number;
  readonly payload: WirePayload;

  constructor(params: {
    id: string;
    source: string;
    kind: string;
    timestamp: number;
    payload: WirePayload;
  }) {
    const { id, source, kind, timestamp, payload } = params;
    if (!id.trim()) throw new EventError("id must not be empty");
    if (!source.trim()) throw new EventError("source must not be empty");
    if (!kind.trim()) throw new EventError("kind must not be empty");
    if (!(timestamp > 0) || !Number.isFinite(timestamp)) {
      throw new EventError(`invalid timestamp: ${timestamp}`);
    }
    validatePayload(payload);
    this.id = id;
    this.source = source;
    this.kind = kind.toLowerCase();
    this.timestamp = timestamp;
    this.payload = payload;
  }

  /** Wire shape (kind normalized, payload tagged). */
  toJSON(): { id: string; source: string; kind: string; timestamp: number; payload: WirePayload } {
    return {
      id: this.id,
      source: this.source,
      kind: this.kind,
      timestamp: this.timestamp,
      payload: this.payload,
    };
  }
}

/** Local validation failure; message text matches the engine's. */
export class EventError extends Error {}

export function text(value: string): WirePayload {
  return { type: "text", value };
}

export function numeric(value: number): WirePayload {
  return { type: "numeric", value };
}

export function boolean(value: boolean): WirePayload {
  return { type: "boolean", value };
}

export function structured(value: Record<string, unknown>): WirePayload {
  return { type: "structured", value };
}

export function nullPayload(): WirePayload {
  return { type: "null" };
}

function validatePayload(payload: WirePayload): void {
  if (typeof payload !== "object" || payload === null || typeof payload.type !== "string") {
    throw new EventError("payload must carry a string `type` tag");
  }
  switch (payload.type) {
    case "numeric":
      if (typeof payload.value !== "number" || !Number.isFinite(payload.value)) {
        throw new EventError("numeric payload requires a finite number value");
      }
      break;
    case "text":
      if (typeof payload.value !== "string" || !payload.value.trim()) {
        throw new EventError("text payload must not be empty or whitespace-only");
      }
      break;
    case "boolean":
      if (typeof payload.value !== "boolean") {
        throw new EventError("boolean payload requires a boolean value");
      }
      break;
    case "structured":
      if (typeof payload.value !== "object" || payload.value === null || Array.isArray(payload.value)) {
        throw new EventError("structured payload requires a JSON object value");
      }
      checkDepth(payload.value, 0);
      break;
    case "null":
      if ("value" in payload && payload.value !== undefined && payload.value !== null) {
        throw new EventError("null payload requires no value");
      }
      break;
    default: {
      const declared: unknown = (payload as { type?: unknown }).type;
      throw new EventError(`unknown payload type: ${String(declared)}`);
    }
  }
}

function checkDepth(value: unknown, depth: number): void {
  if (depth > MAX_STRUCTURED_DEPTH) {
    throw new EventError(`structured payload exceeds maximum depth of ${MAX_STRUCTURED_DEPTH}`);
  }
  if (Array.isArray(value)) {
    for (const child of value) checkDepth(child, depth + 1);
  } else if (typeof value === "object" && value !== null) {
    for (const child of Object.values(value)) checkDepth(child, depth + 1);
  }
}
