# Zero2Prod (in axum) — Progress Log

Learning project: working through Luca Palmieri's *Zero To Production In Rust*, but
building everything in **axum** instead of the book's actix-web. Goal is learn-by-doing.

**Working style:** Claude guides + explains deeply (the "why", Rust idioms, axum-vs-actix
differences); **I write the code myself** and ask when stuck. Claude does NOT edit project
files unless I explicitly ask.

---

## ✅ Done so far (Chapter 3 — COMPLETE)

### Milestone 1 — Health check endpoint (complete)
- Split into **library crate** (`src/lib.rs`, real logic) + thin **binary** (`src/main.rs`).
- `health_check` handler → returns `StatusCode::OK`.
- `app()` builds the `Router`; `run(listener)` serves it (takes a `TcpListener` arg so tests
  can inject a port-0 listener).
- Black-box **integration test** in `tests/`, using `reqwest` as the HTTP client.

### Test suite restructure (complete)
- Moved from a single `tests/health_check.rs` into a **single test binary** under `tests/api/`:
  - `tests/api/main.rs`  → declares modules (`mod helpers; mod health_check; mod subscriptions;`)
  - `tests/api/helpers.rs` → `spawn_app()` (shared; binds port 0, spawns server, returns base URL)
  - `tests/api/health_check.rs`
  - `tests/api/subscriptions.rs`
- Reason: each file directly in `tests/` is its own crate (can't share code, slow to link);
  the folder layout makes them **modules of one crate** so they share `helpers::spawn_app`.

### Milestone 2 — POST /subscriptions, step 1 (complete: accepts form, no DB yet)
- `FormData` struct with `#[derive(serde::Deserialize)]` (fields `name`, `email`).
- `subscribe` handler takes the **`Form<FormData>`** extractor, returns `StatusCode::OK`.
  (Kept as `data: Form<FormData>` — not destructured — so fields are accessed via `data.0.email`.)
- Registered `POST /subscriptions` route with `post(subscribe)`.
- Tests: `subscribe_returns_a_200_for_valid_form_data` (passes).
- `subscribe_returns_a_422_when_data_is_missing`: **axum returns 422 (Unprocessable Entity),
  NOT the book's 400.** Reason: malformed-but-parseable body w/ a missing field is a *semantic*
  failure (422), not a *syntax* failure (400). Assertion updated to `UNPROCESSABLE_ENTITY`.

### Milestone 3 — persist subscriber to Postgres (DONE — all steps)

**DB setup (done):**
- `newsletter` DB created + migration applied via `sqlx-cli` (`sqlx database create` / `migrate run`).
  Verified `subscriptions` table: `id` (PK), `name`, `email` (UNIQUE), `subscribed_at` (timestamptz).
- Note: local Postgres uses **trust auth** — it does NOT check the password locally (a junk
  password still connects). So `APP_DATABASE__PASSWORD`'s value is cosmetic *locally*; it matters
  only when auth is tightened / in prod.

**Config & secrets (done):**
- `configuration.yaml` = committed **shared baseline**, NO secrets (host/port/username/db_name only).
- Password comes from the **environment**, layered on top via the `config` crate's
  `Environment::with_prefix("APP").separator("__")` source. So `APP_DATABASE__PASSWORD` → maps to
  the nested struct field `database.password`. (`APP_` = "is this var mine?" filter; `__` = nesting
  separator, double so it doesn't clash with the single `_` in `database_name`.)
- `.env` (gitignored) holds local secrets, loaded into the process via `dotenvy::dotenv().ok()`
  (added `dotenvy = "0.15"`). Both `main` AND tests must call it (separate processes).
- `.env` has TWO entries on purpose: `DATABASE_URL` (for `sqlx-cli` + the `query!` compile-time
  macro) and `APP_DATABASE__PASSWORD` (for the app's `config`). Not duplication to "fix" — two
  tools, two formats.
- **`@` in the password is URL-reserved.** In `DATABASE_URL` it's percent-encoded (`@`→`%40`).
  In the app we *avoid URLs entirely*: replaced `connection_string()` (built a `postgres://…`
  string) with **`connect_options() -> PgConnectOptions`** (sets host/user/password as separate
  fields → any char is safe). App connects via `PgPool::connect_with(...)`.

**App wiring (done — Step 1 & 2, `cargo check --bin zero2prod` is GREEN):**
- `configuration` module moved into the **library** (`pub mod configuration;` in lib.rs), so the
  binary/tests reach it as `zero2prod::configuration::…`.
- `PgPool` threaded as axum shared state: `main` builds the pool → `run(listener, pool)` →
  `app(pool)` → `.with_state(pool)` → handler pulls it via `State(pool): State<PgPool>`.
  (This is the actix `web::Data<PgPool>` → axum `State<PgPool>` translation.)
- `subscribe` handler now `INSERT`s via `sqlx::query!(...)` (compile-time-checked SQL, `$1..$4`
  bound params, `Uuid::new_v4()` + `Utc::now()`), `.execute(&pool)`, returns 200 on `Ok` / 500 on `Err`.

---

**Step 3 — test proves persistence (DONE):**
- `spawn_app` builds a `PgPool`, returns `TestApp { address, db_pool }`; passes `pool.clone()` to
  `run`, keeps the original for querying. All 3 call sites use `app.address`.
- 200 test now `SELECT email, name`s via `.fetch_one(&app.db_pool)` and asserts the saved row.
  (Gotcha: assert against the **decoded** values — `@` not `%40`, space not `%20`. Percent-encoding
  is transport-only; the `Form` extractor decodes before storing.)

**Test isolation (DONE — the real end of Ch 3):**
- Each `spawn_app` overrides `database.database_name` with a fresh `Uuid::new_v4()`, then
  `configure_database` **creates** that DB and **migrates** it. So every test run gets a pristine,
  randomly-named DB → no UNIQUE collisions, `newsletter` never touched.
- **Two connections, two destinations:** `without_db()` connects to the **`postgres`** maintenance
  DB (you must be connected to an *existing* DB to run `CREATE DATABASE`); `with_db()` connects to
  the just-created DB to migrate + use it. `with_db()` = `without_db().database(&self.database_name)`.
- `without_db()` had to add `.database("postgres")` — with NO database set, Postgres defaults the
  db name to the **username** (`jyothi`), which doesn't exist → error 3D000.
- Config fix: `Environment::with_prefix("APP").prefix_separator("_").separator("__")`. The `config`
  crate uses the *separator* to strip the prefix too, so `APP_DATABASE__PASSWORD` (single `_` after
  APP) was being ignored. `prefix_separator("_")` makes the "`_` filters, `__` nests" model real.
- **sqlx 0.9 SQL-injection guard:** can't `.execute()` a runtime-built `String` directly — a plain
  `&str` must be `&'static`. Wrap dynamic SQL in `sqlx::raw_sql(sqlx::AssertSqlSafe(query))` to
  assert it's audited/not user-controlled (here it's only our own UUID).
- Leftover test DBs are NOT auto-dropped (book's choice). Cleanup scripted in
  `scripts/clean_test_dbs.sh` (drops only UUID-named DBs). Auto-teardown via `Drop` deferred — async
  -in-`Drop` + must close pool before `DROP DATABASE`; revisit later.

---

## ✅ Added 2026-06-23 session — Multi-environment config split (DONE, `cargo check` GREEN)

**Mental model first — the universal backend skeleton.** Every axum backend `main` is the same
six slots, contents grow but slots don't: `TELEMETRY → CONFIG → RESOURCES(state) → ROUTER(+middleware)
→ BIND → SERVE`. `main` = composition (wire real pieces); `lib` = the pieces themselves (so tests
construct them independently). Two slots not yet filled: **observability** (slot #1, `tracing`) and
**middleware** (`.layer(...)`).

**The config refactor (replaces the old single `configuration.yaml`):**
- Split one file → a `configuration/` **folder at project root** (data; distinct from
  `src/configuration.rs`, the code that reads it):
  - `base.yaml` — identical everywhere (`application_port`, `database.port`, `database.database_name`).
  - `local.yaml` — laptop, non-secret (`host: localhost`, `username: jyothi`). Default when env unset.
  - `production.yaml` — prod, non-secret placeholders (`host`, `username`). No real server yet.
- **Three-bin rule** (drives where every setting lives): same-everywhere → `base.yaml`; differs-but-
  not-secret → per-env yaml; differs-AND-secret → env var / secrets store. Username *differs* (kicked
  out of base) but isn't secret (→ per-env yaml); password differs AND secret (→ `.env` / secrets).
- **`Environment` enum** (private — nothing outside the module names it; `pub` is a refactor-freedom
  cost, add only when a boundary forces it):
  - `as_filename(&self) -> &'static str` — variant → filename. `&'static str` not `String` because the
    returns are **literals already in the binary**; borrow them, don't heap-allocate a copy. (`&str`
    alone won't compile in return position — needs a lifetime; `'static` = "lives the whole program".)
  - `impl TryFrom<String>` (`type Error = String`) — gates the **untrusted env-var string** (open set:
    `"banana"`, `"prod"`, …) into the **trusted 2-variant enum** at startup. Error must be owned
    `String` because it's `format!`-built at runtime (nothing pre-existing to borrow) — opposite branch
    of the same own-vs-borrow rule as `as_filename`. Gives `.try_into()` for free.
- **`get_configuration` rewired:**
  - Path built from `std::env::current_dir().join("configuration")` (robust) instead of bare relative
    `"configuration.yaml"` (fragile — only worked from repo root).
  - `std::env::var("APP_ENVIRONMENT").unwrap_or_else(|_| "local".into()).try_into().expect(...)` —
    **safe default**: unset → local; dangerous env (prod) requires *explicit* opt-in.
  - `config::File::from(path)` (infers YAML from extension) replaces `File::new(name, FileFormat::Yaml)`.
  - Sources, weak→strong (last wins): `base.yaml` → `{env}.yaml` → `APP_*` env vars. Order = override
    policy; flip any pair and the wrong layer wins.
- **`expect`/panic vs `?`** judgment: bad `APP_ENVIRONMENT` and `current_dir()` failure are
  **unrecoverable startup faults** (no graceful handling — crash loud & early) → `expect`. `build()`
  uses `?` because `ConfigError` is the function's *declared* error channel (a missing field is a
  normal, well-typed failure `main` already handles).

**Deployment mental model (discussed, not yet built):**
- A later step adds `APP_ENVIRONMENT=production` switch via per-env files toggled by **one** var.
- **Config is declarative & ahead-of-time, NOT live.** Prod env vars are *declared once* in the
  platform (dashboard / `docker -e` / k8s `env:` / systemd unit / CI) and injected on **every** boot —
  you do NOT SSH in to hand-edit `.env`. `.env` is a laptop-only convenience.
- **Two separate databases, never linked.** Local Postgres = throwaway junk data; prod = real users.
  **Data never travels.** What travels local→prod: (1) Rust **code** and (2) migration **`.sql` files**
  (via git/CI). Migrations = versioned, ordered, run-once-per-DB schema scripts → every DB ends up the
  *same shape*. Same `sqlx migrate run`, different `DATABASE_URL`.

**Logging:** decided to DEFER `println!` info-logs (environment / db-connect / listening) — would just
be ripped out when `tracing` lands. Slot #1 reserved for the telemetry session.

---

## 🚧 Chapter 4 (Telemetry) — IN PROGRESS (slot #1)

### ✅ #2 DONE — the real 3-crate subscriber (JSON output) — verified `cargo run` emits JSON

**The motivation chain (reasoned out from first principles, NOT memorized):**
prod breaks while unwatched → need a saved *recording*, not a live feed → SSH-and-watch only
shows the *present*, the 3am failure already scrolled past on a screen no one was reading →
app writes logs to **stdout**, the **platform/supervisor** (Docker/systemd/k8s/cloud) captures
that stream into **durable storage** → you **search it later** at 09:00 → searching requires
**structured JSON** (named fields a machine can index, like DB columns) not pretty text (a
grep-the-waterfall substring hunt) → therefore build a JSON-emitting subscriber. *This file is
that "therefore."*

**Key clarification locked in:** we did NOT switch away from `tracing_subscriber` — we only
dropped its `fmt::init()` **shortcut** (the vending-machine button that hard-wires pretty-text-
to-stdout). Same crate; `Registry`/`EnvFilter`/`SubscriberExt` still come from it. Hand-building
buys 3 things: (1) **JSON** [the real why], (2) **layers we control**, (3) **the `log→tracing`
bridge** we now wire ourselves (`fmt::init()` did it invisibly).

**The 3 crates & their roles:** `tracing-subscriber` = chassis (`Registry`, bottom of stack,
stores span state, NO output) + `EnvFilter` (volume knob, reads `RUST_LOG`); `tracing-bunyan-
formatter` = `JsonStorageLayer` (collects span fields so events inherit them) + `BunyanFormatting
Layer` (actually prints JSON-per-line); `tracing-log` = `LogTracer` (catches the `log` crate's
records — i.e. our *dependencies* — and re-emits as tracing events, else they silently vanish).

**Mental model — a subscriber is a STACK OF LAYERS** assembled via `.with()` (last = outermost):
`Registry::default().with(env_filter).with(JsonStorageLayer).with(formatting_layer)`. `.with()`
comes from the `SubscriberExt` extension trait → must be `use`d even though never named (else
"method `with` not found").

**The build/install SPLIT (the crux):** `get_subscriber` = *build* the machine, returns
`impl Subscriber + Send + Sync`, ZERO side effects, callable many times (`Send + Sync` because the
global is hit by every worker thread; `impl Trait` return because the real type is un-spellable
nested generic soup). `init_subscriber` = *install* it (`LogTracer::init()` + `set_global_default`),
the "exactly once" side effect, `.expect()` because a failed logger at startup is unrecoverable
(can't log the logging failure — crash loud & early). This split is WHY tests will work (below).

**Two bug/idiom catches from this session:**
- **`set_global_default` import trap:** `tracing` has TWO — `tracing::dispatcher::set_global_default`
  (wants a `Dispatch`) vs `tracing::subscriber::set_global_default` (wants a `Subscriber`). IDE
  autocompletes the `dispatcher::` one first → won't compile ("expected `Dispatch`"). Use the
  `subscriber::` one (the ergonomic wrapper that boxes our subscriber into a `Dispatch` for us).
- **`std::io::stdout` passed WITHOUT `()`** — handing over the function (a writer-*factory*), not
  one handle; the layer calls it for a fresh handle per line so concurrent threads don't collide
  (the `MakeWriter` pattern → also what makes the `stdout`↔`sink` test toggle possible tomorrow).
- **`name: String` not `&str`** in `get_subscriber` → call sites need `.into()`/`String::from(...)`/
  `.to_string()` on the `"zero2prod"`/`"info"` literals (all 3 identical; `.into()` only works when
  the target type is inferable from the signature). Reason it takes owned `String`: the formatting
  layer *keeps* the name for the program's life — can't hold a borrow that long without lifetimes.

**Files touched (all GREEN, `cargo check` passes, JSON verified via curl):**
- `Cargo.toml`: added `tracing-bunyan-formatter = "0.3"`, `tracing-log = "0.2"`; added `registry`
  feature to `tracing-subscriber`; removed `log`/`env_logger`.
- `src/telemetry.rs`: NEW — `get_subscriber` + `init_subscriber` (see above).
- `src/lib.rs`: `pub mod telemetry;` (in the library so tests reach `zero2prod::telemetry::…`).
- `src/main.rs`: replaced `tracing_subscriber::fmt::init()` with the two-step
  `let subscriber = get_subscriber("zero2prod".into(), "info".into()); init_subscriber(subscriber);`.
  (Old line + a why-comment kept as breadcrumbs.)
- **Verified:** `cargo run` + POST /subscriptions → JSON lines with `[ADDING A NEW SUBSCRIBER -
  START]`/`- END]` (END carries `elapsed_milliseconds` = span-as-a-timed-box) and every line
  carrying `request_id`/`subscriber_email`/`subscriber_name` inherited from the span. Orphan-dots
  problem dead AND output now queryable.

### ✅ #3 DONE — test suite emits logs + `stdout`↔`sink` toggle (verified)
- **Step 1 (LazyLock install-once):** `static TRACING: LazyLock<()>` in `tests/api/helpers.rs`
  builds + installs the subscriber; first line of `spawn_app` is `LazyLock::force(&TRACING)`.
  Solves the "global subscriber installs once per process, but `cargo test` = N tests in ONE
  process" panic. `LazyLock` runs its closure on FIRST touch, no-ops after, and is thread-safe
  (synchronizes the race to be first toucher). `<()>` = we want only the side effect.
- **Step 2 (sink toggle):** `get_subscriber` now generic over the **sink** —
  `get_subscriber<Sink>(name, env_filter, sink) where Sink: for<'a> MakeWriter<'a> + Send + Sync + 'static`.
  `BunyanFormattingLayer::new(name, sink)` instead of hard-wired `std::io::stdout`. `main` passes
  `std::io::stdout`; `helpers` `TRACING` does `if std::env::var("TEST_LOG").is_ok()` → `stdout`
  else `std::io::sink` (black hole). Default `cargo test` = SILENT; `TEST_LOG=true cargo test
  -- --nocapture` = JSON flood. **Verified both directions** (0 lines silent / 3 lines flooded).
  - **Key idea locked in:** `env_filter` controls the *level* (which events survive); the *sink*
    controls the *destination* (where survivors go) — two independent dials. Even with `RUST_LOG`
    unset the floor is `info`, so logs are NOT off by default — the `sink` is what silences tests.
  - **Why the `if/else` duplicates the whole `get_subscriber`+`init_subscriber` call:** `std::io::stdout`
    and `std::io::sink` are *different concrete types*, so an `if/else` can't return "either" into one
    variable; the generic monomorphizes per branch → the call lives inside each arm. (Static dispatch
    tradeoff; `Box<dyn>` would unify but the book picks zero-cost.)
  - **stdout = a stream (fd 1), NOT "the terminal":** the program writes to fd 1 and is ignorant of
    where it leads; the launcher wires it (terminal interactively, file via `>`, platform collector in
    prod). `std::io::sink` is the one fixed "nowhere" destination.

### ✅ #4 DONE — request-tracing as MIDDLEWARE (the last Ch 4 item; skeleton slot #6 filled)
- **actix→axum translation:** book's `TracingLogger` middleware → **`tower-http`'s `TraceLayer`**.
  Added `tower-http = { version = "0.6", features = ["trace"] }`.
- In `app()`: `.layer(TraceLayer::new_for_http().make_span_with(|request| info_span!("request",
  request_id = %Uuid::new_v4(), method = %request.method(), uri = %request.uri())))`, placed
  BEFORE `.with_state(pool)` (still pay the state debt last → `Router<()>`).
- **Two-level span hierarchy now:** outer `request` span (middleware, every request, `request_id`/
  `method`/`uri`) wraps inner `Adding a new subscriber` span (handler, `subscriber_email`/`name`).
  Removed `request_id` from `subscribe`'s `#[instrument]` fields — it now INHERITS from the parent
  request span (bunyan `JsonStorageLayer` merges every enclosing span's fields onto each event).
- **Rule locked in:** put a field on the HIGHEST span where it's true (request_id = whole-request →
  middleware; subscriber_email = one-operation → handler). Events collect the union of all enclosing
  spans. Middleware span = mandatory free baseline; `#[instrument]` = optional per-handler enrichment
  only when an operation has its own identity/fields/timing (so `health_check` stays BARE).
- **Verified:** `curl /health_check` (a bare handler) emits exactly `[REQUEST - START]` + `[REQUEST -
  END]` JSON with `request_id`/`method=GET`/`uri`, END carrying `elapsed_milliseconds`. TraceLayer's
  built-in `on_request`/`on_response` events are DEBUG → filtered out at the `info` floor (would show
  under `RUST_LOG=debug`) — concrete proof the filter shapes *level*, not destination.

## 🎉 CHAPTER 4 (Telemetry) — COMPLETE. All skeleton slots now filled.

---

## 🚧 Chapter 5 (Deployment) — IN PROGRESS

**Roadmap (full arc):** 0 `.dockerignore` → 1 sqlx offline → 2 bind `0.0.0.0` (`host` field) →
3 naive build runs → 4 optimize (multi-stage + cargo-chef) → 5 deploy to host + managed Postgres →
6 deferred Ch-3 debts (least-priv DB role, secret-managed password).

### ✅ Step 0 — `.dockerignore` (DONE)
- Created `.dockerignore` with `target/` (gigabytes of build artifacts) and `.env` (local secret).
- **Why it matters:** the `.` in `docker build` = the **build context**, a tar bundle shipped to the
  daemon. `.dockerignore` prunes that bundle, so `COPY . .` *cannot* copy what's excluded — secret
  never enters the image, and we don't ship GBs over the socket every build.

### ✅ Step 1 — sqlx OFFLINE mode (DONE — the chapter's crux so far)
- **The problem:** `sqlx::query!` is a macro that connects to a LIVE Postgres *at compile time* (via
  `DATABASE_URL`) and runs `PREPARE` to type-check the SQL against the real schema. Inside `docker
  build` there's no DB, no `.env`, no network → `cargo build` would fail. Build-time DB validation
  (great for bugs) vs self-contained reproducible builds (no external deps) are in tension; offline
  mode resolves it.
- **The fix = a committed cache.** `cargo sqlx prepare` connects ONCE locally, finds every `query!`,
  asks Postgres to validate each, and writes one JSON per query into `.sqlx/`. Commit `.sqlx/` (NOT
  ignored — the build needs it; it's schema-knowledge, not data → travels like migrations do).
  `ENV SQLX_OFFLINE true` in the Dockerfile makes the macro read `.sqlx/` instead of phoning a DB.
- **`--all-targets` gotcha:** plain `cargo sqlx prepare` scans only lib+bin → it MISSED the `query!`
  in `tests/api/subscriptions.rs` (the `SELECT email,name`). Re-ran `cargo sqlx prepare -- --all-targets`
  → now **2 JSON files** (INSERT from `src/lib.rs:42` + SELECT from the test). The missing test query
  wouldn't break the Docker build (release build = bin only, not tests) but WOULD break `cargo test`
  / CI under `SQLX_OFFLINE`.
- **Cache mechanics locked in:** ONE file per *unique query*, not per run. Filename `query-<hash>.json`
  where hash = fingerprint of the SQL string (content-addressed, like git objects). Re-running prepare
  is idempotent (same queries → same files overwritten; orphaned/old-hash files cleaned up). Change SQL
  → new hash → new file; the macro hashing the new text and finding no file = the staleness safety check
  (build fails loud rather than validating against an old snapshot). **Discipline: change any SQL →
  re-run `cargo sqlx prepare -- --all-targets` → commit `.sqlx/`.**
- The cache JSON holds the DB's frozen answer: `parameters.Left` = `$1..$N` types Postgres reported
  (Uuid/Text/Text/Timestamptz for the INSERT), `columns`/`nullable` = output column info.

### ✅ Step 2 — `host` field + `0.0.0.0` bind (DONE, USER WROTE IT)
- Added `host: String` to `ApplicationSettings`; `local.yaml` = `127.0.0.1`, `production.yaml` = `0.0.0.0`.
- `TcpListener::bind(format!("{}:{}", config.application.host, config.application.port))` — host now
  comes from config, not hardcoded. `0.0.0.0` = bind all interfaces → reachable from outside the container.
  `127.0.0.1` = loopback only → in-container only, unreachable from host/outside.

### ✅ Step 3 — naive image BUILDS and the binary RUNS (DONE)
- Dockerfile (single-stage, now superseded): `FROM rust:1.96.0` · `WORKDIR /app` · `apt-get install lld clang`
  · `COPY . .` · `ENV SQLX_OFFLINE true` · `cargo build --release` · `ENTRYPOINT`.
- `docker build -t zero2prod .` → **SUCCESS.** Offline mode end-to-end proven.
- **Image = 4.49 GB** — shipped whole Rust toolchain + build artifacts + OS just to run one ~30 MB binary.

### ✅ Step 4 — optimised image: multi-stage build + cargo-chef (DONE)
- **Two problems solved separately:**
  1. **Image too large** → multi-stage build. Only the last stage's filesystem becomes the final image.
     Builder stage (big: Rust toolchain, lld, clang, target/) compiles the binary; runtime stage (tiny:
     `debian:bookworm-slim`) copies only the binary out. Toolchain + artifacts discarded.
     `debian:bookworm-slim` not Alpine: same glibc family as the builder → no dynamic linking issues.
     Runtime needs `openssl` (dynamically linked — binary has a "load libssl.so at startup" note) and
     `ca-certificates` (a file OpenSSL reads to verify TLS certs). Both installed in one `RUN` chain
     (one layer) so the apt cache cleanup actually reduces size — deletions in a later layer don't help.
     **Result: 4.49 GB → 161 MB.**
  2. **Builds too slow** → cargo-chef. Every `docker build` recompiled all 50 deps from scratch because
     `COPY . .` invalidates the layer before `cargo build` on every source change. cargo-chef splits
     dep compilation from your-code compilation via a `recipe.json` manifest:
     - **Planner stage:** `COPY . .` → `cargo chef prepare` → `recipe.json` (captures all targets including
       implicit ones like `src/lib.rs`, `src/main.rs`, `tests/api/main.rs` that Cargo.toml doesn't list).
     - **Builder stage:** `COPY recipe.json` → `cargo chef cook` (creates dummy source files at all target
       paths to pass cargo's pre-flight existence check, compiles all deps into target/) → `COPY . .` →
       `cargo build --release` (deps cached, only your code recompiles).
     - Cook layer is cached as long as `Cargo.toml`/`Cargo.lock` unchanged. Source change → cook = cache
       hit, only your code reruns. **Verified: second build after touching src/lib.rs was nearly instant.**
- **Key concepts locked in:** compiled binary = machine code, no Rust toolchain needed at runtime; deps
  are statically linked (baked in) except OpenSSL (dynamically linked by design — security patches);
  containers share kernel, each has own userspace; Docker layers = diffs, only last stage ships;
  layer order rule: rarely-changes near top, often-changes near bottom; `&&` chains = one layer so
  cleanup actually works; `recipe.json` exists because cargo's pre-flight check requires ALL target
  source files to exist before compiling anything.

### 🔜 NEXT — Step 5 (deploy to real host + managed Postgres) → Step 6 (deferred debts)

### (historical) #3 plan — make the test suite emit logs (the "init only once" puzzle)
**Puzzle (already reasoned out):** subscriber installs ONCE per process, but `cargo test` = many
tests in ONE process, each calling `spawn_app` → naive per-test install panics on test #2. The
build/install split above is the fix.

**Plan (NOT yet written — pick up here):**
- *Step 1 — run-once static* in `tests/api/helpers.rs`:
  ```rust
  use std::sync::LazyLock;
  use zero2prod::telemetry::{get_subscriber, init_subscriber};
  static TRACING: LazyLock<()> = LazyLock::new(|| {
      let subscriber = get_subscriber("test".into(), "info".into());
      init_subscriber(subscriber);
  });
  ```
  then first line of `spawn_app`: `LazyLock::force(&TRACING);`. `LazyLock` runs its closure on the
  FIRST touch only → installs once; later touches are no-ops. `<()>` = we want only the side effect,
  not a value. `force` = trigger it (a lazy static is inert until touched). Then `cargo test`.
- *Step 2 — silence by default* (the `stdout`↔`sink` toggle): refactor `get_subscriber` to take the
  **destination (sink) as a parameter** (becomes generic over `MakeWriter`); `main` passes
  `std::io::stdout`, tests pass `std::io::stdout` if `TEST_LOG` env set else `std::io::sink` (a
  black hole, like /dev/null). Default `cargo test` stays quiet; `TEST_LOG=true cargo test` floods
  logs for debugging a specific failure. (`get_subscriber` sig change → must update `main.rs` call
  too.)

**Done so far (pre-this-session, #1 — kept for context):**
- Walked the book's arc: `log` + `env_logger` → felt the wall → `tracing` + spans.
- Added `log = "0.4"` + `env_logger = "0.9"`; emitted `info!`/`error!` in `subscribe`;
  proved (2 concurrent curls) that flat logs are **orphaned dots** — can't tell which
  request a line belongs to under interleaving.
- Switched `log::` → `tracing::` (drop-in), added a span via `info_span!` + `.enter()`.
  Saw it was NOT enough through `env_logger` (the `log`-world speaker is deaf to spans —
  ERROR lines stayed naked).
- **Stripped-down subscriber** (skipped the book's 3-crate bunyan setup for now):
  `tracing-subscriber = { version = "0.3", features = ["env-filter"] }`, removed `env_logger`,
  one line in main: `tracing_subscriber::fmt::init()`. NOW events inherit span context
  (`request_id`, `subscriber_email`, `subscriber_name`) automatically — orphan problem dead.
- ✅ **#1 done (verified `cargo check` green):** `subscribe` refactored from `info_span!`/`.enter()`
  → `#[tracing::instrument(name="Adding a new subscriber", skip(form,pool), fields(request_id, subscriber_email, subscriber_name))]`.
  Fixes the async context-bleed bug AND deletes the span boilerplate. (`%&x` works but the `&` is
  redundant — `%x` is cleaner.)

**Two crucial sigils / rules locked in:**
- `%x` = record field via `Display`; `?x` = via `Debug`. `skip(...)` stops `#[instrument]`
  auto-capturing un-printable args (`PgPool`, `Form`); `fields(...)` can still reference them.
- **NEVER hold a `.enter()` span guard across an `.await`** — always `#[tracing::instrument]`
  on async fns. (Deep dive below.)

**Still TODO in Ch 4:** NONE — chapter complete. **← NEXT SESSION: Chapter 5 (deployment).**
- ~~#2 — structured JSON + `get_subscriber`/`init_subscriber` split~~ ✅ **DONE**.
- ~~#3 — test suite emits logs + `stdout`↔`sink` toggle~~ ✅ **DONE**.
- ~~#4 — request tracing as `TraceLayer` middleware~~ ✅ **DONE**.

## 🪧 Deferred (revisit in Ch 5 — deployment)
- Use a **dedicated, least-privilege app role** instead of the `jyothi` superuser.
- Real password with special chars: percent-encode in any URL, or stick with `PgConnectOptions`.
- Production secrets via env/secret manager (the env-override pipeline is already in place).

---

## Concepts covered (so I don't re-explain unless asked)
package vs crate vs module; lib vs bin; integration tests = separate crates; `crate::` (own crate)
vs crate-name path (external crate); async/futures/lazy poll model; runtime + `#[tokio::main]`
expansion (saw via `cargo expand`); tasks vs threads vs cores; concurrency vs parallelism;
Waker/reactor wake-then-repoll vs Node callbacks; `.await` unwraps a future's value;
extractors (`Path`, `Form`); newtype/tuple structs; `#[derive(Deserialize)]` / serde;
`#[tokio::test]`; 400 vs 422 semantics.

**Added this session:** `mod` (declares a module, bare name) vs `use`/paths (`crate::`/`zero2prod::`,
navigate an existing tree) — and that `crate::` is relative to the *crate*, not the file; the
`config` crate as a gather-merge-deserialize pipeline (sources → one nested key tree → `Settings`);
why env-var names need `__` (flat name → nested struct slot) and `APP_` (namespace filter); config
file (shared) vs env (per-dev secret) split; URL percent-encoding (`@`→`%40`) & `PgConnectOptions`
vs URL strings; **can't move a field out of `&self`** (`&self.host`, not `self.host` — borrow ≠
ownership; container-ref lets you borrow fields, never move them); axum `Router<S>` type-state &
`.with_state` (paying the state "debt" → `Router<()>`); body-consuming extractor (`Form`) must come
last; `sqlx::query!` compile-time SQL checking (connects to DB via `DATABASE_URL` at build time);
`PgPool` clone = cheap `Arc` handle.

**Added 2026-06-21 session:** connection pools (reuse + cap; N kept-open connections; browser
keep-alive/per-host pools as the same pattern); a connection = an open TCP pipe (+ a Postgres
*process* on the far end), NOT a thread; Postgres uses process-per-connection for isolation/safety
(+ history) vs tokio's cheap-task model → the pool bridges high-scale app to limited-conn DB;
multi-threaded vs event-loop concurrency, and tokio = BOTH (small thread pool, event loop per
thread); the `.await` → park-task → reactor/`epoll` → Waker → re-poll cycle (request → server →
server-as-DB-client → back); move (give, receiver keeps it) vs borrow `&` (lend, momentary use) —
why `run`/`app` move the pool but `execute(&pool)` borrows; per-request state is a cheap `clone`
(Arc handle) so every handler "owns" its own handle to the one shared pool; connection-string (URL,
must percent-encode) vs `PgConnectOptions` (struct of fields, nothing to escape); shell command
anatomy (program + flags + `|` pipe); "use SQL to generate SQL, then pipe-execute it"; shebang +
`chmod +x` + `set -euo pipefail`.

**Added 2026-06-23 session:** the universal backend skeleton (6 slots: telemetry→config→state→router
→bind→serve; `main`=composition, `lib`=pieces); config layering = weak→strong source merge (last
wins); the three-bin rule (same-everywhere / differs-not-secret / differs-and-secret); `src/configuration.rs`
(reader code) vs `configuration/` folder (data read); `'static` as a concrete lifetime ("lives the whole
program") & why `&str` needs a lifetime in return position; own-vs-borrow decision is only *real* when
returning text that already exists permanently (literal→borrow `&'static str`; `format!`-built→own
`String`); `pub` = cross-module visibility = a commitment, default private; `TryFrom`/`TryInto` as a
fallible gate from untrusted-open-set → trusted-enum at the boundary; `expect`/panic (unrecoverable
startup fault) vs `?` (declared, handleable error channel); safe-default policy (dangerous env needs
explicit opt-in); config is declarative & ahead-of-time (platform injects env vars every boot, not SSH);
migrations = ordered run-once schema scripts that travel (code+schema travel, data never does); local
vs prod = two separate DBs.

**Added 2026-06-23 (telemetry + async deep dive):** telemetry = high-quality data the server emits
so you can reconstruct what happened in prod *after* it breaks (observability). **Facade pattern**:
`log`/`tracing` = the microphones (emit, blind); `env_logger`/`tracing-subscriber` = the ONE speaker
you install at startup (`init()`/`set_global_default`, once). Libraries emit; the app chooses the
listener → you inherit all deps' logs for free (the "flood"). One shared `log` crate (Cargo unifies
compatible `0.4.x`) = one global slot = why the flood works. `RUST_LOG` = volume knob (level filter,
per-target). **Logs = point-in-time dots; spans = boxes** (interval + context inherited by everything
inside) → spans solve the orphaned-interleaved-log problem. **Deep async machinery (the real payoff):**
`async fn` is LAZY — calling it BUILDS a state-machine struct (`state: enum` = "which `.await` am I
parked at"; same role as a counter field) and runs NOTHING; `.await` is what drives it (polls it);
no `.await`/spawn → body never runs. **A future = a struct + a `poll(&mut self, cx) -> Poll`** method;
`poll` returns `Ready(v)` or `Pending`. **State survives between polls only in the struct** (stack is
gone once poll returns) → locals crossing an `.await` become enum-variant fields. **Pending = go fully
dormant** (zero CPU), re-polled only when the **Waker** (stashed via `cx`) fires from the reactor →
task back on ready queue. **Who calls poll:** runtime (`block_on`/worker) drives the TOP; each `.await`
polls the next one down (nested dolls). **Thread = saved bookmark (stack+registers+IP) the OS swaps
onto a core; core executes.** Single core can run many threads (time-slice = concurrency); many cores
= parallelism. tokio ≈ several Node event loops (≈1 worker thread/core), each driving thousands of
cheap tasks. **The `.enter()`-across-`.await` bug:** the guard is a local that crosses the await → it's
parked in the future struct (NOT dropped at suspension); its `Drop` is what clears the per-thread
"current span" record, so that record goes STALE → the next task polled on that thread (or the task's
own resumed events on a stolen thread) get the wrong span context. Broken on every runtime flavor.
Fix = `#[tracing::instrument]` (enters on each poll, exits before each poll returns).

**Added 2026-06-23 (telemetry #2 — the why, slowed way down):** the full motivation chain (prod
breaks unwatched → recording not live-feed → stdout captured by platform → search later → JSON
needed); deployment = app runs on an unwatched machine in a far rack, "can't see" = not present in
time, not lack of access; SSH-and-watch is a live feed with no DVR (only shows the present, the past
already scrolled); separation of concerns: app writes stdout (dumb/portable), platform routes to
storage (Twelve-Factor "logs as a stream"); structured JSON = named fields a machine indexes like DB
columns vs plain text = substring/grep hunt; we ditched the `fmt::init()` SHORTCUT, not the crate;
subscriber = stack of layers via `.with()` (`SubscriberExt` extension trait); build/install split &
why it makes tests possible; `impl Trait` return (un-spellable generic) + why `Send + Sync`;
`set_global_default` two-module trap (`dispatcher` wants `Dispatch`, `subscriber` wants `Subscriber`);
`MakeWriter` (pass `stdout` the *function*, not `stdout()`) → enables `stdout`↔`sink` toggle;
`String` vs `&str` at call sites (`.into()`/`String::from`/`.to_string()`; own because the layer keeps
the name); `LazyLock` = run-closure-once-on-first-touch (the upcoming test init-once tool).

## Confidence note
User is a few weeks into Rust — comfortable with logic, "will keep tripping." Confidence not
high yet but steadily moving forward. Keep deep-dive explanations; keep letting them write the code.
