// Node test for the A15 capture core. No framework: `node capture-core.test.js`.
// Proves dwell accumulates only foreground time, the event envelope + fields
// match the contract, and the edge cases hold.
const assert = require("assert");
const C = require("./capture-core.js");

function ids() {
  let n = 0;
  return { uuid: () => `uuid-${++n}`, sessionId: "sess1234", deviceId: "dev12345", userId: "local" };
}

// dwell accumulates only the visible time across visibility changes
{
  const out = [];
  const d = new C.ItemDwell("shopify:nomanwalksalone.com:15257", ids(), (e) => out.push(e));
  d.start(1000, 0.8); // impression_start at t=1s
  d.onHidden(6000); // was visible 5s -> accum 5000
  d.onVisible(9000); // hidden for 3s (not counted)
  d.end(11000); // visible 2s more -> accum 7000; emits impression_end
  const start = out.find((e) => e.type === "impression_start");
  const end = out.find((e) => e.type === "impression_end");
  assert.equal(start.item_id, "shopify:nomanwalksalone.com:15257");
  assert.equal(start.viewport_pct, 0.8);
  assert.equal(start.schema_version, 1);
  assert.equal(start.session_id, "sess1234");
  assert.equal(start.device_id, "dev12345");
  assert.equal(end.type, "impression_end");
  assert.equal(end.dwell_ms, 7000, `dwell must be visible time only, got ${end.dwell_ms}`);
  assert.equal(end.max_viewport_pct, 0.8);
  assert.equal(new Date(end.client_ts).getTime(), 11000);
}

// live dwell reflects the currently-open visible span
{
  const d = new C.ItemDwell("i", ids(), () => {});
  d.start(0, 1);
  assert.equal(d.dwellMs(4000), 4000);
  d.onHidden(4000);
  assert.equal(d.dwellMs(9000), 4000, "hidden time does not accrue");
}

// end() before start() emits nothing; double end() is safe
{
  const out = [];
  const d = new C.ItemDwell("i", ids(), (e) => out.push(e));
  d.end(5000);
  assert.equal(out.length, 0, "end without start emits nothing");
  d.start(0, 1);
  d.end(1000);
  d.end(2000);
  assert.equal(out.filter((e) => e.type === "impression_end").length, 1, "only one impression_end");
}

// heartbeat + click_detail shapes match the contract
{
  const out = [];
  const id = ids();
  const d = new C.ItemDwell("i", id, (e) => out.push(e));
  d.start(0, 1);
  d.heartbeat(1000, true);
  const hb = out.find((e) => e.type === "heartbeat");
  assert.equal(hb.visible, true);
  assert.equal(hb.item_id, "i");
  const clk = C.clickDetail("i", id, 2000, "external");
  assert.equal(clk.type, "click_detail");
  assert.equal(clk.source, "external");
  assert.equal(clk.item_id, "i");
  assert.ok(clk.event_id && clk.client_ts);
}

// clamp guards viewport fractions
assert.equal(C.clamp01(1.5), 1);
assert.equal(C.clamp01(-0.2), 0);
assert.equal(C.clamp01(0.5), 0.5);
assert.equal(C.clamp01(NaN), 0);

console.log("capture-core: all assertions passed");
