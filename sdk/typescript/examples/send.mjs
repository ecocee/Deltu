/**
 * Runnable example: send one event to a running engine.
 *
 * Start an engine first:
 *     deltu run
 *
 * Then (from sdk/typescript):
 *     node examples/send.mjs
 */
import { DeltuClient, Event, text } from "../dist/index.js";

const baseUrl = "http://127.0.0.1:8080";

const client = new DeltuClient(baseUrl);
console.log("healthy:", await client.health());

const result = await client.sendEvent(
  new Event({
    id: `example-${Date.now()}-${Math.random()}`,
    source: "example",
    kind: "door.state",
    timestamp: Date.now(),
    payload: text("open"),
  }),
);
console.log("accepted:", result.accepted);

const snapshot = await client.status();
console.log("state entries:", snapshot.state.entries);
