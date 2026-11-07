#!/usr/bin/env bash
# The driver of `user32_check/`: asks the real USER32 through `window::user32` and
# `window_input::user32` beside calls declared by hand, under Wine, on a private screen.
#
#   bash ../../layer-check/bench/wine.sh setup <work>   the toolchain's own linker, once
#   build <work>        the 32-bit program, into <work>/target
#   run <work> <out>    every arm, clean and planted, each scored from its log in <out>
#
# `run` exits 0 only if every arm came out as its row below says. A planted arm that passes
# would mean its check cannot go red; the no-focus arm must blame the computer and not the thin
# call, which is what the hand-declared calls are for. Nobody looks at the screen or presses a
# key: each key comes from `xdotool`, which enters past the device.
set -euo pipefail

here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
readonly HERE="${here}"
readonly WINE_SH="${HERE}/../../layer-check/bench/wine.sh"
readonly TARGET=i686-pc-windows-gnu
readonly ARM_LIMIT_S=60

# Wine opens a sound probe at each start: no server to reach, and its sound drivers off.
export LAYER_CHECK_SCREEN=1024x768x24
export PULSE_SERVER=/nonexistent/pulse
export WINEDLLOVERRIDES="winepulse.drv=d;winealsa.drv=d;mmdevapi=d;mscoree=d;mshtml=d"
# `wine.sh` sets a private DISPLAY alone: a program left this would reach a real desktop.
unset WAYLAND_DISPLAY

err() {
  printf '%s\n' "$*" >&2
}

build() {
  local work=$1
  local version root wrapper
  version=$(rustc --version | awk '{print $2}')
  root="${work}/mingw/rust-mingw-${version}-${TARGET}/rust-mingw/lib/rustlib/${TARGET}"
  wrapper="${work}/wine-gcc"
  printf '#!/usr/bin/env bash\nexec wine "%s" "$@"\n' \
    "${root}/bin/self-contained/i686-w64-mingw32-gcc.exe" >"${wrapper}"
  chmod +x "${wrapper}"
  CARGO_TARGET_DIR="${work}/target" CARGO_INCREMENTAL=0 \
    CARGO_TARGET_I686_PC_WINDOWS_GNU_LINKER="${wrapper}" \
    RUSTFLAGS="-C linker-flavor=gcc -C link-self-contained=yes -C link-arg=-B${root}/bin/self-contained -L ${root}/lib/self-contained -L /usr/i686-w64-mingw32/lib" \
    bash "${WINE_SH}" under "${work}" \
    cargo build --locked --manifest-path "${HERE}/user32_check/Cargo.toml" --target "${TARGET}"
}

# Does what the program asks, and writes the answer it waits for.
answer() {
  local request=$1 focus=$2
  case "${request}" in
    focus)
      if [[ "${focus}" == yes ]]; then
        xdotool search --name '^uc keys$' windowfocus >/dev/null
      fi
      printf 'done\n'
      ;;
    location)
      xdotool getmouselocation | sed -E 's/^x:([0-9]+) y:([0-9]+).*/\1 \2/'
      ;;
    *)
      # A request is words for xdotool, split on purpose.
      # shellcheck disable=SC2086
      xdotool ${request} >/dev/null
      printf 'done\n'
      ;;
  esac
}

# One run of the program, its lines into a log, its asks answered.
arm() {
  local exe=$1 work=$2 check=$3 plant=$4 focus=$5 log=$6
  local fifo="${work}/ask.$$" line
  rm -f "${fifo}"
  mkfifo "${fifo}"
  exec 4<>"${fifo}"
  : >"${log}"
  while IFS= read -r line; do
    printf '%s\n' "${line}" >>"${log}"
    case "${line}" in
      ask\ *) answer "${line#ask }" "${focus}" >&4 ;;
    esac
  done < <(timeout "${ARM_LIMIT_S}" wine "${exe}" "${check}" "${plant}" <"${fifo}" 2>&1)
  exec 4>&-
  rm -f "${fifo}"
  # Each arm starts from a fresh Wine, so one arm's cursor or keys cannot pass for the next's.
  wineserver -k 2>/dev/null || true
  wineserver -w 2>/dev/null || true
}

count() {
  grep -c "$1" "$2" || true
}

# Whether an arm's log came out as its row says.
scored() {
  local upper=$1 want=$2 name=$3 log=$4
  local thin control
  thin=$(count '^red thin ' "${log}")
  control=$(count '^red control ' "${log}")
  case "${want}" in
    pass)
      [[ "${thin}" == 0 && "${control}" == 0 ]] &&
        grep -q "^${upper} pass$" "${log}" &&
        grep -Eq "^${upper} names ([0-9]+) of \1$" "${log}"
      ;;
    thin)
      [[ "${control}" == 0 ]] &&
        grep -q "^red thin ${upper} ${name} " "${log}" &&
        grep -q "^${upper} red " "${log}"
      ;;
    control)
      [[ "${thin}" == 0 && "${control}" != 0 ]] &&
        grep -q "^${upper} red " "${log}"
      ;;
    *)
      err "no outcome is named ${want}"
      return 2
      ;;
  esac
}

# Runs on the private screen, by `run`.
arms() {
  local work=$1 out=$2
  local exe="${work}/target/${TARGET}/debug/user32_check.exe"
  local check plant focus want name label log upper wrong=0 total=0
  if [[ -z "${DISPLAY:-}" || "${DISPLAY}" == "${OUTER_DISPLAY:-}" || -n "${WAYLAND_DISPLAY:-}" ]]; then
    err "not on a private screen: DISPLAY=${DISPLAY:-} WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-}"
    return 1
  fi
  if [[ "${WINEPREFIX:-}" != "${work}"/* ]]; then
    err "WINEPREFIX is not under ${work}"
    return 1
  fi
  while read -r check plant focus want name <&5; do
    label="${check}-${plant}-focus-${focus}"
    log="${out}/${label}.log"
    upper=${check^^}
    arm "${exe}" "${work}" "${check}" "${plant}" "${focus}" "${log}"
    total=$((total + 1))
    if scored "${upper}" "${want}" "${name}" "${log}"; then
      printf 'arm %s wanted %s %s: as expected\n' "${label}" "${want}" "${name}"
    else
      printf 'arm %s wanted %s %s: NOT as expected\n' "${label}" "${want}" "${name}"
      wrong=$((wrong + 1))
    fi
  done 5<<'ARMS'
wu1 none yes pass -
wu1 swap-xy yes thin ScreenToClient
wu1 no-line yes thin GetSystemMetrics
wu1 wrong-by yes thin FindWindowA
wu1 no-kept yes thin DestroyWindow
wu2 none yes pass -
wu2 keep-message yes thin PeekMessageA
wu2 no-timer yes control PeekMessageA
wu3 none yes pass -
wu3 key-off-by-one yes thin GetAsyncKeyState
wu3 none no control -
ARMS
  printf '%s arms, %s not as expected\n' "${total}" "${wrong}"
  [[ "${wrong}" == 0 ]]
}

run() {
  local work=$1 out=$2
  mkdir -p "${out}"
  OUTER_DISPLAY="${DISPLAY:-}" \
    bash "${WINE_SH}" under "${work}" bash "${BASH_SOURCE[0]}" arms "${work}" "${out}"
}

main() {
  local verb=${1:-}
  [[ $# -gt 0 ]] && shift
  case "${verb}" in
    build) build "$@" ;;
    run) run "$@" ;;
    arms) arms "$@" ;;
    *)
      err "usage: user32_check.sh build <work> | run <work> <out>"
      return 2
      ;;
  esac
}

main "$@"
