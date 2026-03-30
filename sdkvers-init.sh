sdkvers() {
  local sdkvers_resolver
  local sdkvers_output
  local sdkvers_exit

  if [ -n "${SDKVERS_HOME:-}" ] && [ -x "${SDKVERS_HOME}/sdkvers-resolve" ]; then
    sdkvers_resolver="${SDKVERS_HOME}/sdkvers-resolve"
  else
    sdkvers_resolver="sdkvers-resolve"
  fi

  # Capture output and exit code separately so that any successful sdk use
  # commands are evaluated even when some candidates fail to resolve.
  sdkvers_output=$("$sdkvers_resolver" resolve-project "$@")
  sdkvers_exit=$?
  eval "$sdkvers_output"
  return $sdkvers_exit
}
