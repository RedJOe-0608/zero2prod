# WHY THREE STAGES?
# A Docker image is a snapshot of a filesystem (the userspace — /bin, /usr, /lib, your files).
# Containers share the host's kernel; each gets its own isolated userspace.
# Only the LAST stage's filesystem becomes the final image. Earlier stages are discarded.
#
# Problem with a single stage: the build environment (Rust toolchain, lld, clang, all of target/)
# ends up in the image that runs in production — even though NONE of it is needed at runtime.
# A compiled Rust binary is self-contained machine code. The compiler's job is done.
#
# Fix: three stages:
#   planner  → scans project, emits recipe.json (dependency manifest)
#   builder  → compiles deps (cached layer), then compiles your code
#   runtime  → tiny image, just the binary

# WHY LAYER ORDER MATTERS:
# Each RUN/COPY instruction creates a layer. Docker caches each layer by its content.
# If a layer changes, ALL layers after it are invalidated and must rerun — layers before it are fine.
# Rule: put things that change RARELY near the top; things that change OFTEN near the bottom.

# ----------------------------
# Stage 1: planner
# ----------------------------
# cargo-chef is pre-installed in this image (it's rust:1.96.0 + cargo-chef on top).
# prepare: reads Cargo.toml/Cargo.lock + discovers all targets (lib, bin, tests) →
# emits recipe.json, a complete dependency manifest cargo-chef uses to build dummy source files.
FROM lukemathwalker/cargo-chef:latest-rust-1.96.0 AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ----------------------------
# Stage 2: builder
# ----------------------------
FROM lukemathwalker/cargo-chef:latest-rust-1.96.0 AS builder
WORKDIR /app

# lld  = a fast linker. The linker is the final step: it stitches all compiled object files
#         into one binary. The default Linux linker (ld) is slow; lld is much faster.
# clang = a C compiler. Some crates have C code inside them (via build.rs scripts) that must
#         be compiled as part of cargo build. Your Mac already has clang (via Xcode CLT);
#         this minimal Debian image does not, so we install it explicitly.
# Both are BUILD-TIME tools only. Not needed to run the binary.
RUN apt-get update && apt-get install lld clang -y

# cook: reads recipe.json → creates dummy src files → runs cargo build → compiles ALL deps.
# This layer is cached as long as recipe.json doesn't change (i.e. Cargo.toml/Cargo.lock unchanged).
# Change a dependency → cache miss, recompile deps. Change your own code → cache hit, skip this. ✅
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Now copy real source and build only YOUR code.
# deps are already compiled above — cargo finds them in target/ and skips recompiling them.
# COPY . . is used (not just src/) because cargo needs Cargo.toml, Cargo.lock, .sqlx/, tests/ etc.
# The extra files land in the builder filesystem which is discarded anyway — no size cost.
COPY . .
# SQLX_OFFLINE=true makes the query! macros read from .sqlx/ instead of a live database.
ENV SQLX_OFFLINE true
RUN cargo build --release

# ----------------------------
# Stage 3: runtime
# ----------------------------
# debian:bookworm-slim = a minimal Debian userspace. No Rust. No build tools.
# Same Linux family as the builder (both Debian-based) so library compatibility is guaranteed.
# "slim" = most optional tools stripped out. ~80MB vs rust:1.96.0's ~1GB+.
# Contrast with Alpine (~5MB) which uses musl instead of glibc — can cause compatibility issues
# with binaries compiled against glibc (which Rust on Linux does by default).
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# Our binary is NOT 100% self-contained — most deps are statically linked (baked in),
# but OpenSSL is dynamically linked: the binary contains a note saying "load libssl.so at startup."
# If that .so file isn't on the filesystem, the process crashes before main() even runs.
# Why dynamic? OpenSSL gets security patches often. Dynamic linking means you patch OpenSSL
# system-wide (apt-get upgrade) and every program gets the fix — no recompile needed.
#
# ca-certificates = a file OpenSSL reads at runtime (/etc/ssl/certs/ca-certificates.crt).
# It contains root certificates from trusted CAs (Let's Encrypt, DigiCert, etc.).
# Without it, TLS certificate verification fails — including our Postgres TLS connection.
#
# All cleanup chained in ONE RUN so it's one layer — deletions in a later layer don't reduce
# image size because the earlier layer's snapshot still contains the files.
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

# Pull ONLY the compiled binary out of the builder stage. The Rust toolchain, lld, clang,
# and the entire target/ directory (gigabytes of build artifacts) stay in stage 2 and are discarded.
COPY --from=builder /app/target/release/zero2prod zero2prod

# The binary calls current_dir().join("configuration") at startup to read the yaml files.
# Those files are not compiled into the binary — they must exist on the runtime filesystem.
COPY configuration configuration

# Without this, the app defaults to APP_ENVIRONMENT=local → binds 127.0.0.1 (loopback only,
# unreachable from outside the container) and points at localhost for the DB (wrong host).
# production.yaml sets host: 0.0.0.0 (all interfaces → reachable) and the real DB host.
ENV APP_ENVIRONMENT production

ENTRYPOINT ["./zero2prod"]
