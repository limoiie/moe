#!/usr/bin/env bash
# Regenerate the embedded model-catalog snapshot from models.dev.
#
# - Source:  https://models.dev/api.json (MIT; see docs/research/ai-model-catalogs.md)
# - Output:  crates/moe-platform/data/models-catalog.json — reduced subset:
#            the official providers, and only the model fields the picker needs.
# - Run by:  .github/workflows/refresh-models-catalog.yml (daily PR); safe to run
#            locally. The runtime app additionally refreshes the full catalog over
#            HTTP with an ETag check (docs/issues/MOE-0025-model-catalog-refresh.md);
#            this snapshot is the offline baseline embedded in the binary.
#            Set MOE_MODELS_SRC to a local api.json path to regenerate offline.
set -euo pipefail

cd "$(dirname "$0")/.."

SRC_URL="${MOE_MODELS_SRC:-https://models.dev/api.json}"
case "${SRC_URL}" in
  http://* | https://* | file://*) ;;
  *) SRC_URL="file://${SRC_URL}" ;;
esac
OUT_FILE="crates/moe-platform/data/models-catalog.json"
USER_AGENT="moe-model-catalog/0.1 (+https://github.com/)"

# Providers shipped in the offline baseline (first-party labs, aggregators, local).
# The runtime refresh expands this with every provider models.dev carries.
# Edit freely — the script fails loudly if an id disappears upstream.
PROVIDERS=(
  alibaba anthropic cohere deepseek google groq lmstudio meta mistral
  moonshotai openai openrouter perplexity togetherai xai zai zhipuai
)

tmp="$(mktemp)"
trap 'rm -f "$tmp" "${OUT_FILE}.tmp"' EXIT

echo "fetching ${SRC_URL}"
curl -fsSL --retry 3 --retry-delay 2 --max-time 180 -A "${USER_AGENT}" "${SRC_URL}" -o "${tmp}"
jq -e 'type == "object"' "${tmp}" >/dev/null

missing=()
for p in "${PROVIDERS[@]}"; do
  jq -e --arg p "${p}" 'has($p)' "${tmp}" >/dev/null || missing+=("${p}")
done
if [ "${#missing[@]}" -gt 0 ]; then
  echo "error: provider id(s) missing from models.dev: ${missing[*]}" >&2
  exit 1
fi

provider_json="$(printf '%s\n' "${PROVIDERS[@]}" | jq -R . | jq -s .)"

mkdir -p "$(dirname "${OUT_FILE}")"
jq --argjson keep "${provider_json}" '
  def clean: with_entries(select(.value != null));
  def reduce_model: {
    id,
    name,
    attachment,
    reasoning,
    reasoning_options,
    interleaved,
    tool_call,
    structured_output,
    knowledge,
    release_date,
    last_updated,
    modalities,
    open_weights,
    limit,
    cost,
    status,
    canonical_model_id
  } | clean;
  with_entries(select(.key as $k | $keep | contains([$k])))
  | map_values(
      ({ id, name, env, api, doc, npm } | clean)
      + {
          models: (
            (.models // {})
            | map_values(reduce_model)
            | to_entries
            | sort_by(.key)
            | from_entries
          )
        }
    )
  | to_entries
  | sort_by(.key)
  | from_entries
' "${tmp}" > "${OUT_FILE}.tmp"

provider_count="$(jq 'length' "${OUT_FILE}.tmp")"
model_count="$(jq '[.[] | .models | length] | add' "${OUT_FILE}.tmp")"
if [ "${model_count}" -lt 100 ]; then
  echo "error: only ${model_count} models — models.dev schema changed?" >&2
  exit 1
fi

mv "${OUT_FILE}.tmp" "${OUT_FILE}"
echo "wrote ${OUT_FILE}: ${provider_count} providers, ${model_count} models, $(wc -c < "${OUT_FILE}" | tr -d ' ') bytes"
