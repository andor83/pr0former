#!/bin/bash
# Called by build.sh; build an offline-capable LGPL helper, never use Homebrew's binary.
set -euo pipefail
build_root=$1
jobs=$2
version=8.1.1
sha=b6863adde98898f42602017462871b5f6333e65aec803fdd7a6308639c52edf3
archive="$build_root/ffmpeg-$version.tar.xz"
source_dir="$build_root/ffmpeg-$version"
executable="$source_dir/ffmpeg"
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) executable="$executable.exe" ;; esac
python_command=python3
command -v "$python_command" >/dev/null 2>&1 || python_command=python
mkdir -p "$build_root"
if [[ ! -f "$archive" ]]; then
  curl --fail --location --retry 3 "https://ffmpeg.org/releases/ffmpeg-$version.tar.xz" -o "$archive.tmp"
  mv "$archive.tmp" "$archive"
fi
"$python_command" - "$archive" "$sha" <<'PY'
import hashlib,sys
with open(sys.argv[1], 'rb') as f: actual=hashlib.file_digest(f,'sha256').hexdigest() if hasattr(hashlib,'file_digest') else hashlib.sha256(f.read()).hexdigest()
if actual != sys.argv[2]: raise SystemExit('FFmpeg source checksum mismatch; remove the cached archive and retry')
PY
if [[ ! -f "$source_dir/.pr0-built-v1" ]]; then
  tar -xf "$archive" -C "$build_root"
  (
    cd "$source_dir"
    if ! ./configure --disable-autodetect --disable-gpl --disable-nonfree --disable-version3 \
        --disable-shared --enable-static --disable-doc --disable-debug --disable-ffplay --disable-ffprobe \
        --disable-network --disable-x86asm --disable-encoders --enable-encoder=pcm_f32le \
        --disable-muxers --enable-muxer=wav --disable-devices > pr0-configure.log 2>&1; then
      echo 'FFmpeg configuration failed; last 200 log lines:' >&2
      tail -n 200 pr0-configure.log >&2
      exit 1
    fi
    if ! make -j "$jobs" ffmpeg > pr0-build.log 2>&1; then
      echo 'FFmpeg compilation failed; last 200 log lines:' >&2
      tail -n 200 pr0-build.log >&2
      exit 1
    fi
    touch .pr0-built-v1
  )
fi
"$executable" -protocols 2>/dev/null | "$python_command" -c 'import sys; assert "fd" in sys.stdin.read().split(), "FFmpeg fd protocol is required"'
