#!/usr/bin/env bash
# A sound server of this run's own: run by `bash private_sound.sh`. A program inside finds no
# real output or input to name, because this server has no device module at all.
#
#   private_sound.sh <command> [arguments]
#
# It starts its own `pipewire`, `wireplumber` and `pipewire-pulse` in a new folder, each on
# its own settings and socket, runs the command inside, then stops them all and removes the
# folder, on every way out but a kill it cannot see. The computer's own sound server is
# never stopped, restarted or changed.
#
# The inside names the folder in every variable a sound client reads: a dead `PULSE_SERVER`
# alone silenced Firefox but may not bind a program using PipeWire's own ALSA plugin. The
# session bus is left out, as device reservation and realtime go by it, and so are the real
# screen's names, so nothing inside opens on the real desktop. `PRIVATE_SOUND=1` tells a script
# inside that the server it reads is this one.
#
# Exits with the command's own code, or 3 when a server will not start or a device shows up.
# Written against wireplumber 0.4's Lua settings: on any other it refuses, by name.
set -euo pipefail

readonly READY_TRIES=100
readonly COMMAND_STOP_TRIES=60
readonly SERVER_STOP_TRIES=30
# A socket's path holds 108 bytes with its closing zero.
readonly LONGEST_SOCKET=107
readonly STOCK_SESSION=/usr/share/wireplumber
readonly HARDWARE_LINE='"type": "PipeWire:Interface:Device"|"api\.(alsa|bluez5|v4l2|libcamera)[.a-z]*":'

root=""
command_pid=""
server_pids=()
inside=()

err() {
  printf '%s\n' "$*" >&2
}

write_server_settings() {
  local libraries='context.spa-libs = {
  audio.convert.* = audioconvert/libspa-audioconvert
  support.* = support/libspa-support
}'
  cat >"${root}/pipewire/pipewire.conf" <<EOF
context.properties = {
  core.daemon = true
  core.name = pipewire-0
}
${libraries}
context.modules = [
  { name = libpipewire-module-protocol-native }
  { name = libpipewire-module-metadata }
  { name = libpipewire-module-spa-node-factory }
  { name = libpipewire-module-client-node }
  { name = libpipewire-module-client-device }
  { name = libpipewire-module-access }
  { name = libpipewire-module-adapter }
  { name = libpipewire-module-link-factory }
  { name = libpipewire-module-session-manager }
]
context.objects = [
  { factory = spa-node-factory
    args = { factory.name = support.node.driver node.name = Dummy-Driver
             node.group = pipewire.dummy priority.driver = 20000 } }
  { factory = spa-node-factory
    args = { factory.name = support.node.driver node.name = Freewheel-Driver
             node.group = pipewire.freewheel node.freewheel = true priority.driver = 19000 } }
]
EOF
  cat >"${root}/pipewire/pipewire-pulse.conf" <<EOF
${libraries}
context.modules = [
  { name = libpipewire-module-protocol-native }
  { name = libpipewire-module-client-node }
  { name = libpipewire-module-adapter }
  { name = libpipewire-module-metadata }
  { name = libpipewire-module-protocol-pulse args = { server.address = [ "unix:native" ] } }
]
EOF
  cat >"${root}/pipewire/client.conf" <<EOF
${libraries}
context.modules = [
  { name = libpipewire-module-protocol-native }
  { name = libpipewire-module-client-node }
  { name = libpipewire-module-client-device }
  { name = libpipewire-module-adapter }
  { name = libpipewire-module-metadata }
  { name = libpipewire-module-session-manager }
]
EOF
  cp "${root}/pipewire/client.conf" "${root}/pipewire/client-rt.conf"
}

# The stock session settings, less everything that finds hardware: its monitors are enabled
# only by the file this replaces, and their libraries are not named. The flatpak portal goes
# too: it asks for the session bus, left out here, and wireplumber exits when it cannot have it.
write_session_settings() {
  local name
  mkdir -p "${root}/wireplumber/main.lua.d" "${root}/wireplumber/policy.lua.d"
  for name in 00-functions 20-default-access 40-device-defaults 40-stream-defaults \
    50-default-access-config; do
    cp "${STOCK_SESSION}/main.lua.d/${name}.lua" "${root}/wireplumber/main.lua.d/"
  done
  for name in 00-functions 10-default-policy 50-endpoints-config 90-enable-all; do
    cp "${STOCK_SESSION}/policy.lua.d/${name}.lua" "${root}/wireplumber/policy.lua.d/"
  done
  cat >"${root}/wireplumber/main.lua.d/90-enable-all.lua" <<'EOF'
default_access.properties["enable-flatpak-portal"] = false
load_module("metadata")
default_access.enable()
device_defaults.enable()
stream_defaults.enable()
load_script("suspend-node.lua")
EOF
  cat >"${root}/wireplumber/wireplumber.conf" <<'EOF'
context.properties = {
  wireplumber.script-engine = lua-scripting
}
context.spa-libs = {
  audio.convert.* = audioconvert/libspa-audioconvert
  support.* = support/libspa-support
}
context.modules = [
  { name = libpipewire-module-protocol-native }
  { name = libpipewire-module-client-node }
  { name = libpipewire-module-client-device }
  { name = libpipewire-module-adapter }
  { name = libpipewire-module-metadata }
  { name = libpipewire-module-session-manager }
  { name = libpipewire-module-spa-node-factory }
]
wireplumber.components = [
  { name = libwireplumber-module-lua-scripting, type = module }
  { name = main.lua, type = config/lua }
  { name = policy.lua, type = config/lua }
]
EOF
}

# Waits for a check to pass, naming what never came: a refusal is the caller's to report.
await() {
  local what=$1 tries=0
  shift
  until "$@" >/dev/null 2>&1; do
    tries=$((tries + 1))
    if ((tries > READY_TRIES)); then
      err "${what} did not come up in $((READY_TRIES / 10)) s"
      tail -n 5 "${root}/servers.log" >&2 || true
      return 3
    fi
    sleep 0.1
  done
}

session_manager_up() {
  [[ "$("${inside[@]}" pw-dump 2>/dev/null)" == *'"WirePlumber"'* ]]
}

start_server() {
  "${inside[@]}" "$@" >>"${root}/servers.log" 2>&1 &
  server_pids+=("$!")
}

start_servers() {
  start_server pipewire
  await pipewire test -S "${root}/run/pipewire-0"
  start_server wireplumber
  await wireplumber session_manager_up
  start_server pipewire-pulse
  await pipewire-pulse "${inside[@]}" pactl info
}

# How many devices, and nodes that speak to hardware, the server lists. Prints nothing if the
# server cannot be read, which no caller may take for none.
hardware_count() {
  local dump
  dump=$("${inside[@]}" pw-dump 2>/dev/null) || return 0
  [[ -n "${dump}" ]] || return 0
  grep -cE "${HARDWARE_LINE}" <<<"${dump}" || true
}

require_no_hardware() {
  local count
  count=$(hardware_count)
  if [[ "${count}" != "0" ]]; then
    err "the private server lists hardware or cannot be read: count '${count}'"
    return 3
  fi
}

stop_process() {
  local pid=$1 tries=0
  kill "${pid}" 2>/dev/null || true
  while kill -0 "${pid}" 2>/dev/null; do
    tries=$((tries + 1))
    if ((tries > SERVER_STOP_TRIES)); then
      kill -KILL "${pid}" 2>/dev/null || true
      break
    fi
    sleep 0.1
  done
  wait "${pid}" 2>/dev/null || true
}

# The command leads its own group: a guard inside gets its time to remove what it added
# while the server still answers.
stop_command() {
  local tries=0
  kill -TERM -- "-${command_pid}" 2>/dev/null || true
  while kill -0 -- "-${command_pid}" 2>/dev/null; do
    tries=$((tries + 1))
    if ((tries > COMMAND_STOP_TRIES)); then
      kill -KILL -- "-${command_pid}" 2>/dev/null || true
      break
    fi
    sleep 0.1
  done
  wait "${command_pid}" 2>/dev/null || true
}

# Every live process whose environment names this run's folder. This script's own never does,
# as it hands those names only to what it starts.
left_behind() {
  grep -lsaF -- "=${root}/" /proc/[0-9]*/environ | sed -E 's|/proc/([0-9]+)/environ|\1|' || true
}

# What a command started in a session of its own would outlive its group by.
sweep_left_behind() {
  local pids
  pids=$(left_behind)
  [[ -n "${pids}" ]] || return 0
  # shellcheck disable=SC2086 # one word per process
  kill ${pids} 2>/dev/null || true
  sleep 0.5
  pids=$(left_behind)
  [[ -n "${pids}" ]] || return 0
  # shellcheck disable=SC2086 # one word per process
  kill -KILL ${pids} 2>/dev/null || true
}

cleanup() {
  local code=$?
  # A second signal must not stop this half done: nothing it stops ignores signals because
  # of this, as all of it started before.
  trap '' TERM INT
  trap - EXIT
  local index
  [[ -z "${command_pid}" ]] || stop_command
  for ((index = ${#server_pids[@]} - 1; index >= 0; index--)); do
    stop_process "${server_pids[index]}"
  done
  if [[ -n "${root}" ]]; then
    sweep_left_behind
    rm -rf -- "${root}"
  fi
  exit "${code}"
}

main() {
  if (($# < 1)); then
    err "usage: private_sound.sh <command> [arguments]"
    return 2
  fi
  if [[ ! -f "${STOCK_SESSION}/main.lua.d/20-default-access.lua" ]]; then
    err "no wireplumber 0.4 settings at ${STOCK_SESSION}: refusing"
    return 3
  fi
  root=$(mktemp -d "${TMPDIR:-/tmp}/psnd.XXXXXX")
  local socket="${root}/run/pulse/native"
  if ((${#socket} > LONGEST_SOCKET)); then
    err "the folder ${root} is too long for a socket's path"
    return 3
  fi
  mkdir -p "${root}/run/pulse" "${root}/config" "${root}/state" "${root}/data" "${root}/pipewire"
  write_server_settings
  write_session_settings
  inside=(env -u DBUS_SESSION_BUS_ADDRESS -u PIPEWIRE_REMOTE -u PULSE_SINK -u PULSE_SOURCE
    -u DISPLAY -u WAYLAND_DISPLAY -u WAYLAND_SOCKET
    PRIVATE_SOUND=1 "XDG_RUNTIME_DIR=${root}/run" "PIPEWIRE_RUNTIME_DIR=${root}/run"
    "PULSE_RUNTIME_PATH=${root}/run/pulse" "PULSE_SERVER=unix:${root}/run/pulse/native"
    "XDG_CONFIG_HOME=${root}/config" "XDG_STATE_HOME=${root}/state" "XDG_DATA_HOME=${root}/data"
    "PIPEWIRE_CONFIG_DIR=${root}/pipewire" "WIREPLUMBER_CONFIG_DIR=${root}/wireplumber")
  start_servers
  require_no_hardware
  local code=0
  # A background command's input is empty unless it is named, so name it.
  setsid "${inside[@]}" "$@" <&0 &
  command_pid=$!
  wait "${command_pid}" || code=$?
  command_pid=""
  require_no_hardware
  return "${code}"
}

trap cleanup EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
main "$@"
