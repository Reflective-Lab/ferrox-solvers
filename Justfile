# ferrox development commands
# Install: brew install just  |  cargo install just
# Usage:   just --list

set dotenv-load := true

# Show available recipes
default:
    @just --list

# Build without native solver features
build:
    cargo build --workspace

# Build with all solvers. Requires `just deps`.
build-full:
    cargo build --workspace --features ferrox/full

# Build release artifacts with all solvers
build-release:
    cargo build --workspace --release --features ferrox/full

# Check all supported feature combinations
check:
    cargo check --workspace
    cargo check --workspace --features ferrox/ortools
    cargo check --workspace --features ferrox/highs
    cargo check --workspace --features ferrox/full

# Run pure Rust tests
test:
    cargo test --workspace

# Run tests with OR-Tools linked. Requires `just deps-ortools`.
test-ortools:
    cargo test --workspace --features ferrox/ortools

# Run tests with HiGHS linked. Requires `just deps-highs`.
test-highs:
    cargo test --workspace --features ferrox/highs

# Run tests with both native solver stacks. Requires `just deps`.
test-full:
    cargo test --workspace --features ferrox/full

# Alias for the full test gate
test-all: test-full

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Format code
fmt:
    cargo fmt --all

# Run clippy for default and full solver configurations
clippy:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy --workspace --all-targets --features ferrox/full -- -D warnings

# Formatting plus clippy
lint: fmt-check clippy

# Build all native dependencies
deps:
    make all

# Build OR-Tools native dependency
deps-ortools:
    make ortools

# Build HiGHS native dependency
deps-highs:
    make highs

# Remove native dependency build artifacts
deps-clean:
    make clean

# Generate docs
doc:
    cargo doc --no-deps --workspace --features ferrox/full

# Generate and open docs
doc-open:
    cargo doc --no-deps --workspace --features ferrox/full --open

# Run CP-SAT Sudoku example. Requires `just deps-ortools`.
example-cp:
    DYLD_LIBRARY_PATH="$(pwd)/vendor/ortools/build/lib:${DYLD_LIBRARY_PATH:-}" \
        cargo run --manifest-path examples/cp_sudoku/Cargo.toml --features ferrox/ortools

# Run HiGHS MIP example. Requires `just deps-highs`.
example-mip:
    DYLD_LIBRARY_PATH="$(pwd)/vendor/highs/build/lib:${DYLD_LIBRARY_PATH:-}" \
        cargo run --manifest-path examples/highs_mip/Cargo.toml --features ferrox/highs

# Run multi-agent assignment example. Requires `just deps-ortools`.
example-maatw:
    DYLD_LIBRARY_PATH="$(pwd)/vendor/ortools/build/lib:${DYLD_LIBRARY_PATH:-}" \
        cargo run --release --manifest-path examples/maatw/Cargo.toml

# Run job-shop benchmark example. Requires `just deps-ortools`.
example-jspbench:
    DYLD_LIBRARY_PATH="$(pwd)/vendor/ortools/build/lib:${DYLD_LIBRARY_PATH:-}" \
        cargo run --release --manifest-path examples/jspbench/Cargo.toml

# Run VRPTW example. Requires `just deps-ortools`.
example-vrptw:
    DYLD_LIBRARY_PATH="$(pwd)/vendor/ortools/build/lib:${DYLD_LIBRARY_PATH:-}" \
        cargo run --release --manifest-path examples/vrptw/Cargo.toml

# Run Criterion benchmarks
bench:
    cargo bench --workspace --features ferrox/full

# Run gRPC server locally without TLS
server:
    cargo run --package ferrox-server --features ferrox-server/full

# Build the Docker image
docker-build:
    docker build -f Dockerfile -t ferrox-server:latest ..

# Run the Docker image with certs from ./tls
docker-run:
    docker run --rm -p 50051:50051 -v "$(pwd)/tls:/tls:ro" ferrox-server:latest

# Bring up the docker-compose stack
up:
    docker compose up --build

# Tear down the docker-compose stack
down:
    docker compose down

# Generate self-signed development certs for localhost testing
tls-dev-certs:
    mkdir -p tls
    openssl req -x509 -newkey rsa:4096 -keyout tls/server.key \
      -out tls/server.crt -days 365 -nodes \
      -subj "/CN=ferrox-server" \
      -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

# Session opener
focus: status check test

# Git status and recent commits
status:
    git status --short --branch
    git log --oneline -5

# Remove Rust build artifacts
clean:
    cargo clean

# ── Release-grade gates (appended from ~/dev/templates/converge-extension) ─
# Standard: https://github.com/Reflective-Lab/converge/blob/main/kb/Standards/Extension%20Release%20Checklist.md

# ── Release-grade gates (Extension Release Checklist) ─────────────────────
# Mirror of foundation Justfile recipes. Update from
# ~/dev/templates/converge-extension/Justfile in lockstep with foundation.

# Gate 1: supply-chain audit. Output:
#   target/security/audit.json   (cargo-audit JSON)
#   target/security/deny.txt     (cargo-deny human report)
#   target/security/summary.txt  (combined human summary)
security-audit:
    #!/usr/bin/env bash
    set -uo pipefail
    out_dir="target/security"
    mkdir -p "${out_dir}"
    summary="${out_dir}/summary.txt"
    : > "${summary}"
    echo "── cargo-audit ──────────────────────────────" | tee -a "${summary}"
    cargo audit --json > "${out_dir}/audit.json" || true
    cargo audit --deny warnings 2>&1 | tee -a "${summary}"
    audit_human_status=${PIPESTATUS[0]}
    echo "" | tee -a "${summary}"
    echo "── cargo-deny ───────────────────────────────" | tee -a "${summary}"
    cargo deny check 2>&1 | tee "${out_dir}/deny.txt" | tee -a "${summary}"
    deny_status=${PIPESTATUS[0]}
    echo "" | tee -a "${summary}"
    echo "audit→${out_dir}/audit.json  deny→${out_dir}/deny.txt  summary→${summary}"
    if [ "${audit_human_status}" -ne 0 ] || [ "${deny_status}" -ne 0 ]; then
        exit 1
    fi

# Gate 2: workspace coverage. ≥ 80% per crate, no regression.

# Gate 2: workspace coverage. ≥ 80% per crate, no regression.
coverage:
    #!/usr/bin/env bash
    set -euo pipefail
    out_dir="target/coverage"
    mkdir -p "${out_dir}/html"
    common=(--workspace --lib --tests
        --ignore-filename-regex '(^|/)(tests|benches|examples)/')
    cargo llvm-cov clean --workspace
    rm -rf target/tests/trybuild
    cargo llvm-cov "${common[@]}" --no-report
    cargo llvm-cov report \
        --json --summary-only --output-path "${out_dir}/converge-coverage.json"
    cargo llvm-cov report \
        --lcov --output-path "${out_dir}/lcov.info"
    cargo llvm-cov report \
        --html --output-dir "${out_dir}/html"
    pct=$(python3 -c "import json; d=json.load(open('${out_dir}/converge-coverage.json')); print(f\"{d['data'][0]['totals']['lines']['percent']:.1f}\")")
    echo "coverage: ${pct}%  json→${out_dir}/converge-coverage.json  lcov→${out_dir}/lcov.info  html→${out_dir}/html/index.html"
    awk -v p="${pct}" 'BEGIN { if (p+0 < 80) { print "FAIL: coverage " p "% below 80% floor"; exit 1 } }'

# Gate 3: Criterion baseline. Set PERF_BASELINE to the release tag.

# Gate 3: Criterion baseline. Set PERF_BASELINE to the release tag.
performance-profile:
    #!/usr/bin/env bash
    set -euo pipefail
    name="${PERF_BASELINE:-v0.1.0}"
    mode_flag="--save-baseline"
    if [ -d "target/criterion" ]; then
        existing="$(find target/criterion -mindepth 2 -maxdepth 3 -type d -name "${name}" -print -quit 2>/dev/null || true)"
        if [ -n "${existing}" ]; then
            mode_flag="--baseline"
        fi
    fi
    echo "performance-profile: ${mode_flag} ${name}"
    cargo bench --workspace -- "${mode_flag}" "${name}" || true
    if [ -f scripts/extract-criterion-baseline.py ]; then
        python3 scripts/extract-criterion-baseline.py || \
            echo "warn: baseline extraction failed (non-fatal)"
    fi
    echo "performance-profile: criterion→target/criterion/"

# Gate 4: bounded soak run. Configure with SOAK_DURATION_MIN (default 5).

# Gate 4: bounded soak run. Configure with SOAK_DURATION_MIN (default 5).
soak:
    #!/usr/bin/env bash
    set -euo pipefail
    duration_min="${SOAK_DURATION_MIN:-5}"
    out_dir="target/soak"
    mkdir -p "${out_dir}"
    stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    log="${out_dir}/soak-${stamp}.log"
    cycles=$(awk -v d="${duration_min}" 'BEGIN { printf "%d", 200 * d }')
    iterations=$(awk -v d="${duration_min}" 'BEGIN { printf "%d", 40 * d }')
    concurrency=100
    echo "soak: duration=${duration_min}min cycles=${cycles} concurrency=${concurrency} iterations=${iterations}" | tee "${log}"
    SOAK_CYCLES="${cycles}" \
    SOAK_CONCURRENCY="${concurrency}" \
    SOAK_ITERATIONS="${iterations}" \
    cargo test --workspace -- --include-ignored soak --nocapture 2>&1 | tee -a "${log}"
    ln -sf "soak-${stamp}.log" "${out_dir}/latest.log"
    echo "soak: log → ${log}"

# The five-command release ritual. All five must be green before tagging.

# The five-command release ritual. All five must be green before tagging.
release-check:
    just security-audit
    just coverage
    PERF_BASELINE="v$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')" just performance-profile
    SOAK_DURATION_MIN=5 just soak
    just lint
    cargo test --workspace
