#!/usr/bin/env bash
# The driver of `user32_check/src/bin/routes_agree/`: one call script through the route that asks
# the real USER32 and through the route that answers over `Desk`, each in a fresh Wine on a private
# screen, and `routes_agree.py` reads the logs side by side.
#
#   bash ../../layer-check/bench/wine.sh setup <work>   the toolchain's own linker, once
#   bash routes_agree.sh build <work>                   the 32-bit programs, into <work>/target
#   bash routes_agree.sh run <work> <out>               every arm, each scored from its logs in <out>
#   bash routes_agree.sh holds <out>                    the held lines' arms, no Wine, over those logs
#
# `run` exits 0 only if every arm came out as its line in `arms` says. A plant the scorer lets past
# would mean it cannot go red; a list that clears a difference it never saw would mean it clears
# anything. This reads neither `Native` under `Desk` nor a browser's key, and no clock's value.
set -euo pipefail

# shellcheck source=user32_check.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/user32_check.sh"

readonly SCORER="${HERE}/routes_agree.py"

# The `past` lines of the second score that the first has not.
new_pasts() {
  comm -13 <(grep '^past ' "$1" | sort || true) <(grep '^past ' "$2" | sort || true)
}

# The plant's score holds exactly one new `past`, the one its row names.
one_new_past() {
  local clean=$1 planted=$2 pattern=$3 new
  new=$(new_pasts "${clean}" "${planted}")
  [[ $(grep -c '' <<<"${new}") == 1 ]] && grep -Eq "${pattern}" <<<"${new}"
}

# Every one of the 4 known differences went `past` with the list off.
all_four_past() {
  local score=$1
  grep -q '^past GetAsyncKeyState v 1 0 step=keys.tap$' "${score}" &&
    grep -q '^past GetAsyncKeyState v 8001 8000 step=keys.held$' "${score}" &&
    grep -q '^past reached v 1 0 step=timer$' "${score}" &&
    grep -q '^past CreateWindowExA v ' "${score}"
}

no_past() {
  ! grep -q '^past ' "$1"
}

# The listed differences only the real route's log against the other route's can show.
three_stale() {
  local score=$1
  grep -q '^stale the tap$' "${score}" &&
    grep -q '^stale 8001$' "${score}" &&
    grep -q "^stale the timer's window$" "${score}"
}

# Says whether an arm came out as its line says, and keeps the tally.
told() {
  local label=$1
  shift
  if "$@"; then
    printf 'arm %s: as expected\n' "${label}"
  else
    printf 'arm %s: NOT as expected\n' "${label}"
    wrong=$((wrong + 1))
  fi
}

score() {
  local out=$1 name=$2
  shift 2
  python3 "${SCORER}" "$@" >"${out}/${name}.txt"
}

# Runs on the private screen, by `run`.
arms() {
  local work=$1 out=$2
  local exe="${work}/target/${TARGET}/debug/routes_agree.exe"
  local wrong=0
  if [[ -z "${DISPLAY:-}" || "${DISPLAY}" == "${OUTER_DISPLAY:-}" || -n "${WAYLAND_DISPLAY:-}" ]]; then
    err "not on a private screen: DISPLAY=${DISPLAY:-} WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-}"
    return 1
  fi
  if [[ "${WINEPREFIX:-}" != "${work}"/* ]]; then
    err "WINEPREFIX is not under ${work}"
    return 1
  fi
  arm "${exe}" "${work}" thin none yes "${out}/thin.log"
  arm "${exe}" "${work}" desk none yes "${out}/desk.log"
  arm "${exe}" "${work}" desk wrong-metric yes "${out}/desk-wrong-metric.log"
  arm "${exe}" "${work}" desk no-line yes "${out}/desk-no-line.log"
  arm "${exe}" "${work}" thin none yes "${out}/thin-again.log"

  local list_off=(--list-off "${out}/thin.log" "${out}/desk.log")
  local again=(--other-route thin "${out}/thin.log" "${out}/thin-again.log")
  told clean score "${out}" clean "${out}/thin.log" "${out}/desk.log"
  score "${out}" wrong-metric "${out}/thin.log" "${out}/desk-wrong-metric.log" || true
  score "${out}" no-line "${out}/thin.log" "${out}/desk-no-line.log" || true
  score "${out}" list-off "${list_off[@]}" || true
  score "${out}" again "${again[@]}" || true
  told wrong-metric one_new_past "${out}/clean.txt" "${out}/wrong-metric.txt" \
    '^past GetSystemMetrics v 20 21 step=metrics$'
  told no-line one_new_past "${out}/clean.txt" "${out}/no-line.txt" \
    '^past ShowWindow line missing-on-desk '
  told list-off all_four_past "${out}/list-off.txt"
  told control no_past "${out}/again.txt"
  told stale-list three_stale "${out}/again.txt"
  printf '%s arms not as expected\n' "${wrong}"
  [[ "${wrong}" == 0 ]]
}

# A copy of a `Desk` log in which step `classes`' first `RegisterClassA` answers another atom.
move_atom() {
  awk '/^step / { step = $2 }
    step == "classes" && /c=USER32!RegisterClassA / && !moved { sub(/ v=[^ ]* /, " v=1234 "); moved = 1 }
    { print }' "$1" >"$2"
}

# A copy of a log with step `queue`'s first `PeekMessageA` line gone.
take_one_peek() {
  awk '/^step / { step = $2 }
    step == "queue" && /c=USER32!PeekMessageA / && !taken { taken = 1; next }
    { print }' "$1" >"$2"
}

# The moved answer is the one new `past`, and its hold goes `stale`.
atom_went_past() {
  local clean=$1 planted=$2
  one_new_past "${clean}" "${planted}" '^past RegisterClassA v c02b 1234 step=classes$' &&
    grep -q "^stale the class's atom$" "${planted}"
}

# A hold is a difference let through, so these read that it lets through nothing else: the scorer
# alone, over copies of a run's logs. One held answer moved, and one held line taken away.
holds() {
  local out=$1
  local wrong=0
  move_atom "${out}/desk.log" "${out}/desk-atom-moved.log"
  take_one_peek "${out}/thin.log" "${out}/thin-one-peek-less.log"
  score "${out}" atom-moved "${out}/thin.log" "${out}/desk-atom-moved.log" || true
  score "${out}" peek-less "${out}/thin-one-peek-less.log" "${out}/desk.log" || true
  told atom-moved atom_went_past "${out}/clean.txt" "${out}/atom-moved.txt"
  told peek-less one_new_past "${out}/clean.txt" "${out}/peek-less.txt" \
    '^past PeekMessageA lines 62 63 step=queue$'
  printf '%s hold arms not as expected\n' "${wrong}"
  [[ "${wrong}" == 0 ]]
}

run() {
  local work=$1 out=$2
  mkdir -p "${out}"
  OUTER_DISPLAY="${DISPLAY:-}" \
    bash "${WINE_SH}" under "${work}" bash "${BASH_SOURCE[0]}" arms "${work}" "${out}"
  holds "${out}"
}

main() {
  local verb=${1:-}
  [[ $# -gt 0 ]] && shift
  case "${verb}" in
    build) build "$@" ;;
    run) run "$@" ;;
    arms) arms "$@" ;;
    holds) holds "$@" ;;
    *)
      err "usage: routes_agree.sh build <work> | run <work> <out> | holds <out>"
      return 2
      ;;
  esac
}

main "$@"
