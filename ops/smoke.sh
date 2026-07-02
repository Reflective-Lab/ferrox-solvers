#!/usr/bin/env bash
# Smoke-test the deployed ferrox-server.
#
# ─── Canonical flow: Cloud Shell ──────────────────────────────────────────────
# The ferrox-server Cloud Run service is `ingress=internal`, so callers must
# reach it from inside the VPC. The most reliable + zero-setup path is Cloud
# Shell in the browser. It runs in Google's network, ships grpcurl, and uses
# your authenticated identity by default.
#
#   1. Open https://shell.cloud.google.com  (your project = reflective-labs)
#   2. git clone --depth=1 -b next https://github.com/Reflective-Lab/ferrox-solvers.git
#   3. cd ferrox-solvers
#   4. ops/smoke.sh https://ferrox-server-640630843925.europe-west1.run.app
#
# The server registers grpc.reflection.v1.ServerReflection, so no -proto
# flag is needed — grpcurl introspects the schema over the wire.
#
# ─── Alternative: local dev with cloud-run-proxy ──────────────────────────────
# Requires the cloud-run-proxy binary on PATH (Homebrew gcloud ships it under
# /opt/homebrew/share/google-cloud-sdk/bin/ but its h2c local listener has
# been observed to hang on some macOS versions). If you have a working proxy:
#
#   gcloud run services proxy ferrox-server --project=reflective-labs \
#       --region=europe-west1 --port=9090 &
#   ops/smoke.sh http://localhost:9090
#
# If the proxy hangs, fall back to Cloud Shell above, or temporarily flip
# `--ingress=all` (auth still required via ID token) for a 3-minute window:
#
#   gcloud run services update ferrox-server --project=reflective-labs \
#       --region=europe-west1 --ingress=all
#   TOKEN=$(gcloud auth print-identity-token)
#   grpcurl -H "authorization: Bearer ${TOKEN}" \
#       ferrox-server-640630843925.europe-west1.run.app:443 list
#   # ...smoke...
#   gcloud run services update ferrox-server --project=reflective-labs \
#       --region=europe-west1 --ingress=internal
#
# ─── grpcurl install ──────────────────────────────────────────────────────────
# Cloud Shell: pre-installed.
# macOS:      brew install grpcurl
# Linux:      see https://github.com/fullstorydev/grpcurl/releases
set -euo pipefail

URL="${1:?usage: smoke.sh <ferrox-server-url>}"
HOST_PORT="${URL#https://}"
HOST_PORT="${HOST_PORT#http://}"
HOST_PORT="${HOST_PORT%/}"
SCHEME_FLAGS=""
if [[ "$URL" == http://* ]]; then
    SCHEME_FLAGS="-plaintext"
fi

echo "── 1/4: grpc.health.v1.Health/Check should return SERVING ──"
RESP=$(grpcurl $SCHEME_FLAGS -d '{"service": ""}' "$HOST_PORT" grpc.health.v1.Health/Check)
echo "$RESP"
echo "$RESP" | grep -q '"status": "SERVING"' \
    || { echo "FAIL: expected SERVING"; exit 1; }
echo "ok"
echo

echo "── 2/4: SolveCp WITHOUT x-converge-app → INVALID_ARGUMENT ──"
if grpcurl $SCHEME_FLAGS -d '{}' "$HOST_PORT" \
       ferrox.v1.FerroxSolver/SolveCp 2>&1 \
       | grep -q "InvalidArgument"; then
    echo "ok"
else
    echo "FAIL: expected InvalidArgument for missing tenant header"
    exit 1
fi
echo

echo "── 3/4: SolveCp WITH unknown tenant → PERMISSION_DENIED ──"
if grpcurl $SCHEME_FLAGS -H 'x-converge-app: nope-not-real' \
       -d '{}' "$HOST_PORT" \
       ferrox.v1.FerroxSolver/SolveCp 2>&1 \
       | grep -q "PermissionDenied"; then
    echo "ok"
else
    echo "FAIL: expected PermissionDenied for unknown tenant"
    exit 1
fi
echo

echo "── 4/4: SolveCp WITH quorum-sense + minimal valid CP problem ──"
# Trivial CP: maximize x subject to 0 ≤ x ≤ 1. Optimal x=1.
RESP=$(grpcurl $SCHEME_FLAGS \
    -H 'x-converge-app: quorum-sense' \
    -d '{
      "problem": {
        "variables": [{"name": "x", "lb": 0, "ub": 1, "is_bool": false}],
        "objective": {"sense": "maximize", "linear": {"terms": [{"var": "x", "coeff": 1}], "rhs": 0}}
      },
      "time_limit_sec": 5
    }' \
    "$HOST_PORT" ferrox.v1.FerroxSolver/SolveCp)
echo "$RESP"
# Accept any non-error response — the exact shape depends on the solver
# version, but if we got JSON back the solver round-tripped.
echo "$RESP" | grep -q '"status"' \
    || { echo "FAIL: expected a structured response"; exit 1; }
echo "ok"
echo

echo "── ALL 4 SMOKE CHECKS PASSED ──"
