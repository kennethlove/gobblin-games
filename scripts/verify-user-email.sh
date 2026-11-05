#!/usr/bin/env bash
# Set email_verified = true for a user — the production operator path (0oi).
# WHY: POST /dev/verify-email is gated to ENV=development; there is no admin
# UI yet (7mb). This wraps the surreal CLI against an existing deployment.
#
# Usage: scripts/verify-user-email.sh <email>
# Env (defaults from .env if sourced / set): SURREAL_URL, SURREAL_USER,
# SURREAL_PASS, APP_SURREAL_NAMESPACE, APP_SURREAL_DATABASE.
set -euo pipefail

EMAIL="${1:?usage: verify-user-email.sh <email>}"

# Load repo .env when present and not already exported (no `export` prefix in it).
if [[ -f "$(dirname "$0")/../.env" ]]; then
  set -a
  # shellcheck disable=SC1091
  . "$(dirname "$0")/../.env"
  set +a
fi

SURREAL_URL="${SURREAL_URL:-http://localhost:8000}"
SURREAL_USER="${SURREAL_USER:-root}"
SURREAL_PASS="${SURREAL_PASS:-root}"
APP_SURREAL_NAMESPACE="${APP_SURREAL_NAMESPACE:-gobblin-games}"
APP_SURREAL_DATABASE="${APP_SURREAL_DATABASE:-games}"

# Escape single quotes for the SurrealDB string literal.
SAFE_EMAIL="${EMAIL//\'/\'\'}"

RESULT=$(printf "UPDATE user SET email_verified = true WHERE email = '%s' RETURN AFTER;\n" "$SAFE_EMAIL" \
  | surreal sql -e "$SURREAL_URL" -u "$SURREAL_USER" -p "$SURREAL_PASS" \
      --ns "$APP_SURREAL_NAMESPACE" --db "$APP_SURREAL_DATABASE" 2>&1)

if [[ "$RESULT" == *"["* ]]; then
  echo "OK: $EMAIL"
  echo "$RESULT" | grep -o '\[\[.*\]\]' || true
else
  echo "FAILED (is surreal reachable at $SURREAL_URL?):" >&2
  echo "$RESULT" >&2
  exit 1
fi
