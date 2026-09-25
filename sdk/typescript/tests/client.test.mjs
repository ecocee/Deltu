/**
 * Tests for @deltu/client: local validation, typed errors, envelope
 * handling, retries — plus a live end-to-end test against a real
 * `deltu run` (skipped when no engine is reachable).
 */
import assert from "node:assert/strict";
import http from "node:http";
import test from "node:test";

import {
  DeltuClient,
  DeltuError,
  EngineUnavailableError,
  Event,
  EventError,
  InvalidEventError,
  NetworkError,
  PayloadTooLargeError,
  QueueFullError,
  boolean,
  numeric,
  structured,
  text,
} from "../dist/index.js";

// ---------------------------------------------------------------------------
// Local event validation (mirrors the engine's rules)

test("event rejects empty fields", () => {
  assert.throws(() => new Event({ id: "  ", source: "s", kind: "k", timestamp: 1, payload: text("v") }), EventError);
  assert.throws(() => new Event({ id: "e", source: "", kind: "k", timestamp: 1, payload: text("v") }), EventError);
  assert.throws(() => new Event({ id: "e", source: "s", kind: "", timestamp: 1, payload: text("v") }), EventError);
});

test("event rejects bad timestamps", () => {
  assert.throws(() => new Event({ id: "e", source: "s", kind: "k", timestamp: 0, payload: text("v") }), EventError);
  assert.throws(() => new Event({ id: "e", source: "s", kind: "k", timestamp: -5, payload: text("v") }), EventError);
});

test("event rejects bad payloads", () => {
  assert.throws(() => new Event({ id: "e", source: "s", kind: "k", timestamp: 1, payload: text("   ") }), EventError);
  assert.throws(() => new Event({ id: "e", source: "s", kind: "k", timestamp: 1, payload: numeric(Infinity) }), EventError);
  assert.throws(() => new Event({ id: "e", source: "s", kind: "k", timestamp: 1, payload: { type: "numeric" } }), EventError);
  assert.throws(() => new Event({ id: "e", source: "s", kind: "k", timestamp: 1, payload: { type: "wat" } }), EventError);
  assert.throws(() => new Event({ id: "e", source: "s", kind: "k", timestamp: 1, payload: boolean(1) }), EventError);
});

test("structured depth is bounded", () => {
  const deep = {};
  let node = deep;
  for (let i = 0; i < 20; i += 1) {
    node.child = {};
    node = node.child;
  }
  assert.throws(() => new Event({ id: "e", source: "s", kind: "k", timestamp: 1, payload: structured(deep) }), EventError);
});

test("toJSON normalizes kind and keeps the tag", () => {
  const event = new Event({ id: " e1 ", source: "s", kind: "Door.State", timestamp: 10, payload: text("open") });
  const wire = event.toJSON();
  assert.equal(wire.kind, "door.state");
  assert.deepEqual(wire.payload, { type: "text", value: "open" });
});

// ---------------------------------------------------------------------------
// A tiny stub engine standing in for `deltu run`

function respond(res, status, body) {
  const data = JSON.stringify(body);
  res.writeHead(status, { "content-type": "application/json", "content-length": Buffer.byteLength(data) });
  res.end(data);
}

function startStub(handler) {
  return new Promise((resolve) => {
    const server = http.createServer(handler);
    server.listen(0, "127.0.0.1", () => resolve({ server, port: server.address().port }));
  });
}

function stubHandler(req, res) {
  if (req.method === "GET" && req.url === "/health") {
    respond(res, 200, { status: "ok" });
  } else if (req.method === "GET" && req.url === "/v1/status") {
    respond(res, 200, { success: true, data: { version: "0.1.0", queue_depth: 0, state: { entries: 2 } } });
  } else if (req.method === "POST" && req.url === "/v1/events") {
    let raw = "";
    req.on("data", (chunk) => { raw += chunk; });
    req.on("end", () => {
      const body = JSON.parse(raw);
      const id = body.events[0].id;
      if (id === "bad") {
        respond(res, 400, { success: false, error: { code: "invalid_event", message: "events[0] failed validation: bad kind" } });
      } else if (id === "big") {
        respond(res, 413, { success: false, error: { code: "payload_too_large", message: "too large" } });
      } else if (id === "full") {
        respond(res, 429, { success: false, error: { code: "queue_full", message: "queue is full" } });
      } else if (id === "down") {
        respond(res, 503, { success: false, error: { code: "shutting_down", message: "shutting down" } });
      } else {
        respond(res, 202, { success: true, data: { accepted: body.events.map(() => true), actions_fired: 0 } });
      }
    });
  } else {
    respond(res, 404, { success: false, error: { code: "unknown", message: "nope" } });
  }
}

const event = (id = "e1") => new Event({ id, source: "s", kind: "k", timestamp: 1, payload: text("v") });

test("health and status against the stub", async () => {
  const { server, port } = await startStub(stubHandler);
  try {
    const client = new DeltuClient(`http://127.0.0.1:${port}`);
    assert.equal(await client.health(), true);
    const snapshot = await client.status();
    assert.equal(snapshot.version, "0.1.0");
    assert.equal(snapshot.state.entries, 2);
  } finally {
    server.close();
  }
});

test("sendEvents round trip", async () => {
  const { server, port } = await startStub(stubHandler);
  try {
    const client = new DeltuClient(`http://127.0.0.1:${port}`);
    const batch = await client.sendEvents([event(), event("e2")]);
    assert.deepEqual(batch.accepted, [true, true]);
    const single = await client.sendEvent(event());
    assert.equal(single.accepted, true);
  } finally {
    server.close();
  }
});

test("typed errors map status codes", async () => {
  const { server, port } = await startStub(stubHandler);
  try {
    const client = new DeltuClient(`http://127.0.0.1:${port}`);
    await assert.rejects(client.sendEvent(event("bad")), (error) => {
      assert.ok(error instanceof InvalidEventError);
      assert.equal(error.code, "invalid_event");
      assert.match(error.message, /events\[0\]/);
      return true;
    });
    await assert.rejects(client.sendEvent(event("big")), PayloadTooLargeError);
    await assert.rejects(client.sendEvent(event("full")), QueueFullError);
    await assert.rejects(client.sendEvent(event("down")), EngineUnavailableError);
  } finally {
    server.close();
  }
});

test("bad envelope is an error", async () => {
  const { server, port } = await startStub((req, res) => {
    respond(res, 404, { success: false, error: { code: "unknown", message: "nope" } });
  });
  try {
    const client = new DeltuClient(`http://127.0.0.1:${port}`);
    await assert.rejects(
      () => client.status(),
      (error) => {
        assert.ok(error instanceof DeltuError); // unmapped 404 → base type
        assert.equal(error.code, "unknown");
        return true;
      },
    );
  } finally {
    server.close();
  }
});

test("unreachable engine raises NetworkError", async () => {
  const client = new DeltuClient("http://127.0.0.1:9", { timeoutMs: 300 });
  await assert.rejects(client.health(), NetworkError);
});

test("retries only retryable statuses", async () => {
  let attempts = 0;
  const { server, port } = await startStub((req, res) => {
    if (req.method === "POST") {
      attempts += 1;
      if (attempts < 3) {
        respond(res, 429, { success: false, error: { code: "queue_full", message: "queue is full" } });
        return;
      }
    }
    stubHandler(req, res);
  });
  try {
    const client = new DeltuClient(`http://127.0.0.1:${port}`, { retries: 3, backoffMs: 10 });
    const result = await client.sendEvent(event());
    assert.equal(result.accepted, true);
    assert.equal(attempts, 3);
  } finally {
    server.close();
  }
});

// ---------------------------------------------------------------------------
// Live end-to-end against a real engine (skipped when none is running)

const LIVE_PORT = 8210;

async function engineReachable() {
  try {
    const response = await fetch(`http://127.0.0.1:${LIVE_PORT}/health`, { signal: AbortSignal.timeout(500) });
    return response.ok;
  } catch {
    return false;
  }
}

test("live engine end-to-end", { skip: !(await engineReachable()) && "no live engine on 127.0.0.1:8210" }, async () => {
  const client = new DeltuClient(`http://127.0.0.1:${LIVE_PORT}`);
  assert.equal(await client.health(), true);
  const result = await client.sendEvent(new Event({
    id: `ts-live-${Date.now()}-${Math.random()}`,
    source: "sdk-typescript",
    kind: "door.state",
    timestamp: Date.now(),
    payload: text("open"),
  }));
  assert.equal(result.accepted, true);
  const snapshot = await client.status();
  assert.ok("entries" in snapshot.state);
});
