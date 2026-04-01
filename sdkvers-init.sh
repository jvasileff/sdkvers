_sdkvers_init_dir="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"

sdkvers() {
  local sdkvers_resolver
  local sdkvers_output
  local sdkvers_exit

  if [ -n "${SDKVERS_HOME:-}" ] && [ -x "${SDKVERS_HOME}/sdkvers-resolve" ]; then
    sdkvers_resolver="${SDKVERS_HOME}/sdkvers-resolve"
  elif [ -x "${_sdkvers_init_dir}/sdkvers-resolve" ]; then
    sdkvers_resolver="${_sdkvers_init_dir}/sdkvers-resolve"
  else
    sdkvers_resolver="sdkvers-resolve"
  fi

  # Run the shell-function backend.  All output is structured three-section
  # format (eval / stdout / stderr) separated by a UUID sentinel line.
  # Parsing is delegated back to the binary via the extract subcommand.
  #
  # Pass --color when this shell function's stdout is a terminal.  The binary's
  # own stdout is always a pipe (due to $() capture), so it cannot detect the
  # tty itself; the shell function is the authoritative source of that information.
  if [ -t 1 ]; then
    sdkvers_output=$("$sdkvers_resolver" sdkvers --stdout-is-tty "$@")
  else
    sdkvers_output=$("$sdkvers_resolver" sdkvers "$@")
  fi
  sdkvers_exit=$?

  # Some subcommands (e.g. selfupdate) produce no stdout intentionally and
  # rely on this guard to skip the extract/eval steps, avoiding any
  # dependency on the internal protocol across binary versions.
  if [ $sdkvers_exit -eq 0 ] && [ -n "$sdkvers_output" ]; then
    eval "$(printf '%s' "$sdkvers_output" | "$sdkvers_resolver" internal extract eval)"
    printf '%s' "$sdkvers_output" | "$sdkvers_resolver" internal extract stdout
  fi

  return $sdkvers_exit
}
