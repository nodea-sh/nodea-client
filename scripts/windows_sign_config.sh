#!/usr/bin/env bash
# Decide how the Windows release job treats Authenticode signing.
#
#   windows_sign_config.sh mode
#
# Prints one word on stdout:
#   report   no signing is configured; the verify gate lists what is unsigned
#            and warns, and the unsigned MSI still ships (the pre-signing
#            behaviour)
#   enforce  signing is configured; any unsigned or untimestamped file fails
#            the release
#
# Fails instead of printing when the release must not proceed:
#   - WINDOWS_SIGNING=required but signing is not configured. This is the
#     switch that stops a lost secret from silently shipping an unsigned build.
#   - Signing is half configured. Some names set and others not is always a
#     setup mistake, and guessing which half was meant would either sign with
#     a wrong account or quietly fall back to unsigned.
#   - WINDOWS_SIGNING holds anything other than empty or "required", so a
#     typo such as "require" cannot read as "off".
#
# Shell rather than Python for the same reason as select_bundle.sh: it runs in
# release jobs that install no interpreter, and on the Windows runner's bash.
set -euo pipefail

if [ "$#" -ne 1 ] || [ "$1" != "mode" ]; then
  echo "usage: $(basename "$0") mode" >&2
  exit 2
fi

required_names=(
  AZURE_TENANT_ID
  AZURE_CLIENT_ID
  ARTIFACT_SIGNING_ENDPOINT
  ARTIFACT_SIGNING_ACCOUNT
  ARTIFACT_SIGNING_PROFILE
)

switch=${WINDOWS_SIGNING:-}
case "$switch" in
  "" | required) ;;
  *)
    echo "WINDOWS_SIGNING must be empty or 'required', got '$switch'" >&2
    exit 1
    ;;
esac

set_names=()
missing_names=()
for name in "${required_names[@]}"; do
  if [ -n "${!name:-}" ]; then
    set_names+=("$name")
  else
    missing_names+=("$name")
  fi
done

if [ "${#set_names[@]}" -eq 0 ]; then
  if [ "$switch" = "required" ]; then
    {
      echo "WINDOWS_SIGNING=required but Windows signing is not configured."
      echo "Set these in the release environment:"
      printf '  %s\n' "${required_names[@]}"
    } >&2
    exit 1
  fi
  echo report
  exit 0
fi

if [ "${#missing_names[@]}" -gt 0 ]; then
  {
    echo "Windows signing is half configured. Missing:"
    printf '  %s\n' "${missing_names[@]}"
    echo "Set all of them, or none to build unsigned."
  } >&2
  exit 1
fi

# The dlib sends signing requests to this URL with our Azure token, so it must
# be one of Microsoft's regional endpoints and nothing else.
endpoint_pattern='^https://[a-z0-9]+\.codesigning\.azure\.net/?$'
if ! [[ "$ARTIFACT_SIGNING_ENDPOINT" =~ $endpoint_pattern ]]; then
  echo "ARTIFACT_SIGNING_ENDPOINT must look like https://<region>.codesigning.azure.net, got '$ARTIFACT_SIGNING_ENDPOINT'" >&2
  exit 1
fi

echo enforce
