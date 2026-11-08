#!/usr/bin/env bash
# C9's private place to listen: run by `bash null_sink.sh`. It adds a sound output of its
# own that reaches no speaker, records what arrives there, runs a command that plays
# into it (`PULSE_SINK` names it), and removes it on every way out but a kill it cannot see.
#
#   null_sink.sh <recording.raw> <clean|muted> <command> [arguments]
#
# `muted` silences the recording, to show C9's red. The computer's default output, default
# input and volumes are never touched: the script reads them before and after, and exits
# non-zero if they moved. Prints on stdout one `stream` line for each new sound stream, on
# any output, naming that output's index and name, its program and its process, then
# `others_before=N`, the streams already on other outputs. A process is this run's when its own environment names the private
# output; `ours=unread` means it could not be read, so nobody knows. A stream of this run on
# another output could reach the room: the command is stopped at once, and the exit is 5.
# Read 10 times a second, so a stream shorter than that goes unseen.
#
# The gate: the command's stdin stays silent until its stream is read on the private output,
# then carries one line, `go`. A command that makes sound opens its stream, waits for that
# line, and only then makes any, so a stream that lands elsewhere has nothing to play.
set -euo pipefail

readonly RATE=48000
readonly SINK="layer_check_$$"

module=""
capture_pid=""
command_pid=""
gate_open=""
declare -A seen=()

err() {
  printf '%s\n' "$*" >&2
}

# Stops the command and all it started, then waits: an output removed under a live stream
# could send that stream to the speakers.
stop_command() {
  local tries=0
  kill -TERM -- "-${command_pid}" 2>/dev/null || true
  while kill -0 -- "-${command_pid}" 2>/dev/null; do
    tries=$((tries + 1))
    if ((tries > 30)); then
      kill -KILL -- "-${command_pid}" 2>/dev/null || true
      break
    fi
    sleep 0.1
  done
  wait "${command_pid}" 2>/dev/null || true
}

cleanup() {
  local code=$?
  trap - EXIT TERM INT
  [[ -z "${command_pid}" ]] || stop_command
  [[ -z "${capture_pid}" ]] || kill "${capture_pid}" 2>/dev/null || true
  if [[ -n "${module}" ]]; then
    pactl unload-module "${module}" || code=1
  fi
  exit "${code}"
}

# The computer's settings, as one line. Inside `private_sound.sh` there are none to guard: that
# server starts with no output, so the first one added becomes its default, and the computer's
# own server is read from outside it.
box_state() {
  if [[ -n "${PRIVATE_SOUND:-}" ]]; then
    printf 'private\n'
    return 0
  fi
  local sink
  sink=$(pactl get-default-sink)
  printf '%s|%s|%s\n' "${sink}" "$(pactl get-default-source)" "$(pactl get-sink-volume "${sink}")"
}

# Each sound stream as one line: its id, its output, its program, its name, its process.
streams() {
  LC_ALL=C pactl list sink-inputs | awk '
    function flush() {
      if (id != "") print id, sink, binary, name, pid
    }
    function quoted(line, part) {
      split(line, part, "\"")
      gsub(/[[:space:]]+/, "_", part[2])
      return part[2] == "" ? "-" : part[2]
    }
    /^Sink Input #/ {
      flush()
      id = substr($3, 2); sink = "-"; binary = "-"; name = "-"; pid = "-"
    }
    /^[[:space:]]+Sink: / { sink = $2 }
    /application\.process\.binary = / { binary = quoted($0) }
    /application\.name = / { name = quoted($0) }
    /application\.process\.id = / { pid = quoted($0) }
    END { flush() }
  '
}

process_env() {
  { tr '\0' '\n' <"/proc/$1/environ" | sed -n "s/^$2=//p"; } 2>/dev/null || true
}

stream_owner() {
  local pid=$1
  if [[ ! -r "/proc/${pid}/environ" ]]; then
    printf 'unread\n'
  elif [[ "$(process_env "${pid}" PULSE_SINK)" == "${SINK}" ]]; then
    printf 'yes\n'
  else
    printf 'no\n'
  fi
}

# The name of the output with this index, or `-` when no output has it.
sink_name() {
  LC_ALL=C pactl list short sinks | awk -v wanted="$1" '
    $1 == wanted { found = $2 }
    END { print found == "" ? "-" : found }
  '
}

# Prints a stream the first time it is read on an output, so a move prints again.
note_streams() {
  local own=$1
  local list id sink binary name pid where prefix ours sink_label
  list=$(streams) || return 1
  while read -r id sink binary name pid; do
    [[ -n "${id}" && -z "${seen["${id}:${sink}"]:-}" ]] || continue
    seen["${id}:${sink}"]=1
    where=other
    [[ "${sink}" != "${own}" ]] || where=private
    ours=$(stream_owner "${pid}")
    prefix=$(process_env "${pid}" WINEPREFIX)
    prefix=${prefix// /_}
    sink_label=$(sink_name "${sink}")
    printf 'stream id=%s on=%s sink=%s sink_name=%s binary=%s name=%s pid=%s prefix=%s ours=%s\n' \
      "${id}" "${where}" "${sink}" "${sink_label}" "${binary}" "${name}" "${pid}" "${prefix:--}" "${ours}"
    if [[ "${where}" == "other" && "${ours}" == "yes" ]]; then
      err "a stream of this run reached another output: stopping it now"
      return 5
    fi
    if [[ "${where}" == "private" && "${ours}" == "yes" && -z "${gate_open}" ]]; then
      gate_open=1
      printf 'go\n' >&9
    fi
  done <<<"${list}"
}

main() {
  if (($# < 3)); then
    err "usage: null_sink.sh <recording.raw> <clean|muted> <command> [arguments]"
    return 2
  fi
  local recording=$1 mode=$2
  shift 2
  local before own others_before code=0
  before=$(box_state)
  if ! module=$(pactl load-module module-null-sink "sink_name=${SINK}" \
    "rate=${RATE}" channels=1 format=s16le); then
    module=""
    err "the system refused a private sound output"
    return 3
  fi
  if [[ "$(box_state)" != "${before}" ]]; then
    err "adding a private output moved the computer's settings"
    return 4
  fi
  own=$(pactl list short sinks | awk -v name="${SINK}" '$2 == name {print $1}')
  if [[ "${mode}" == "muted" ]]; then
    pactl set-source-mute "${SINK}.monitor" 1
  fi
  parec --device="${SINK}.monitor" --rate="${RATE}" --format=s16le --channels=1 --raw \
    --latency-msec=50 >"${recording}" &
  capture_pid=$!
  sleep 0.3
  note_streams "${own}" >/dev/null
  others_before=$(streams | awk -v own="${own}" '$2 != own {n++} END {print n + 0}')
  mkfifo "${recording}.gate"
  exec 9<>"${recording}.gate"
  rm -f "${recording}.gate"
  PULSE_SINK="${SINK}" setsid "$@" <&9 &
  command_pid=$!
  while kill -0 "${command_pid}" 2>/dev/null; do
    note_streams "${own}" || return $?
    sleep 0.1
  done
  wait "${command_pid}" || code=$?
  command_pid=""
  sleep 0.3
  kill "${capture_pid}" 2>/dev/null || true
  wait "${capture_pid}" 2>/dev/null || true
  capture_pid=""
  pactl unload-module "${module}"
  module=""
  if [[ "$(box_state)" != "${before}" ]]; then
    err "the computer's settings moved during the run"
    return 4
  fi
  printf 'others_before=%s\n' "${others_before}"
  return "${code}"
}

trap cleanup EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
main "$@"
