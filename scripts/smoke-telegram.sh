#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${TELEGRAM_BOT_TOKEN:-}" ]]; then
  echo "SKIP: TELEGRAM_BOT_TOKEN is not set"
  exit 0
fi

api="https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}"
response="$(curl --fail --silent --show-error --max-time 20 -X POST "${api}/getMe")"
if ! jq -e '.ok == true and .result.id != null' >/dev/null <<<"${response}"; then
  echo "Telegram getMe smoke failed" >&2
  exit 1
fi

echo "Telegram getMe smoke passed"

if [[ "${TELEGRAM_SMOKE_SEND:-false}" == "true" ]]; then
  if [[ -z "${TELEGRAM_CHAT_ID:-}" ]]; then
    echo "TELEGRAM_CHAT_ID is required when TELEGRAM_SMOKE_SEND=true" >&2
    exit 1
  fi
  send_response="$(curl --fail --silent --show-error --max-time 20 \
    -X POST "${api}/sendMessage" \
    -H 'Content-Type: application/json' \
    --data "$(jq -cn --arg chat_id "${TELEGRAM_CHAT_ID}" --arg text "my-media-sub Telegram sandbox smoke $(date -u +%FT%TZ)" '{chat_id:$chat_id,text:$text,disable_notification:true}')")"
  jq -e '.ok == true and .result.message_id != null' >/dev/null <<<"${send_response}"
  echo "Telegram sendMessage smoke passed"
fi
