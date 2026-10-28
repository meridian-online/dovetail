#!/usr/bin/env bash
# Fetch FineType's DuckDB extension and its model, for the finetype version
# dovetail pins, so `dovetail_core::finetype_ext::load` can type columns with
# FineType's profile instead of the floor.
#
#   scripts/fetch-finetype.sh DEST_DIR
#
# Writes, under DEST_DIR, replacing any earlier copies of these:
#
#   finetype.duckdb_extension          the extension attached to the release,
#                                      renamed: DuckDB derives the entry point
#                                      from the file name
#   finetype.duckdb_extension.source   which release asset it is, and its sha256
#   model/                             the model the release names (the whole
#                                      directory is replaced)
#
# and prints the two `export` lines that point dovetail at them.
#
# THE TAG is the one the root Cargo.toml pins finetype-core to, which is what
# `finetype_tag()` embeds at build time; the extension is the release asset
# for this machine's platform, checked against the `.sha256` published beside
# it. The release's `finetype-model.json` names the model.
#
# THE MODEL comes from FineType's model registry on HuggingFace, at a pinned
# revision, because the release does not attach the weights. Each file is
# checked against the digest the registry lists for it at that revision (sha256
# for LFS files, the git blob id for the rest). That catches a truncated or
# substituted transfer; it is not an independent attestation, since the listing
# and the files come from the same server.
#
# The model directory's layout is the one FineType's extension loads from
# FINETYPE_MODEL_DIR: the model's own files, with the registry's shared
# `model2vec/` encoder beside them, and `validation_snapshot.json`. The registry
# revision below does not carry the snapshot, so it is taken from the finetype
# repository at the pinned tag (`models/<model>/validation_snapshot.json`).
#
# Needs curl, python3, and shasum or sha256sum.
set -euo pipefail

# The model registry revision Brightfield pins in its packaging/finetype-pin.env
# (FINETYPE_MODEL_REVISION). It carries the model v0.6.64 names, m2v8m-s43.
MODEL_REVISION=94cda10a60c3e2eedb831d295c1db091836540c2
MODEL_REPO="meridian-online/finetype-model"
FINETYPE_REPO="meridian-online/finetype"

fail() { echo "fetch-finetype: $*" >&2; exit 1; }

DEST="${1:?usage: scripts/fetch-finetype.sh DEST_DIR (replaces DEST_DIR/model and DEST_DIR/finetype.duckdb_extension)}"
case "$DEST" in
  ""|"/") fail "refusing to use '${DEST}' as the destination" ;;
esac
command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v python3 >/dev/null 2>&1 || fail "python3 is required to read the release metadata"

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
MANIFEST="${HERE}/../Cargo.toml"
TAG=$(sed -n 's/^finetype-core = .*tag = "\([^"]*\)".*/\1/p' "$MANIFEST")
[ -n "$TAG" ] || fail "could not read finetype-core's tag from ${MANIFEST}"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) TARGET=aarch64-apple-darwin ;;
  Darwin-x86_64) TARGET=x86_64-apple-darwin ;;
  Linux-aarch64|Linux-arm64) TARGET=aarch64-unknown-linux-gnu ;;
  Linux-x86_64) TARGET=x86_64-unknown-linux-gnu ;;
  *) fail "no FineType release extension for $(uname -s) $(uname -m)" ;;
esac

RELEASE="https://github.com/${FINETYPE_REPO}/releases/download/${TAG}"
REGISTRY="https://huggingface.co"
EXT_ASSET="finetype-${TAG}-${TARGET}.duckdb_extension"

digest_of() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

fetch() { # fetch URL OUT
  curl -sSfL --connect-timeout 20 --max-time 600 --retry 3 --retry-delay 2 -o "$2" "$1" || fail "could not fetch $1"
}

WORK=$(mktemp -d "${TMPDIR:-/tmp}/dovetail-finetype-fetch.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

# get_release ASSET: the asset and its .sha256 sidecar, refused unless they agree.
get_release() {
  fetch "${RELEASE}/$1" "${WORK}/$1"
  fetch "${RELEASE}/$1.sha256" "${WORK}/$1.sha256"
  local declared actual
  declared=$(awk 'NR==1 {print $1}' "${WORK}/$1.sha256")
  actual=$(digest_of "${WORK}/$1")
  [ "$declared" = "$actual" ] || fail "$1 does not match its published checksum:
  declared ${declared}
  actual   ${actual}"
  echo "   $1  ${actual}" >&2
}

echo "== FineType ${TAG} for ${TARGET}" >&2
get_release "$EXT_ASSET"
get_release finetype-model.json

MODEL=$(python3 -I - "${WORK}/finetype-model.json" "$TAG" <<'PY'
import json, sys
meta = json.load(open(sys.argv[1]))
if meta.get("tag") != sys.argv[2]:
    sys.exit(f"finetype-model.json was built for {meta.get('tag')!r}, not {sys.argv[2]!r}")
model = meta.get("model") or ""
if not model or "/" in model or model in (".", ".."):
    sys.exit(f"finetype-model.json names model {model!r}, which is not a directory name")
print(model)
PY
) || fail "could not read the model name from finetype-model.json"

echo "== model ${MODEL} at ${MODEL_REVISION}" >&2
for prefix in "$MODEL" model2vec; do
  fetch "${REGISTRY}/api/models/${MODEL_REPO}/tree/${MODEL_REVISION}/${prefix}?recursive=true" \
    "${WORK}/tree-${prefix}.json"
done

# One line per file: local path, registry path, digest kind, digest. The shared
# encoder goes first so a model carrying its own model2vec/ overrides it.
python3 -I - "$MODEL" "${WORK}/tree-model2vec.json" "${WORK}/tree-${MODEL}.json" \
  > "${WORK}/plan.tsv" <<'PY' || fail "could not plan the model download"
import json, sys
model, shared, own = sys.argv[1:4]
plan = {}
for listing, strip in ((shared, ""), (own, model + "/")):
    for entry in json.load(open(listing)):
        if entry.get("type") != "file":
            continue
        remote = entry["path"]
        local = remote[len(strip):] if strip else remote
        lfs = entry.get("lfs") or {}
        kind, digest = ("sha256", lfs["oid"]) if lfs.get("oid") else ("gitblob", entry["oid"])
        plan[local] = (remote, kind, digest)
for required in ("config.json", "label_map.json", "model.safetensors"):
    if required not in plan:
        sys.exit(f"the revision publishes no {model}/{required}")
for local in sorted(plan):
    print("\t".join((local,) + plan[local]))
PY

mkdir -p "${WORK}/model"
while IFS="$(printf '\t')" read -r local remote _kind _digest; do
  mkdir -p "$(dirname "${WORK}/model/${local}")"
  fetch "${REGISTRY}/${MODEL_REPO}/resolve/${MODEL_REVISION}/${remote}" "${WORK}/model/${local}"
done < "${WORK}/plan.tsv"

python3 -I - "${WORK}/plan.tsv" "${WORK}/model" <<'PY' || fail "the model files do not match the registry"
import hashlib, os, sys
plan, root = sys.argv[1:3]
for line in open(plan):
    local, _remote, kind, want = line.rstrip("\n").split("\t")
    data = open(os.path.join(root, local), "rb").read()
    got = (hashlib.sha256(data).hexdigest() if kind == "sha256"
           else hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest())
    if got != want:
        sys.exit(f"{local}: the registry declares {kind} {want}, the download is {got}")
PY

if [ ! -e "${WORK}/model/validation_snapshot.json" ]; then
  fetch "https://raw.githubusercontent.com/${FINETYPE_REPO}/${TAG}/models/${MODEL}/validation_snapshot.json" \
    "${WORK}/model/validation_snapshot.json"
fi
echo "   $(find "${WORK}/model" -type f | wc -l | tr -d ' ') model files" >&2

# Assembled only once every byte has been checked.
mkdir -p "$DEST"
DEST=$(cd "$DEST" && pwd)
# Each file is written beside its target and renamed over it, so a re-run
# replaces the file (a new inode) rather than rewriting it in place: rewriting a
# library some process has mapped can crash that process, and on macOS leaves
# a stale code signature that gets the next LOAD killed. The model is removed
# and copied afresh, which gives new inodes too.
cp "${WORK}/${EXT_ASSET}" "${DEST}/.finetype.duckdb_extension.partial"
mv -f "${DEST}/.finetype.duckdb_extension.partial" "${DEST}/finetype.duckdb_extension"
printf '%s  %s\n' "$(digest_of "${WORK}/${EXT_ASSET}")" "${RELEASE}/${EXT_ASSET}" \
  > "${DEST}/.finetype.duckdb_extension.source.partial"
mv -f "${DEST}/.finetype.duckdb_extension.source.partial" "${DEST}/finetype.duckdb_extension.source"
# Earlier versions of this script kept the release asset's own name.
rm -f "${DEST}"/finetype-*-*.duckdb_extension
rm -rf "${DEST:?}/model"
cp -R "${WORK}/model" "${DEST}/model"
echo "   ${EXT_ASSET} -> ${DEST}/finetype.duckdb_extension" >&2

printf 'export DOVETAIL_FINETYPE_EXTENSION=%q\n' "${DEST}/finetype.duckdb_extension"
printf 'export FINETYPE_MODEL_DIR=%q\n' "${DEST}/model"
