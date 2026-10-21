#!/bin/bash
# Compilación local para macOS Big Sur e Intel; no instala herramientas.
set -euo pipefail

readonly TOOLCHAIN="1.96.0"
readonly TARGET="x86_64-apple-darwin"
readonly APP_NAME="Tuner Compact"
readonly BUNDLE_IDENTIFIER="com.tuner.guitar.compact"
readonly UPDATE_REPOSITORY="Chevosky/tuner-compact"
readonly MINIMUM_MACOS="11.0"
readonly ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly PLIST_BUDDY="/usr/libexec/PlistBuddy"
stage=""

fail() {
  printf '\nError: %s\n' "$*" >&2
  exit 1
}

cleanup() {
  if [[ -n "$stage" && -d "$stage" ]]; then
    rm -rf "$stage"
  fi
}
trap cleanup EXIT

[[ "$(uname -s)" == "Darwin" ]] || fail "Abrí este archivo en el iMac con macOS."
[[ "$(uname -m)" == "x86_64" ]] || fail "Este script está preparado para un Mac Intel."
xcode-select -p >/dev/null 2>&1 || fail "Faltan Xcode Command Line Tools. Ejecutá 'xcode-select --install' en Terminal y volvé a abrir este archivo."
sdk="$(xcrun --sdk macosx --show-sdk-path 2>/dev/null)" || fail "No se encontró el SDK de macOS. Revisá la instalación de Xcode Command Line Tools."
xcrun --find clang >/dev/null 2>&1 || fail "No se encontró clang. Revisá Xcode Command Line Tools."
xcrun --find otool >/dev/null 2>&1 || fail "No se encontró otool. Revisá Xcode Command Line Tools."
xcrun --find lipo >/dev/null 2>&1 || fail "No se encontró lipo. Revisá Xcode Command Line Tools."

# Finder puede abrir Terminal sin cargar el PATH configurado por rustup.
if ! command -v rustup >/dev/null 2>&1; then
  export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
fi
command -v rustup >/dev/null 2>&1 || fail "Falta Rust con rustup. Instalalo siguiendo https://rustup.rs y luego ejecutá:
  rustup toolchain install $TOOLCHAIN --profile minimal
  rustup target add --toolchain $TOOLCHAIN $TARGET
Después volvé a abrir este archivo. El script no instala herramientas automáticamente."
rustup run "$TOOLCHAIN" rustc --version >/dev/null 2>&1 || fail "Falta Rust $TOOLCHAIN. Ejecutá en Terminal:
  rustup toolchain install $TOOLCHAIN --profile minimal
Después volvé a abrir este archivo."
installed_targets="$(rustup target list --toolchain "$TOOLCHAIN" --installed)"
grep -Fx "$TARGET" <<< "$installed_targets" >/dev/null || fail "Falta el target Intel. Ejecutá en Terminal:
  rustup target add --toolchain $TOOLCHAIN $TARGET
Después volvé a abrir este archivo."

[[ -s "$ROOT/Cargo.lock" ]] || fail "Falta Cargo.lock; usá la carpeta completa del proyecto."
version="$(sed -n 's/^version = "\([0-9][0-9.]*\)"$/\1/p' "$ROOT/Cargo.toml")"
[[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] \
  || fail "No se pudo leer la versión del proyecto."
# La app, el bundle y la publicación comparten una sola revisión. Aumentarla
# para cada release; nunca volver a usar una revisión ya publicada.
[[ -s "$ROOT/egui/src/version.rs" ]] || fail "Falta egui/src/version.rs; usá la carpeta completa del proyecto."
compact_revision="$(sed -n 's/^pub const COMPACT_REVISION: u64 = \([1-9][0-9]*\);$/\1/p' "$ROOT/egui/src/version.rs")"
[[ "$compact_revision" =~ ^[1-9][0-9]*$ && ${#compact_revision} -le 9 ]] \
  || fail "No se pudo leer COMPACT_REVISION de egui/src/version.rs."
readonly COMPACT_REVISION="$compact_revision"
readonly RELEASE_TAG="compact-v${version}-r${COMPACT_REVISION}"

export MACOSX_DEPLOYMENT_TARGET="$MINIMUM_MACOS"
export SDKROOT="$sdk"
export CARGO_INCREMENTAL="0"
readonly BUILD_DIRECTORY="$ROOT/target/compact-macos"
readonly BINARY="$BUILD_DIRECTORY/$TARGET/release/guitar-tuner-egui"

printf '\nCompilando %s para Intel / macOS 11 con Rust %s…\n' "$APP_NAME" "$TOOLCHAIN"
printf 'Cargo usa Cargo.lock y puede descargar las dependencias que falten.\n\n'
cd "$ROOT"
rustup run "$TOOLCHAIN" cargo build --locked --release \
  -p guitar-tuner-egui --target "$TARGET" --target-dir "$BUILD_DIRECTORY"

[[ "$(file -b "$BINARY")" == "Mach-O 64-bit executable x86_64"* ]] \
  || fail "El ejecutable no es Mach-O de 64 bits para Intel."
[[ "$(xcrun lipo -archs "$BINARY")" == "x86_64" ]] \
  || fail "El ejecutable no tiene la arquitectura x86_64 esperada."
load_commands="$(xcrun otool -l "$BINARY")"
minimum_versions="$(awk '
  $1 == "cmd" { modern = ($2 == "LC_BUILD_VERSION"); legacy = ($2 == "LC_VERSION_MIN_MACOSX") }
  modern && $1 == "minos" { print $2 }
  legacy && $1 == "version" { print $2 }
' <<< "$load_commands")"
[[ -n "$minimum_versions" ]] || fail "No se pudo comprobar el macOS mínimo del ejecutable."
awk -F. '
  ($1 + 0 > 11) || ($1 + 0 == 11 && ($2 + 0 > 0 || $3 + 0 > 0)) { exit 1 }
' <<< "$minimum_versions" || fail "El ejecutable requiere macOS $minimum_versions; se esperaba 11.0 o anterior."

# El empaquetador original trabaja sobre un workspace temporal para no pisar
# paquetes anteriores. Conserva su icono, plantilla y permiso de micrófono.
stage="$(mktemp -d "${TMPDIR:-/tmp}/tuner-compact.XXXXXX")"
mkdir -p "$stage/target/$TARGET/release" "$stage/assets/icons" "$stage/scripts/assets"
cp "$BINARY" "$stage/target/$TARGET/release/guitar-tuner-egui"
cp "$ROOT/assets/icons/icon.icns" "$stage/assets/icons/icon.icns"
cp "$ROOT/scripts/assets/Info.plist.template" "$stage/scripts/assets/Info.plist.template"
bash "$ROOT/scripts/package-egui-release.sh" package \
  --workspace "$stage" --runner-os macOS --target "$TARGET" \
  --version "$version" --github-output "$stage/package-output"
original_zip="guitar-tuner-v${version}-macos-x86_64.app.zip"
bash "$ROOT/scripts/package-egui-release.sh" verify \
  --workspace "$stage" --runner-os macOS --version "$version" \
  --package-name "$original_zip" --package-path "release-artifacts/$original_zip"

app="$stage/package-work/$APP_NAME.app"
mv "$stage/package-work/Guitar Tuner.app" "$app"
plist="$app/Contents/Info.plist"
microphone_description="$("$PLIST_BUDDY" -c 'Print :NSMicrophoneUsageDescription' "$plist")"
[[ -n "$microphone_description" ]] || fail "Falta la descripción de uso del micrófono."
"$PLIST_BUDDY" -c "Set :CFBundleDisplayName $APP_NAME" "$plist"
"$PLIST_BUDDY" -c "Set :CFBundleName $APP_NAME" "$plist"
"$PLIST_BUDDY" -c "Set :CFBundleIdentifier $BUNDLE_IDENTIFIER" "$plist"
"$PLIST_BUDDY" -c "Set :CFBundleVersion ${COMPACT_REVISION}" "$plist"
"$PLIST_BUDDY" -c "Set :LSMinimumSystemVersion $MINIMUM_MACOS" "$plist" 2>/dev/null \
  || "$PLIST_BUDDY" -c "Add :LSMinimumSystemVersion string $MINIMUM_MACOS" "$plist"
/usr/bin/plutil -lint "$plist"
[[ "$("$PLIST_BUDDY" -c 'Print :CFBundleExecutable' "$plist")" == "guitar-tuner-egui" ]] \
  || fail "El plist no apunta al ejecutable correcto."
[[ "$("$PLIST_BUDDY" -c 'Print :CFBundleIdentifier' "$plist")" == "$BUNDLE_IDENTIFIER" ]] \
  || fail "El bundle no tiene el identificador esperado."
[[ "$("$PLIST_BUDDY" -c 'Print :CFBundleShortVersionString' "$plist")" == "$version" ]] \
  || fail "La versión del bundle no coincide con Cargo.toml."
[[ "$("$PLIST_BUDDY" -c 'Print :CFBundleVersion' "$plist")" == "$COMPACT_REVISION" ]] \
  || fail "La revisión del bundle no coincide con la app."
[[ "$("$PLIST_BUDDY" -c 'Print :LSMinimumSystemVersion' "$plist")" == "$MINIMUM_MACOS" ]] \
  || fail "El bundle no declara macOS $MINIMUM_MACOS."
[[ "$("$PLIST_BUDDY" -c 'Print :NSMicrophoneUsageDescription' "$plist")" == "$microphone_description" ]] \
  || fail "Cambió la descripción de uso del micrófono."
cmp -s "$BINARY" "$app/Contents/MacOS/guitar-tuner-egui" || fail "El bundle contiene otro ejecutable."
cmp -s "$ROOT/assets/icons/icon.icns" "$app/Contents/Resources/icon.icns" || fail "El icono del bundle no coincide."
[[ -x "$app/Contents/MacOS/guitar-tuner-egui" ]] || fail "Falta el permiso de ejecución."
mkdir -p "$app/Contents/Resources/Licenses"
cp "$ROOT/LICENSE" "$app/Contents/Resources/Licenses/Tuner-MIT.txt"
cp "$ROOT/vendor/cpal/LICENSE" "$app/Contents/Resources/Licenses/CPAL-Apache-2.0.txt"

zip_name="Tuner-Compact-v${version}-r${COMPACT_REVISION}-macos-x86_64.app.zip"
zip_path="$stage/$zip_name"
/usr/bin/ditto -c -k --sequesterRsrc --keepParent "$app" "$zip_path"
unzip -tq "$zip_path"
archive_entries="$(unzip -Z1 "$zip_path")"
for entry in Contents/Info.plist Contents/MacOS/guitar-tuner-egui Contents/Resources/icon.icns \
  Contents/Resources/Licenses/Tuner-MIT.txt Contents/Resources/Licenses/CPAL-Apache-2.0.txt; do
  grep -Fx "$APP_NAME.app/$entry" <<< "$archive_entries" >/dev/null \
    || fail "Falta $entry en el ZIP final."
done

[[ "$(wc -c < "$zip_path" | tr -d '[:space:]')" -le 268435456 ]] \
  || fail "El ZIP supera el límite de 256 MiB del actualizador."
sha256="$(shasum -a 256 "$zip_path" | awk '{print $1}')"
[[ "$sha256" =~ ^[0-9a-f]{64}$ ]] || fail "No se pudo calcular SHA-256."
checksum_name="${zip_name}.sha256"
manifest_name="tuner-compact-update.json"
printf '%s  %s\n' "$sha256" "$zip_name" > "$stage/$checksum_name"
# Los campos variables están restringidos a números, puntos y nombres fijos.
# La URL apunta a una release durable; ejecutar este script no la publica.
cat > "$stage/$manifest_name" <<EOF
{
  "schema_version": 1,
  "version": "$version",
  "revision": $COMPACT_REVISION,
  "tag": "$RELEASE_TAG",
  "platform": "macos",
  "arch": "x86_64",
  "minimum_macos": "$MINIMUM_MACOS",
  "bundle_id": "$BUNDLE_IDENTIFIER",
  "executable": "guitar-tuner-egui",
  "asset": "$zip_name",
  "url": "https://github.com/$UPDATE_REPOSITORY/releases/download/$RELEASE_TAG/$zip_name",
  "sha256": "$sha256"
}
EOF

mkdir -p "$ROOT/compact-builds"
output="$(mktemp -d "$ROOT/compact-builds/compact-${version}-r${COMPACT_REVISION}-XXXXXX")"
mv "$app" "$output/$APP_NAME.app"
mv "$zip_path" "$output/$zip_name"
mv "$stage/$checksum_name" "$stage/$manifest_name" "$output/"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    printf 'version=%s\n' "$version"
    printf 'revision=%s\n' "$COMPACT_REVISION"
    printf 'tag=%s\n' "$RELEASE_TAG"
    printf 'zip_name=%s\n' "$zip_name"
    printf 'checksum_name=%s\n' "$checksum_name"
    printf 'manifest_name=%s\n' "$manifest_name"
    printf 'artifact_directory=%s\n' "$output"
  } >> "$GITHUB_OUTPUT"
fi
printf '\nCompilación y paquete verificados (Intel; macOS mínimo: %s).\n' "$minimum_versions"
printf '\nApp: %s\nZIP: %s\n' "$output/$APP_NAME.app" "$output/$zip_name"
printf 'SHA-256: %s\nManifest: %s\n' "$output/$checksum_name" "$output/$manifest_name"
printf '\nPodés abrir Tuner Compact.app. La aplicación instalada no se reemplazó.\n'
printf 'No se publicaron tags ni releases. Actions publica sólo con publish activado o al subir una revisión nueva.\n'
