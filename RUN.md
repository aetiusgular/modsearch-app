# Running ModSearch locally (A9 + A14 + A19)

This wires the app to the on-device engine over native messaging. No localhost:
the app loads from the extension, the engine is a native process.

## Prerequisites
- pnpm and Node
- Rust (stable)

## 1. Build the engine
```
cargo build --release -p modsearch-engine
```
Binary: `target/release/modsearch-engine` (note its absolute path).

## 2. Build the app and stage it into the extension
```
pnpm install
apps/extension/build.sh
```
This builds `apps/web` and copies the result to `apps/extension/app/`.

## 3. Load the extension
- Chrome → chrome://extensions → enable Developer mode → Load unpacked → select `apps/extension`.
- Copy the extension ID it shows.

## 4. Register the native host
```
apps/extension/host/install-host.sh "$(pwd)/target/release/modsearch-engine" <EXTENSION_ID>
```
Restart Chrome so it picks up the host.

## 5. Open it
Click the ModSearch toolbar icon. The full-page app opens and now talks to the
engine (real handlers over the stub catalog). Append `?mock` to the URL to force
the mock data client instead.

## Dev loop (UI only)
```
pnpm -C apps/web dev
```
Runs Vite for fast UI iteration. This uses the mock client and a dev server; it is
a developer convenience, never part of the shipped product. End users never touch
localhost.

## What is stubbed
The engine (A9) serves an in-memory synthetic catalog. A10 adds SQLite/DuckDB, A11
the vector index + graph, A12 ONNX embedding, A13 the taste model, A16 real ingested
boutiques. See MODSEARCH_BUILD_PROMPTS.md.

## Replaying a feed (determinism kernel, A28-A31)

Every ranked response is a pure function of (catalog state, config, userId,
dayEpoch) and carries a `provenance` field: engine version, config hash,
catalog fingerprint, seed, day epoch, and the exploration slots with their
propensities. To reproduce a feed exactly, send the same request with the
`userId` and `dayEpoch` from its provenance:

```
echo '{"type":"getFeed","query":{},"hidden":[],"userId":"local","dayEpoch":20000}' \
  | ./target/release/modsearch-engine oneshot
```

Same day, same state: byte-identical output, any machine, any run. Exploration
reseeds daily per user (seeded RNG, no wall-clock in the ranked path). Ranking
constants live in EngineConfig; point `MOD_ENGINE_CONFIG` at a JSON file to
override, and the response's `configHash` changes with it.

## Persistence and learning (A10 + A13/A11 port)

The engine now persists to SQLite. Default path `~/.modsearch/engine.db`;
override with `MOD_ENGINE_DB=/path/to.db` (tests and replay harnesses should
always set it). Like/Save/Hide from the app update the dual-window,
dual-space taste profile (the fork's exact math, golden-tested); telemetry
batches fold into episodes and flush on impression-end or the 60s idle
horizon, using event client_ts only, never the wall clock. Once taste exists
the For You feed ranks by w_cos*[cos(U,V)*(1+gamma*J)] + freshness + quality
through MMR with brand caps, and matchScore/matchReasons in responses reflect
the live model. Delete the DB file to reset to cold start.

## Offline eval harness (A26)

`cargo run -p modsearch-engine -- eval` prints a JSON evaluation report;
add `--md` for the human table. Pure and store-free, so it runs anywhere.

It reports three things over the fixture catalog:
- Baseline ladder (random -> popularity -> taste -> full), recall@10 / ndcg@10 /
  MRR / MAP with bootstrap 95% CIs. Personalized ranking beats the
  non-personalized baselines by a wide margin (recall@10 ~0.64 vs ~0.20).
- Diversity: catalog coverage@10 and Gini concentration of the shipped ranking.
- Off-policy evaluation: uniform exploration logs (the A29 propensity records)
  are used to IPS/SNIPS-estimate the value of a taste-softmax target policy
  WITHOUT deploying it. The target is estimated ~4x the uniform policy's reward,
  with the effective sample size reported so the estimate's trust is visible.

Relevance/rewards come from a simulated-preference oracle (a brand), so the
numbers validate the estimators and pipeline and gate regressions; they are not
a claim about live quality. Real evaluation swaps the oracle for the
archived-interaction temporal split (fork protocol.py), same metrics and
estimators. The report is deterministic and the golden test is the regression
gate: a ranking change that moves the headline numbers fails CI until re-pinned.
