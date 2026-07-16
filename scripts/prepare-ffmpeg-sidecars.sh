#!/usr/bin/env bash
set -euo pipefail

FFMPEG_VERSION="8.1.2"
FFMPEG_SHA256="464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c"
X264_COMMIT="b35605ace3ddf7c1a5d67a2eb553f034aef41d55"
X264_SHA256="6eeb82934e69fd51e043bd8c5b0d152839638d1ce7aa4eea65a3fedcf83ff224"
MACOS_DEPLOYMENT_TARGET="11.0"
BUILD_ID="$FFMPEG_VERSION-x264-${X264_COMMIT:0:12}-macos-$MACOS_DEPLOYMENT_TARGET-static-v2"

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target_triple="${TAURI_ENV_TARGET_TRIPLE:-$(rustc --print host-tuple)}"

case "$target_triple" in
  aarch64-apple-darwin)
    target_arch="arm64"
    ;;
  x86_64-apple-darwin)
    target_arch="x86_64"
    ;;
  *)
    echo "FFmpeg sidecars are currently bundled only for macOS; skipping $target_triple."
    exit 0
    ;;
esac

cache_dir="$root_dir/.cache/ffmpeg"
archive="$cache_dir/ffmpeg-$FFMPEG_VERSION.tar.xz"
source_dir="$cache_dir/ffmpeg-$FFMPEG_VERSION"
x264_archive="$cache_dir/x264-$X264_COMMIT.tar.bz2"
x264_source_dir="$cache_dir/x264-$X264_COMMIT"
x264_build_dir="$cache_dir/build-x264-$target_triple"
x264_install_dir="$cache_dir/install-x264-$target_triple"
build_dir="$cache_dir/build-$target_triple"
binary_dir="$root_dir/src-tauri/binaries"
ffmpeg_output="$binary_dir/ffmpeg-$target_triple"
ffprobe_output="$binary_dir/ffprobe-$target_triple"
version_marker="$binary_dir/ffmpeg-$target_triple.version"

verify_archive() {
  local actual
  actual="$(openssl dgst -sha256 "$archive" | awk '{print $NF}')"
  if [[ "$actual" != "$FFMPEG_SHA256" ]]; then
    echo "FFmpeg archive checksum mismatch: expected $FFMPEG_SHA256, got $actual." >&2
    exit 1
  fi
}

verify_x264_archive() {
  local actual
  actual="$(openssl dgst -sha256 "$x264_archive" | awk '{print $NF}')"
  if [[ "$actual" != "$X264_SHA256" ]]; then
    echo "x264 archive checksum mismatch: expected $X264_SHA256, got $actual." >&2
    exit 1
  fi
}

validate_binary_architecture() {
  local binary="$1"
  local description
  description="$(file "$binary")"
  if [[ "$description" != *"$target_arch"* ]]; then
    echo "$binary has the wrong architecture: $description" >&2
    exit 1
  fi

  local dependencies
  dependencies="$(otool -L "$binary")"
  if [[ "$dependencies" == *"/opt/homebrew/"* || "$dependencies" == *"/usr/local/"* ]]; then
    echo "$binary contains a package-manager dependency:" >&2
    echo "$dependencies" >&2
    exit 1
  fi
}

validate_native_features() {
  local host_triple
  host_triple="$(rustc --print host-tuple)"
  if [[ "$host_triple" != "$target_triple" ]]; then
    if ! "$ffmpeg_output" -version >/dev/null 2>&1 || \
       ! "$ffprobe_output" -version >/dev/null 2>&1; then
      echo "Cross-built $target_triple sidecars; runtime checks are unavailable on $host_triple."
      return
    fi
    echo "Running $target_triple feature checks through the local compatibility layer."
  fi

  local encoders filters smoke_dir
  encoders="$($ffmpeg_output -hide_banner -encoders 2>/dev/null)"
  filters="$($ffmpeg_output -hide_banner -filters 2>/dev/null)"

  for encoder in libx264 aac pcm_s16be pcm_s24be; do
    if [[ "$encoders" != *"$encoder"* ]]; then
      echo "Bundled FFmpeg is missing required encoder: $encoder" >&2
      exit 1
    fi
  done

  for filter in ebur128 astats acompressor alimiter equalizer highpass lowpass; do
    if [[ "$filters" != *"$filter"* ]]; then
      echo "Bundled FFmpeg is missing required filter: $filter" >&2
      exit 1
    fi
  done

  smoke_dir="$(mktemp -d "${TMPDIR:-/tmp}/rau-studio-ffmpeg.XXXXXX")"
  trap 'rm -rf "$smoke_dir"' RETURN
  "$ffmpeg_output" \
    -hide_banner -loglevel error -f lavfi -i "sine=frequency=440:duration=0.1" \
    -c:a pcm_s16be "$smoke_dir/smoke.aiff"
  "$ffprobe_output" \
    -v error -select_streams a:0 -show_entries stream=codec_name -of csv=p=0 \
    "$smoke_dir/smoke.aiff" | grep -q "pcm_s16be"

  "$ffmpeg_output" \
    -hide_banner -loglevel error \
    -f lavfi -i "color=c=black:s=128x128:d=0.25" \
    -f lavfi -i "sine=frequency=440:duration=0.25" \
    -map 0:v:0 -map 1:a:0 -c:v libx264 -c:a aac -pix_fmt yuv420p \
    -movflags +faststart "$smoke_dir/smoke.mp4"
  "$ffprobe_output" \
    -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 \
    "$smoke_dir/smoke.mp4" | grep -q "h264"
  rm -rf "$smoke_dir"
  trap - RETURN
}

validate_sidecars() {
  validate_binary_architecture "$ffmpeg_output"
  validate_binary_architecture "$ffprobe_output"
  validate_native_features
}

mkdir -p "$cache_dir" "$binary_dir"

if [[ -x "$ffmpeg_output" && -x "$ffprobe_output" && -f "$version_marker" && \
      -f "$binary_dir/COPYING.FFMPEG-GPLv2" && -f "$binary_dir/COPYING.X264-GPLv2" ]] && \
   [[ "$(<"$version_marker")" == "$BUILD_ID" ]]; then
  validate_sidecars
  echo "FFmpeg $FFMPEG_VERSION sidecars are ready for $target_triple."
  exit 0
fi

if [[ ! -f "$archive" ]]; then
  echo "Downloading FFmpeg $FFMPEG_VERSION source from ffmpeg.org..."
  curl --fail --location --show-error \
    "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" \
    --output "$archive"
fi
verify_archive

if [[ ! -f "$x264_archive" ]]; then
  echo "Downloading x264 $X264_COMMIT source from VideoLAN..."
  curl --fail --location --show-error \
    "https://code.videolan.org/videolan/x264/-/archive/$X264_COMMIT/x264-$X264_COMMIT.tar.bz2" \
    --output "$x264_archive"
fi
verify_x264_archive

if [[ ! -d "$source_dir" ]]; then
  tar -xf "$archive" -C "$cache_dir"
fi
if [[ ! -d "$x264_source_dir" ]]; then
  tar -xf "$x264_archive" -C "$cache_dir"
fi

rm -rf "$x264_build_dir" "$x264_install_dir"
mkdir -p "$x264_build_dir" "$x264_install_dir"

x264_configure_flags=(
  "--prefix=$x264_install_dir"
  "--host=$target_triple"
  "--enable-static"
  "--enable-pic"
  "--disable-cli"
  "--disable-opencl"
  "--extra-cflags=-arch $target_arch -mmacosx-version-min=$MACOS_DEPLOYMENT_TARGET"
  "--extra-ldflags=-arch $target_arch -mmacosx-version-min=$MACOS_DEPLOYMENT_TARGET"
)

echo "Building x264 $X264_COMMIT for $target_triple..."
x264_configure_log="$x264_build_dir/configure.log"
if ! (
  cd "$x264_build_dir"
  CC=clang MACOSX_DEPLOYMENT_TARGET="$MACOS_DEPLOYMENT_TARGET" \
    "$x264_source_dir/configure" "${x264_configure_flags[@]}" > "$x264_configure_log" 2>&1
); then
  cat "$x264_configure_log" >&2
  exit 1
fi

jobs="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 4)"
x264_build_log="$x264_build_dir/build.log"
if ! make -s -C "$x264_build_dir" -j "$jobs" > "$x264_build_log" 2>&1; then
  tail -n 300 "$x264_build_log" >&2
  exit 1
fi
make -s -C "$x264_build_dir" install-lib-static >> "$x264_build_log" 2>&1

if [[ ! -f "$x264_install_dir/lib/libx264.a" || ! -f "$x264_install_dir/lib/pkgconfig/x264.pc" ]]; then
  echo "x264 static library installation is incomplete." >&2
  exit 1
fi

rm -rf "$build_dir"
mkdir -p "$build_dir"

configure_flags=(
  "--prefix=/rau-studio/ffmpeg"
  "--extra-version=rau-studio"
  "--arch=$target_arch"
  "--target-os=darwin"
  "--cc=clang"
  "--extra-cflags=-arch $target_arch -mmacosx-version-min=$MACOS_DEPLOYMENT_TARGET"
  "--extra-ldflags=-arch $target_arch -mmacosx-version-min=$MACOS_DEPLOYMENT_TARGET"
  "--disable-autodetect"
  "--disable-debug"
  "--disable-doc"
  "--disable-network"
  "--disable-shared"
  "--enable-static"
  "--disable-programs"
  "--enable-ffmpeg"
  "--enable-ffprobe"
  "--enable-gpl"
  "--enable-libx264"
  "--pkg-config-flags=--static"
  "--enable-audiotoolbox"
  "--enable-videotoolbox"
)

if [[ "$(rustc --print host-tuple)" != "$target_triple" ]]; then
  configure_flags+=("--enable-cross-compile")
fi

echo "Configuring FFmpeg $FFMPEG_VERSION for $target_triple..."
configure_log="$build_dir/configure.log"
if ! (
  cd "$build_dir"
  PKG_CONFIG_PATH="$x264_install_dir/lib/pkgconfig" \
    MACOSX_DEPLOYMENT_TARGET="$MACOS_DEPLOYMENT_TARGET" \
    "$source_dir/configure" "${configure_flags[@]}" > "$configure_log" 2>&1
); then
  cat "$configure_log" >&2
  exit 1
fi

jobs="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 4)"
echo "Building ffmpeg and ffprobe with $jobs jobs..."
build_log="$build_dir/build.log"
if ! make -s -C "$build_dir" -j "$jobs" ffmpeg ffprobe > "$build_log" 2>&1; then
  tail -n 300 "$build_log" >&2
  exit 1
fi

install -m 755 "$build_dir/ffmpeg" "$ffmpeg_output"
install -m 755 "$build_dir/ffprobe" "$ffprobe_output"
strip -x "$ffmpeg_output" "$ffprobe_output"
printf '%s\n' "$BUILD_ID" > "$version_marker"
install -m 644 "$source_dir/COPYING.GPLv2" "$binary_dir/COPYING.FFMPEG-GPLv2"
install -m 644 "$x264_source_dir/COPYING" "$binary_dir/COPYING.X264-GPLv2"

validate_sidecars
echo "FFmpeg $FFMPEG_VERSION sidecars are ready for $target_triple."
