#!/usr/bin/env bash
# C9's private place to listen: run by `bash null_sink.sh`. It adds a sound output of its
# own that reaches no speaker, records what arrives there, runs a command that plays
# into it (`PULSE_SINK` names it), and removes it on every way out but a kill it cannot see.
#
#   null_sink.sh <recording.raw> <clean|muted> <command> [arguments]
#
# `muted` silences the recording, to show C9's red. The computer's default output, default
# input and volumes are never touched: the script reads them before and after, and exits
# non-zero if they moved. Prints `others_before=N others_max=N` on stdout, the sound
# streams on any other output before and during the command.
set -euo pipefail

readonly RATE=48000
readonly SINK="layer_check_$$"

module=""
capture_pid=""
command_pid=""

err() {
  printf '%s\n' "$*" >&2
}

cleanup() {
  local code=$?
  trap - EXIT TERM INT
  [[ -z "${command_pid}" ]] || kill "${command_pid}" 2>/dev/null || true
  [[ -z "${capture_pid}" ]] || kill "${capture_pid}" 2>/dev/null || true
  if [[ -n "${module}" ]]; then
    pactl unload-module "${module}" || code=1
  fi
  exit "${code}"
}

box_state() {
  local sink
  sink=$(pactl get-default-sink)
  printf '%s|%s|%s\n' "${sink}" "$(pactl get-default-source)" "$(pactl get-sink-volume "${sink}")"
}

other_streams() {
  local own=$1
  pactl list short sink-inputs | awk -v own="${own}" '$2 != own {n++} END {print n + 0}'
}

main() {
  if (($# < 3)); then
    err "usage: null_sink.sh <recording.raw> <clean|muted> <command> [arguments]"
    return 2
  fi
  local recording=$1 mode=$2
  shift 2
  local before own others_before others_max count code=0
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
  others_before=$(other_streams "${own}")
  others_max=${others_before}
  PULSE_SINK="${SINK}" "$@" &
  command_pid=$!
  while kill -0 "${command_pid}" 2>/dev/null; do
    count=$(other_streams "${own}")
    ((count <= others_max)) || others_max=${count}
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
  printf 'others_before=%s others_max=%s\n' "${others_before}" "${others_max}"
  return "${code}"
}

trap cleanup EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
main "$@"
