#!/usr/bin/env bash

set -Eeuo pipefail
IFS=$'\n\t'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REMOTE="${RELEASE_REMOTE:-origin}"
WORKFLOW_FILE="build-installers.yml"

info() {
  printf '\n==> %s\n' "$1"
}

die() {
  printf '\nError: %s\n' "$1" >&2
  exit 1
}

confirm() {
  local prompt="$1"
  local answer
  read -r -p "$prompt [y/N] " answer
  [[ "$answer" == "y" || "$answer" == "Y" || "$answer" == "yes" || "$answer" == "YES" ]]
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "Falta el comando requerido: $1"
}

is_semver() {
  [[ "$1" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]
}

is_greater_version() {
  node -e '
    const left = process.argv[1].split(".").map(Number);
    const right = process.argv[2].split(".").map(Number);
    for (let index = 0; index < 3; index += 1) {
      if (left[index] > right[index]) process.exit(0);
      if (left[index] < right[index]) process.exit(1);
    }
    process.exit(1);
  ' "$1" "$2"
}

suggest_patch_version() {
  node -e '
    const parts = process.argv[1].split(".").map(Number);
    parts[2] += 1;
    process.stdout.write(parts.join("."));
  ' "$1"
}

github_repository() {
  local remote_url="$1"
  remote_url="${remote_url%.git}"
  case "$remote_url" in
    git@github.com:*) printf '%s' "${remote_url#git@github.com:}" ;;
    ssh://git@github.com/*) printf '%s' "${remote_url#ssh://git@github.com/}" ;;
    https://github.com/*) printf '%s' "${remote_url#https://github.com/}" ;;
    http://github.com/*) printf '%s' "${remote_url#http://github.com/}" ;;
    *) return 1 ;;
  esac
}

show_help() {
  cat <<'HELP'
Uso: npm run release

Asistente interactivo para publicar Rau Studio:
  1. Sugiere la siguiente version patch.
  2. Actualiza package.json, package-lock.json, Cargo y Tauri.
  3. Ejecuta formato, build y tests.
  4. Crea un commit Conventional Commit y un tag anotado.
  5. Hace push atomico del commit y tag.
  6. El tag dispara Build installers y el GitHub Release.

Variables opcionales:
  RELEASE_REMOTE=origin  Remote al que se hace fetch/push.
HELP
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  show_help
  exit 0
fi

[[ $# -eq 0 ]] || die "Argumento desconocido: $1. Usa --help."
[[ -t 0 ]] || die "Este asistente necesita una terminal interactiva."

require_command git
require_command node
require_command npm
require_command cargo

cd "$REPO_DIR"
git rev-parse --show-toplevel >/dev/null 2>&1 || die "No se encontro un repositorio Git."
git remote get-url "$REMOTE" >/dev/null 2>&1 || die "No existe el remote '$REMOTE'."

branch="$(git branch --show-current)"
[[ -n "$branch" ]] || die "No se puede publicar desde un HEAD desacoplado."

current_version="$(node -p "require('./package.json').version")"
is_semver "$current_version" || die "La version actual no es SemVer estable: $current_version"

lock_version="$(node -p "require('./package-lock.json').version")"
tauri_version="$(node -p "require('./src-tauri/tauri.conf.json').version")"
cargo_version="$(node -e '
  const fs = require("fs");
  const source = fs.readFileSync("src-tauri/Cargo.toml", "utf8");
  const match = source.match(/\[package\]\s+name = "rau-studio"\s+version = "([^"]+)"/);
  if (!match) process.exit(1);
  process.stdout.write(match[1]);
')"

for version_source in "$lock_version" "$tauri_version" "$cargo_version"; do
  [[ "$version_source" == "$current_version" ]] || \
    die "Las versiones del proyecto no coinciden. Corrigelas antes de publicar."
done

info "Sincronizando referencias y tags desde $REMOTE"
git fetch "$REMOTE" --tags

if git show-ref --verify --quiet "refs/remotes/$REMOTE/$branch"; then
  git merge-base --is-ancestor "$REMOTE/$branch" HEAD || \
    die "La rama local no contiene $REMOTE/$branch. Integra los cambios remotos primero."
fi

latest_tag=""
while IFS= read -r candidate; do
  if [[ "$candidate" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
    latest_tag="$candidate"
    break
  fi
done < <(git tag --list 'v*' --sort=-version:refname)

base_version="$current_version"
if [[ -n "$latest_tag" ]]; then
  latest_version="${latest_tag#v}"
  if is_greater_version "$latest_version" "$base_version"; then
    base_version="$latest_version"
  fi
fi
suggested_version="$(suggest_patch_version "$base_version")"

info "Preparando release"
printf 'Rama:          %s\n' "$branch"
printf 'Version actual: %s\n' "$current_version"
printf 'Ultimo tag:     %s\n' "${latest_tag:-ninguno}"
printf 'Sugerencia:     v%s\n' "$suggested_version"
printf '\nCambios que entrarian al release:\n'
git status --short

if [[ "$branch" != "main" ]]; then
  confirm "Estas en '$branch', no en 'main'. Continuar de todos modos?" || exit 0
fi

read -r -p "Nueva version [$suggested_version]: " requested_version
version="${requested_version:-$suggested_version}"
version="${version#v}"
is_semver "$version" || die "Version invalida: $version. Usa X.Y.Z."
is_greater_version "$version" "$base_version" || \
  die "La nueva version debe ser mayor que $base_version."

tag="v$version"
git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null && die "El tag $tag ya existe."

confirm "Actualizar versiones y ejecutar todas las validaciones para $tag?" || exit 0

info "Actualizando archivos de version a $version"
RELEASE_VERSION="$version" node <<'NODE'
const fs = require("fs");

const version = process.env.RELEASE_VERSION;

function readJson(path) {
  return JSON.parse(fs.readFileSync(path, "utf8"));
}

function writeJson(path, value) {
  fs.writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

const packageJson = readJson("package.json");
packageJson.version = version;
writeJson("package.json", packageJson);

const packageLock = readJson("package-lock.json");
packageLock.version = version;
if (packageLock.packages && packageLock.packages[""]) {
  packageLock.packages[""].version = version;
}
writeJson("package-lock.json", packageLock);

const tauriConfig = readJson("src-tauri/tauri.conf.json");
tauriConfig.version = version;
writeJson("src-tauri/tauri.conf.json", tauriConfig);

const cargoPath = "src-tauri/Cargo.toml";
const cargoSource = fs.readFileSync(cargoPath, "utf8");
const cargoPattern = /(\[package\]\s+name = "rau-studio"\s+version = ")[^"]+("[\s\S]*)/;
if (!cargoPattern.test(cargoSource)) {
  throw new Error(`No se encontro la version de rau-studio en ${cargoPath}`);
}
fs.writeFileSync(cargoPath, cargoSource.replace(cargoPattern, `$1${version}$2`));
NODE

info "Validando formato Rust"
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

info "Compilando frontend"
npm run build

info "Ejecutando tests Rust"
cargo test --manifest-path src-tauri/Cargo.toml

info "Revisando whitespace y versiones"
git diff --check
RELEASE_VERSION="$version" node <<'NODE'
const fs = require("fs");
const expected = process.env.RELEASE_VERSION;
const values = {
  "package.json": JSON.parse(fs.readFileSync("package.json", "utf8")).version,
  "package-lock.json": JSON.parse(fs.readFileSync("package-lock.json", "utf8")).version,
  "src-tauri/tauri.conf.json": JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8")).version,
  "src-tauri/Cargo.toml": fs.readFileSync("src-tauri/Cargo.toml", "utf8")
    .match(/\[package\]\s+name = "rau-studio"\s+version = "([^"]+)"/)[1],
  "Cargo.lock": fs.readFileSync("Cargo.lock", "utf8")
    .match(/\[\[package\]\]\s+name = "rau-studio"\s+version = "([^"]+)"/)[1]
};
for (const [path, actual] of Object.entries(values)) {
  if (actual !== expected) {
    throw new Error(`${path}: esperaba ${expected}, encontro ${actual}`);
  }
}
NODE

info "Cambios listos para $tag"
git diff --stat
printf '\n'
git status --short

confirm "Crear el commit y tag $tag incluyendo todos estos cambios?" || {
  printf '\nVersiones actualizadas, pero no se creo commit ni tag.\n'
  exit 0
}

read -r -p "Resumen opcional del release: " release_summary

git add -A
git diff --cached --quiet && die "No hay cambios para commitear."

if [[ -n "$release_summary" ]]; then
  git commit -m "chore(release): prepare $tag" -m "$release_summary"
else
  git commit -m "chore(release): prepare $tag"
fi

git tag -a "$tag" -m "Rau Studio $tag"
release_commit="$(git rev-parse HEAD)"

confirm "Hacer push atomico de '$branch' y '$tag' a '$REMOTE'?" || {
  printf '\nCommit y tag creados localmente. Para publicarlos luego:\n'
  printf '  git push --atomic %s HEAD:refs/heads/%s refs/tags/%s:refs/tags/%s\n' \
    "$REMOTE" "$branch" "$tag" "$tag"
  exit 0
}

info "Publicando commit y tag"
git push --atomic "$REMOTE" \
  "HEAD:refs/heads/$branch" \
  "refs/tags/$tag:refs/tags/$tag"

remote_url="$(git remote get-url "$REMOTE")"
repository="$(github_repository "$remote_url" || true)"

printf '\n%s publicado. El workflow %s construira los instaladores y creara el GitHub Release.\n' \
  "$tag" "$WORKFLOW_FILE"

if [[ -n "$repository" ]]; then
  printf 'Actions: https://github.com/%s/actions/workflows/%s\n' "$repository" "$WORKFLOW_FILE"
  printf 'Release: https://github.com/%s/releases/tag/%s\n' "$repository" "$tag"
fi

if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  if confirm "Esperar y seguir el workflow desde esta terminal?"; then
    info "Esperando que GitHub registre el workflow"
    run_id=""
    for _attempt in 1 2 3 4 5 6 7 8 9 10 11 12; do
      run_id="$(gh run list \
        --repo "$repository" \
        --workflow "$WORKFLOW_FILE" \
        --commit "$release_commit" \
        --limit 1 \
        --json databaseId \
        --jq '.[0].databaseId // empty' 2>/dev/null || true)"
      [[ -n "$run_id" ]] && break
      sleep 5
    done

    if [[ -n "$run_id" ]]; then
      gh run watch "$run_id" --repo "$repository" --exit-status
    else
      printf 'No se encontro el workflow todavia; revisa el enlace de Actions.\n'
    fi
  fi
elif command -v gh >/dev/null 2>&1; then
  printf 'Tip: ejecuta `gh auth login -h github.com` para monitorear futuros releases desde el script.\n'
fi
