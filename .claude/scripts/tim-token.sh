#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Prints a GitHub installation access token for the "Tim the Enchanter Bot"
# GitHub App (app ID 5515695), scoped to Signalare/Chalkboard. Tokens are valid for one hour
# and cached until 10 minutes before expiry.
#
# Usage:  GH_TOKEN="$(.claude/scripts/tim-token.sh)" gh issue list
#
# Configuration (outside the repository, never commit it):
#   ~/.config/signalare/tim-the-enchanter/app.env   TIM_APP_ID=<numeric app id>
#   ~/.config/signalare/tim-the-enchanter/private-key.pem

set -euo pipefail

CONFIG_DIR="${TIM_CONFIG_DIR:-$HOME/.config/signalare/tim-the-enchanter}"
CACHE_FILE="${XDG_CACHE_HOME:-$HOME/.cache}/signalare/tim-the-enchanter.token"
OWNER="Signalare"
REPO="Chalkboard"
API="https://api.github.com"

# shellcheck source=/dev/null
source "$CONFIG_DIR/app.env"
KEY="$CONFIG_DIR/private-key.pem"
: "${TIM_APP_ID:?TIM_APP_ID missing in $CONFIG_DIR/app.env}"
[[ -r "$KEY" ]] || { echo "tim-token: cannot read $KEY" >&2; exit 1; }

now=$(date +%s)

# Reuse a cached token while it has more than 10 minutes left.
if [[ -r "$CACHE_FILE" ]]; then
  read -r cached_exp cached_token < "$CACHE_FILE" || true
  if [[ -n "${cached_token:-}" && "${cached_exp:-0}" -gt $((now + 600)) ]]; then
    printf '%s\n' "$cached_token"
    exit 0
  fi
fi

b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

header=$(printf '{"alg":"RS256","typ":"JWT"}' | b64url)
payload=$(printf '{"iat":%d,"exp":%d,"iss":"%s"}' $((now - 60)) $((now + 540)) "$TIM_APP_ID" | b64url)
signature=$(printf '%s.%s' "$header" "$payload" | openssl dgst -sha256 -sign "$KEY" -binary | b64url)
jwt="$header.$payload.$signature"

gh_app_api() {
  curl -fsS -X "$1" -H "Authorization: Bearer $jwt" \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" "${@:3}" "$API$2"
}

json_field() { python3 -I -c 'import json,sys; print(json.load(sys.stdin)[sys.argv[1]])' "$1"; }

installation_id=$(gh_app_api GET "/repos/$OWNER/$REPO/installation" | json_field id)
response=$(gh_app_api POST "/app/installations/$installation_id/access_tokens" \
  -d "{\"repositories\":[\"$REPO\"]}")
token=$(json_field token <<< "$response")

# Tokens live for one hour; cache with a conservative expiry.
mkdir -p "$(dirname "$CACHE_FILE")"
(umask 077 && printf '%d %s\n' $((now + 3600)) "$token" > "$CACHE_FILE")

printf '%s\n' "$token"
