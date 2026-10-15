#!/usr/bin/env bash
set -euo pipefail

if [[ ${1:-} == --self-test ]]; then
  test "$(bash "$0" 0.1.0 1)" = v0.1.1
  test "$(bash "$0" 0.1.5 10)" = v0.1.15
  test "$(bash "$0" 1.2.0 10)" = v1.2.10
  if bash "$0" 0.1.0 0 >/dev/null 2>&1; then exit 1; fi
  if bash "$0" 0.1.0-rc.1 1 >/dev/null 2>&1; then exit 1; fi
  exit
fi

version=${1:-}
run_number=${2:-}
if [[ ! $version =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ || ! $run_number =~ ^[1-9][0-9]*$ ]]; then
  echo 'Expected stable Cargo version and positive workflow run number' >&2
  exit 1
fi
IFS=. read -r major minor patch <<< "$version"
printf 'v%s.%s.%d\n' "$major" "$minor" "$((patch + run_number))"
