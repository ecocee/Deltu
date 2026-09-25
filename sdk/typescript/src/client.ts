/**
 * Deltu TypeScript client — a thin fetch-based wrapper around the public
 * v1 API. Zero runtime dependencies (built-in `fetch`); no processing
 * logic lives here (invariant 15: SDKs communicate, never reimplement).
 */

import { errorForResponse, DeltuError, NetworkError } from "./errors.js";
import type { Event } from "./events.js";

export { DeltuError, NetworkError, InvalidEventError, PayloadTooLargeError, QueueFullError, EngineUnavailableError } from "./errors.js";
export { Event, EventError, text, numeric, boolean, structured, nullPayload } from "./events.js";
export type { WirePayload } from "./events.js";

/** Result of `POST /v1/events`. */
export interface BatchResult {
  readonly accepted: boolean[];
  readonly actionsFired: number;
}

/** Result of a single-event send. */
export interface EventResult {
  readonly accepted: boolean;
  readonly actionsFired: number;
}

/** Mirrors the engine's `/v1/status` export field-for-field (spec 10). */
export interface StatusSnapshot {
  readonly version: string;
  readonly uptimeSeconds: number;
  readonly queueDepth: number;
  readonly pipeline: {
    filteredOut: number;
    duplicates: number;
    lateDropped: number;
    windowsEvicted: number;
    changeStatesEvicted: number;
    changesSuppressed: number;
  };
  readonly state: { entries: number; expired: number; evicted: number };
  readonly metrics: Record<string, unknown>;
  readonly raw: Record<string, unknown>;
}

export interface DeltuClientOptions {
  /** Bearer token sent as `authorization` when provided. */
  apiKey?: string;
  /** Request timeout in milliseconds (default 10_000). */
  timeoutMs?: number;
  /** Retries for 429/503 only (default 0 — no silent retries). */
  retries?: number;
  /** Base backoff in milliseconds, exponential (default 200). */
  backoffMs?: number;
}

interface Envelope<T> {
  success: boolean;
  data?: T;
  error?: { code: string; message: string };
}

const RETRYABLE = new Set([429, 503]);

/** Client for a running Deltu engine (`deltu run`). */
export class DeltuClient {
  private readonly baseUrl: string;
  private readonly headers: Record<string, string>;
  private readonly timeoutMs: number;
  private readonly retries: number;
  private readonly backoffMs: number;

  constructor(url: string = "http://127.0.0.1:8080", options: DeltuClientOptions = {}) {
    this.baseUrl = url.replace(/\/+$/, "");
    this.timeoutMs = options.timeoutMs ?? 10_000;
    this.retries = options.retries ?? 0;
    this.backoffMs = options.backoffMs ?? 200;
    this.headers = { accept: "application/json" };
    if (options.apiKey !== undefined) this.headers.authorization = `Bearer ${options.apiKey}`;
  }

  /** `POST /v1/events` with a batch (1..max_batch events). */
  async sendEvents(events: Event[]): Promise<BatchResult> {
    if (events.length === 0) throw new Error("sendEvents requires at least one event");
    const envelope = await this.post<Record<string, unknown>>("/v1/events", {
      events: events.map((event) => event.toJSON()),
    });
    const data = envelope.data ?? {};
    return {
      accepted: Array.isArray(data.accepted) ? data.accepted.map(Boolean) : [],
      actionsFired: Number(data.actions_fired ?? 0),
    };
  }

  /** Single-event convenience over the same endpoint. */
  async sendEvent(event: Event): Promise<EventResult> {
    const batch = await this.sendEvents([event]);
    return {
      accepted: batch.accepted[0] ?? false,
      actionsFired: batch.actionsFired,
    };
  }

  /** `GET /health` — liveness probe. */
  async health(): Promise<boolean> {
    const response = await this.request("/health", "GET");
    if (response.status >= 400) {
      throw errorForResponse(response.status, await safeJson(response));
    }
    return response.status < 400;
  }

  /** `GET /v1/status` — counters snapshot. */
  async status(): Promise<StatusSnapshot> {
    const envelope = await this.get<Record<string, unknown>>("/v1/status");
    const data = envelope.data ?? {};
    const pipeline = (data.pipeline ?? {}) as Record<string, unknown>;
    const state = (data.state ?? {}) as Record<string, unknown>;
    return {
      version: String(data.version ?? ""),
      uptimeSeconds: Number(data.uptime_seconds ?? 0),
      queueDepth: Number(data.queue_depth ?? 0),
      pipeline: {
        filteredOut: Number(pipeline.filtered_out ?? 0),
        duplicates: Number(pipeline.duplicates ?? 0),
        lateDropped: Number(pipeline.late_dropped ?? 0),
        windowsEvicted: Number(pipeline.windows_evicted ?? 0),
        changeStatesEvicted: Number(pipeline.change_states_evicted ?? 0),
        changesSuppressed: Number(pipeline.changes_suppressed ?? 0),
      },
      state: {
        entries: Number(state.entries ?? 0),
        expired: Number(state.expired ?? 0),
        evicted: Number(state.evicted ?? 0),
      },
      metrics: (data.metrics ?? {}) as Record<string, unknown>,
      raw: data,
    };
  }

  // -- transport ---------------------------------------------------------

  private async get<T>(path: string): Promise<Envelope<T>> {
    const response = await this.request(path, "GET");
    return this.checked(response);
  }

  private async post<T>(path: string, body: unknown): Promise<Envelope<T>> {
    let attempt = 0;
    for (;;) {
      const response = await this.request(path, "POST", JSON.stringify(body));
      if (RETRYABLE.has(response.status) && attempt < this.retries) {
        attempt += 1;
        await delay(this.backoffMs * 2 ** (attempt - 1));
        continue;
      }
      return this.checked(response);
    }
  }

  private async request(path: string, method: string, body?: string): Promise<Response> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);
    const init: RequestInit = { method, headers: { ...this.headers }, signal: controller.signal };
    if (body !== undefined) {
      init.body = body;
      init.headers = { ...this.headers, "content-type": "application/json" };
    }
    try {
      return await fetch(`${this.baseUrl}${path}`, init);
    } catch (error) {
      throw new NetworkError(`engine unreachable at ${this.baseUrl}: ${String(error)}`);
    } finally {
      clearTimeout(timer);
    }
  }

  private async checked<T>(response: Response): Promise<Envelope<T>> {
    if (response.status >= 400) throw errorForResponse(response.status, await safeJson(response));
    const body = (await safeJson(response)) as Envelope<T> | null;
    if (body === null || typeof body !== "object" || body.success !== true) {
      throw new DeltuError(`unexpected response envelope: ${JSON.stringify(body)}`, "bad_envelope");
    }
    return body;
  }
}

async function safeJson(response: Response): Promise<unknown> {
  try {
    return await response.json();
  } catch {
    return { success: false, error: { code: "unknown", message: "response was not JSON" } };
  }
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
