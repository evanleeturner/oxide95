#!/usr/bin/env bash
# The bench's Wine recipe: run by `bash wine.sh`, and touching only what lies under the
# working folder it is given, so other programs' Wine on this computer is left alone.
#
#   setup <work> [version]   fetch the toolchain's own linker, matched to `rustc --version`
#   link <work> <gnu|lld> <file.rs> <out.exe>
#   run <work> <file.exe> [arguments]
#   under <work> <command...>   run a command on the private screen, which `run` does
#                               not hand out, so it can read that screen from outside
#
# LAYER_CHECK_SCREEN sets the private screen, as Xvfb's WxHxD: 640x480x16 if unset.
# LAYER_CHECK_SCREEN_LIMIT_S sets how long it may live, in seconds: 300 if unset.
#
# The fetched linker is never committed. `rust-lld` is kept only to show why it is not the
# recipe: a program linked by it dies at a panic it means to catch.
set -euo pipefail

readonly TARGET=i686-pc-windows-gnu
readonly MINGW_LIBS=/usr/i686-w64-mingw32/lib
readonly XVFB_LIMIT_S="${LAYER_CHECK_SCREEN_LIMIT_S:-300}"
readonly SCREEN="${LAYER_CHECK_SCREEN:-640x480x16}"

xvfb_pid=""
prefix=""

err() {
  printf '%s\n' "$*" >&2
}

toolchain_root() {
  local work=$1
  printf '%s\n' \
    "${work}/mingw/rust-mingw-$(rust_version)-${TARGET}/rust-mingw/lib/rustlib/${TARGET}"
}

rust_version() {
  rustc --version | awk '{print $2}'
}

setup() {
  local work=$1 asked=${2:-}
  local version name url tar_file sum_file want got
  version=$(rust_version)
  if [[ -n "${asked}" && "${asked}" != "${version}" ]]; then
    err "asked for rust-mingw ${asked}, but rustc is ${version}: refusing a mismatch"
    return 1
  fi
  name="rust-mingw-${version}-${TARGET}.tar.xz"
  url="https://static.rust-lang.org/dist/${name}"
  mkdir -p "${work}/download"
  tar_file="${work}/download/${name}"
  sum_file="${tar_file}.sha256"
  [[ -f "${tar_file}" ]] || curl --fail --silent --show-error --location -o "${tar_file}" "${url}"
  [[ -f "${sum_file}" ]] || curl --fail --silent --show-error --location -o "${sum_file}" "${url}.sha256"
  want=$(awk '{print $1}' "${sum_file}")
  got=$(sha256sum "${tar_file}" | awk '{print $1}')
  if [[ "${want}" != "${got}" ]]; then
    err "${name}: its checksum reads ${got}, and the published one is ${want}"
    return 1
  fi
  rm -rf "${work}/mingw"
  mkdir -p "${work}/mingw"
  tar -xJf "${tar_file}" -C "${work}/mingw"
  printf 'rust_mingw=%s sha256=%s\n' "${version}" "${got}"
}

stop_wine() {
  if [[ -n "${prefix}" ]]; then
    WINEPREFIX="${prefix}" wineserver -k 2>/dev/null || true
  fi
  if [[ -n "${xvfb_pid}" ]]; then
    kill "${xvfb_pid}" 2>/dev/null || true
    wait "${xvfb_pid}" 2>/dev/null || true
  fi
}

# Starts a private screen, for as long as the limit at most, and points Wine at a private
# folder.
start_wine() {
  local work=$1
  local display_file="${work}/display.$$" tries=0 screen
  prefix="${work}/wineprefix"
  export WINEPREFIX="${prefix}" WINEDEBUG=-all
  trap stop_wine EXIT
  trap 'exit 143' TERM
  trap 'exit 130' INT
  : >"${display_file}"
  timeout "${XVFB_LIMIT_S}" Xvfb -displayfd 3 -screen 0 "${SCREEN}" 3>"${display_file}" 2>/dev/null &
  xvfb_pid=$!
  while [[ ! -s "${display_file}" ]]; do
    tries=$((tries + 1))
    if ((tries > 100)); then
      err "the private screen did not start in 10 s"
      return 1
    fi
    sleep 0.1
  done
  screen=$(<"${display_file}")
  export DISPLAY=":${screen}"
  # A native program takes a Wayland name before `DISPLAY`: left set, it would open on the
  # computer's real desktop and not on this private screen.
  unset WAYLAND_DISPLAY WAYLAND_SOCKET
  rm -f "${display_file}"
}

link() {
  local work=$1 kind=$2 file=$3 out=$4
  local root
  root=$(toolchain_root "${work}")
  local flags=(--edition 2024 --target "${TARGET}" -C link-self-contained=yes
    -L "${root}/lib/self-contained" -L "${MINGW_LIBS}" -o "${out}")
  case "${kind}" in
    gnu)
      start_wine "${work}"
      local wrapper="${work}/wine-gcc"
      printf '#!/usr/bin/env bash\nexec wine "%s" "$@"\n' \
        "${root}/bin/self-contained/i686-w64-mingw32-gcc.exe" >"${wrapper}"
      chmod +x "${wrapper}"
      flags+=(-C "linker=${wrapper}" -C linker-flavor=gcc
        -C "link-arg=-B${root}/bin/self-contained")
      ;;
    lld)
      flags+=(-C linker=rust-lld)
      ;;
    *)
      err "link takes gnu or lld, not ${kind}"
      return 2
      ;;
  esac
  rustc "${flags[@]}" "${file}"
}

run_exe() {
  local work=$1 exe=$2
  shift 2
  start_wine "${work}"
  wine "${exe}" "$@"
}

under() {
  local work=$1
  shift
  start_wine "${work}"
  "$@"
}

main() {
  local verb=${1:-}
  [[ $# -gt 0 ]] && shift
  case "${verb}" in
    setup) setup "$@" ;;
    link) link "$@" ;;
    run) run_exe "$@" ;;
    under) under "$@" ;;
    *)
      err "usage: wine.sh setup|link|run|under <work> ..."
      return 2
      ;;
  esac
}

main "$@"
