# shellcheck shell=bash
# Values install.sh keeps when a re-run does not pass the flag. Sourced by install.sh.
# A flag that was passed wins, including an empty one. A flag that was not passed keeps
# what .env already holds, so a re-run cannot turn a hosted hub back into a single-owner
# one or drop Sign in with Shippin by omission.

unquote() { sed "s/^'\(.*\)'\$/\1/; s/^\"\(.*\)\"\$/\1/"; }

# $1 the .env name, $2 the value this run would write, $3 true when the flag was passed,
# $4 the existing .env (may be missing).
keep_env() {
  local name="$1" given="$2" was_set="$3" file="$4" prior
  if [[ "$was_set" == true ]]; then
    printf '%s' "$given"
    return
  fi
  prior=""
  [[ -f "$file" ]] && prior=$(sed -n "s/^${name}=//p" "$file" | tail -1 | unquote)
  if [[ -n "$prior" ]]; then
    printf '%s' "$prior"
  else
    printf '%s' "$given"
  fi
}
