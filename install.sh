#!/usr/bin/env bash
# part-registry — one-line registry (data repo) setup, per ADR-038 §5.
#
#   curl -fsSL https://raw.githubusercontent.com/MorePET/part-registry/main/install.sh \
#     | bash -s -- <owner/repo> [bootstrap flags]
#
# Fetches the RELEASE-PINNED bootstrap (sha256-verified against the
# release's published checksum — same pin discipline as the gate, ADR-034
# §2) and runs tools/bootstrap-data-repo.sh, which seeds the registry:
# data files, contract, CODEOWNERS, the pr-check gate, the ADR-037
# anchor + bundle workflows, and (with --protect) branch protection.
#
# Env overrides:
#   PART_REGISTRY_CODE_REPO   tool repo slug   (default MorePET/part-registry)
#   PART_REGISTRY_RELEASE     release tag       (default: latest release)
#
# Requires: gh (authenticated), git, curl, tar, sha256sum.
set -euo pipefail

CODE_REPO="${PART_REGISTRY_CODE_REPO:-MorePET/part-registry}"
TAG="${PART_REGISTRY_RELEASE:-}"

err() { echo "install.sh: $*" >&2; exit 1; }
log() { echo "install.sh: $*" >&2; }

for cmd in gh git curl tar sha256sum; do
  command -v "$cmd" >/dev/null || err "missing required command: $cmd"
done
[[ $# -ge 1 ]] || err "usage: install.sh <owner/repo> [bootstrap flags] — see tools/bootstrap-data-repo.sh --help"

if [[ -z "$TAG" ]]; then
  TAG=$(gh release view --repo "$CODE_REPO" --json tagName -q .tagName 2>/dev/null) \
    || err "no releases found on ${CODE_REPO} — pass PART_REGISTRY_RELEASE=<tag>"
fi
log "using ${CODE_REPO} release ${TAG}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"

# Preferred path: the release-published bootstrap tarball, verified
# against its released checksum (releases ≥ the ADR-038 release.yml).
gh release download "$TAG" --repo "$CODE_REPO" \
  --pattern "bootstrap-*.tar.gz*" 2>/dev/null || true
for sha_file in bootstrap-*.tar.gz.sha256; do
  [[ -e "$sha_file" ]] || break
  sha256sum -c "$sha_file"
  tar -xzf "${sha_file%.sha256}"
  log "verified + extracted release bootstrap"
  break
done

# Fallback for releases predating the bootstrap asset: shallow-clone
# the tool repo AT THE TAG (integrity via TLS + the tag's commit; the
# verified-asset path above is the preferred, pinned route).
if [[ ! -x tools/bootstrap-data-repo.sh ]]; then
  log "release has no bootstrap asset — falling back to a tag-pinned shallow clone"
  git clone --quiet --depth 1 --branch "$TAG" \
    "https://github.com/${CODE_REPO}.git" src
  cp -R src/tools tools
fi

[[ -x tools/bootstrap-data-repo.sh ]] || err "bootstrap script not found in ${TAG} — cannot continue"

# Default the gate pin to this same release unless the caller chose one.
extra_args=()
case " $* " in
  *" --pr-release "*) ;;
  *) extra_args=(--pr-release "$TAG") ;;
esac

log "running bootstrap (gate pinned to ${TAG})"
exec tools/bootstrap-data-repo.sh "$@" "${extra_args[@]}"
