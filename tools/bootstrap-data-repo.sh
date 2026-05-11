#!/usr/bin/env bash
# Bootstrap (or update) a data repo for the part-registry per ADR-019 +
# #35 Phase 3. Idempotent: missing files are created; existing files
# are left as-is unless `--force` is passed for the relevant kind.
#
# Usage:
#   tools/bootstrap-data-repo.sh <owner/repo> [options]
#
# Options:
#   --create            `gh repo create` if the repo doesn't exist
#                       (default: error out if missing).
#   --visibility VIS    public | private | internal (default: public —
#                       the org needs Pro for Pages-on-private, see
#                       exo-pet/exopet-registry#1).
#   --pages-base BASE   override the VITE_BASE used by the pages
#                       workflow (default: /<repo-name>/).
#   --force-pages       overwrite an existing pages.yml.
#   --force-readme      overwrite an existing README.md.
#   --dry-run           print what would happen, don't push.
#
# Examples:
#   tools/bootstrap-data-repo.sh exo-pet/exopet-registry-sandbox --create
#   tools/bootstrap-data-repo.sh exo-pet/exopet-registry        # idempotent additions
#
# Reads its own location to find sibling template files in
# `tools/data-repo-templates/`. Shells out to `gh` and `git`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEMPLATES_DIR="${SCRIPT_DIR}/data-repo-templates"
CODE_REPO="MorePET/part-registry"

# CSV headers (canonical, from the workspace contracts).
REGISTRY_HEADER='id,status,minted_at,batch,bound_at,type,description,vendor,part_number,location,notes'
PRINT_LOG_HEADER='id,printed_at,printed_by,layout,size_mm,extra,copies,output_mode,batch_label'
AUDIT_LOG_HEADER='request_id,timestamp,actor,action,target,before,after,extra,signatures,chain_hash'

# --- Args ----------------------------------------------------------

target=""
create=0
visibility="public"
pages_base=""
force_pages=0
force_readme=0
dry_run=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --create) create=1; shift ;;
    --visibility) visibility="$2"; shift 2 ;;
    --pages-base) pages_base="$2"; shift 2 ;;
    --force-pages) force_pages=1; shift ;;
    --force-readme) force_readme=1; shift ;;
    --dry-run) dry_run=1; shift ;;
    -h|--help)
      sed -n '2,/^set -/p' "$0" | sed -n '/^#/p' | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    -*) echo "unknown flag: $1" >&2; exit 2 ;;
    *)
      if [[ -z "$target" ]]; then target="$1"; else
        echo "extra positional arg: $1" >&2; exit 2
      fi
      shift
      ;;
  esac
done

[[ -n "$target" ]] || { echo "error: missing <owner/repo>" >&2; exit 2; }

if [[ ! "$target" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
  echo "error: target must be owner/repo, got: $target" >&2
  exit 2
fi

name="${target#*/}"
[[ -n "$pages_base" ]] || pages_base="/${name}/"

# --- Helpers -------------------------------------------------------

log()  { printf '\033[1m[bootstrap]\033[0m %s\n' "$*"; }
warn() { printf '\033[33m[bootstrap]\033[0m %s\n' "$*" >&2; }
err()  { printf '\033[31m[bootstrap]\033[0m %s\n' "$*" >&2; exit 1; }
do_or_say() {
  if (( dry_run )); then echo "DRY-RUN: $*"; else "$@"; fi
}

ensure_dir() { mkdir -p "$1"; }
write_if_missing() {
  local path="$1" content="$2" force="${3:-0}"
  if [[ -e "$path" && "$force" != 1 ]]; then
    log "  exists, skipping: $path"
    return
  fi
  if (( dry_run )); then
    echo "DRY-RUN: would write $path ($(echo -n "$content" | wc -c) bytes)"
    return
  fi
  printf '%s' "$content" > "$path"
  log "  wrote: $path"
}

# Render `${PLACEHOLDER}` substitutions into a template. Bash's
# `envsubst` would do it but we want a no-extra-dep path.
render_template() {
  local src="$1"; shift
  local out="$1"; shift
  if [[ ! -f "$src" ]]; then
    err "template missing: $src"
  fi
  # Build a sed program from $key=value pairs.
  local sed_script=""
  while (( $# > 0 )); do
    local key="${1%%=*}" val="${1#*=}"
    # Escape sed metacharacters in the value.
    val=$(printf '%s' "$val" | sed -e 's/[\/&]/\\&/g')
    sed_script+="s/__${key}__/${val}/g; "
    shift
  done
  sed -e "$sed_script" "$src" > "$out"
  log "  rendered: $out"
}

# --- Repo existence + creation ------------------------------------

log "target: $target (visibility intent: $visibility)"
log "code repo (FE source): $CODE_REPO"

if gh repo view "$target" >/dev/null 2>&1; then
  log "repo exists upstream"
else
  if (( create )); then
    log "creating repo via gh repo create"
    do_or_say gh repo create "$target" \
      --"$visibility" \
      --description "Operator data for part-registry — registry.csv, print_log.csv, audit_log.csv. Code lives at $CODE_REPO." \
      --clone=false
  else
    err "repo $target does not exist; pass --create to create it"
  fi
fi

# --- Local working clone ------------------------------------------

workdir="$(mktemp -d -t bootstrap-data-repo.XXXXXX)"
trap 'rm -rf "$workdir"' EXIT

log "cloning into $workdir"
if (( dry_run )); then
  log "DRY-RUN: skipping clone (using empty workdir)"
  mkdir -p "$workdir/$name"
else
  # gh repo clone resolves auth correctly for private repos; falls
  # back to HTTPS for public.
  gh repo clone "$target" "$workdir/$name" -- --quiet 2>/dev/null || {
    # Brand-new repo with no commits: clone returns empty; init manually.
    log "  fresh repo, initialising local tree"
    mkdir -p "$workdir/$name"
    (cd "$workdir/$name" && git init --initial-branch=main >/dev/null && \
       git remote add origin "https://github.com/$target.git")
  }
fi

cd "$workdir/$name"

# Mark as exo-pet/MorePET admin operation so generated commits aren't
# attributed to a random global git config.
if (( ! dry_run )); then
  git config user.email "bootstrap@part-registry.invalid"
  git config user.name "part-registry bootstrap"
  git config commit.gpgsign false
fi

# --- Files --------------------------------------------------------

log "ensuring schema files (idempotent)"
write_if_missing registry.csv  "${REGISTRY_HEADER}"$'\n'
write_if_missing print_log.csv "${PRINT_LOG_HEADER}"$'\n'
write_if_missing audit_log.csv "${AUDIT_LOG_HEADER}"$'\n'

log "ensuring README"
write_if_missing README.md "$(cat <<EOF
# ${name}

Operator data for [\`${CODE_REPO}\`](https://github.com/${CODE_REPO}) —
\`registry.csv\`, \`print_log.csv\`, \`audit_log.csv\`. See that repo's
[\`decisions/ADR-019\`](https://github.com/${CODE_REPO}/blob/main/decisions/ADR-019-proposal-sink-port.md)
for the code/data split rationale and
[\`decisions/ADR-013\`](https://github.com/${CODE_REPO}/blob/main/decisions/ADR-013-parts-registry-web-app.md)
for the PR-driven mutation model.

## How to interact

Reads + writes flow through the part-registry FE (deployed to GitHub Pages
from this repo via the [\`pages.yml\`](.github/workflows/pages.yml) workflow)
or via the Rust CLIs (\`mint\` / \`label\` / \`bind\`) in the code repo
with \`PART_REGISTRY__REPO__DATA_REPO_URL=https://github.com/${target}\`.

Mutations open PRs via the \`ProposalSink\` adapter (ADR-019) — no
direct commits to \`main\` from operators. Branch protection lands
once the org plan supports it (see
[\`${CODE_REPO}#issue?\`](https://github.com/${CODE_REPO}/issues) or the
upgrade-tracking issue in this repo).

## Schemas

- **\`registry.csv\`** — canonical part records, sorted by ID. Header:
  \`${REGISTRY_HEADER}\`.
- **\`print_log.csv\`** — append-only print events per ADR-015. Header:
  \`${PRINT_LOG_HEADER}\`.
- **\`audit_log.csv\`** — append-only audit trail per ADR-022 with
  forward-compat signature + chain-hash columns (ADR-023). Header:
  \`${AUDIT_LOG_HEADER}\`.

## Bundle provenance

Each Pages deploy records the consumed code-repo release tag + commit
in the build's \`BUNDLE_METADATA.json\`. Match a deployed site back to
its source by inspecting the published bundle.
EOF
)" "$force_readme"

log "ensuring CONTRIBUTING.md"
write_if_missing CONTRIBUTING.md "$(cat <<EOF
# Contributing to ${name}

This is a data repo: only \`registry.csv\`, \`print_log.csv\`,
\`audit_log.csv\`, and the Pages deploy workflow live here. All
schema, validator, and FE changes go in
[\`${CODE_REPO}\`](https://github.com/${CODE_REPO}).

Mutations to this repo arrive as Pull Requests opened by the FE / Rust
CLIs via the \`ProposalSink\` adapter (see code-repo
\`decisions/ADR-019\`). Do not push directly to \`main\` once branch
protection is enabled.
EOF
)"

log "ensuring .gitignore"
write_if_missing .gitignore "$(cat <<'EOF'
# Per-deploy artifacts — never committed.
node_modules/
dist/
web/                  # extracted from the FE bundle at deploy time; not source-controlled here
schema/               # ditto

# OS / editor noise
.DS_Store
*.swp
.vscode/
.idea/
EOF
)"

log "ensuring pages.yml workflow"
ensure_dir .github/workflows
if [[ ! -e .github/workflows/pages.yml || "$force_pages" = 1 ]]; then
  render_template \
    "${TEMPLATES_DIR}/pages.yml.tmpl" \
    .github/workflows/pages.yml \
    "CODE_REPO=${CODE_REPO}" \
    "DATA_REPO=${target}" \
    "PAGES_BASE=${pages_base}"
else
  log "  exists, skipping: .github/workflows/pages.yml (use --force-pages to overwrite)"
fi

# --- Commit + push -------------------------------------------------

if (( dry_run )); then
  log "DRY-RUN: would commit + push to origin/main"
  exit 0
fi

if [[ -z "$(git status --porcelain)" ]]; then
  log "no changes to commit"
  exit 0
fi

log "committing + pushing"
git add -A
git commit -m "Bootstrap data repo ($(date -u +%Y-%m-%d))" \
  -m "Applied by tools/bootstrap-data-repo.sh from ${CODE_REPO}. Idempotent — re-run to add missing files without overwriting existing ones."

# First push needs --set-upstream; subsequent pushes don't.
if git ls-remote --heads origin main | grep -q .; then
  git push origin HEAD:main
else
  git push -u origin HEAD:main
fi

log "done. data repo: https://github.com/${target}"
log "next:"
log "  1. tag a release in ${CODE_REPO} (e.g. v0.1.0) so this repo's"
log "     pages.yml has a bundle to download"
log "  2. trigger pages.yml via workflow_dispatch or wait for the next push"
log "  3. enable Pages in the repo settings: Source = GitHub Actions"
