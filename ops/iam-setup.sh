#!/usr/bin/env bash
# Idempotent: creates the ferrox-server SA and grants minimal IAM.
# Re-running is safe — SA creation is checked first, role grants are
# additive.
set -euo pipefail

PROJECT_ID="${PROJECT_ID:-reflective-labs}"
SA_NAME="ferrox-server"
SA_EMAIL="${SA_NAME}@${PROJECT_ID}.iam.gserviceaccount.com"

# Create SA if it doesn't exist.
if ! gcloud iam service-accounts describe "$SA_EMAIL" \
        --project="$PROJECT_ID" >/dev/null 2>&1; then
    echo ">>> creating SA $SA_EMAIL"
    gcloud iam service-accounts create "$SA_NAME" \
        --project="$PROJECT_ID" \
        --display-name="ferrox-server (Cloud Run runtime)" \
        --description="Minimal-IAM SA for the ferrox-server Cloud Run service (M1 of the gRPC suggestor pattern). No data-plane permissions — solver service is pure compute."
else
    echo ">>> SA $SA_EMAIL already exists"
fi

# Minimal IAM:
#   roles/logging.logWriter   — emit structured logs to Cloud Logging
#   roles/monitoring.metricWriter — emit metrics
# NOT GRANTED: any Firestore/Storage/Secret Manager — solver owns no data.
echo ">>> granting roles/logging.logWriter"
gcloud projects add-iam-policy-binding "$PROJECT_ID" \
    --member="serviceAccount:${SA_EMAIL}" \
    --role="roles/logging.logWriter" \
    --condition=None >/dev/null

echo ">>> granting roles/monitoring.metricWriter"
gcloud projects add-iam-policy-binding "$PROJECT_ID" \
    --member="serviceAccount:${SA_EMAIL}" \
    --role="roles/monitoring.metricWriter" \
    --condition=None >/dev/null

echo ">>> done. SA: $SA_EMAIL"
