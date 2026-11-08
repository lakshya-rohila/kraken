# Kraken shell integration for bash.
#   OSC 133;A        prompt is about to be drawn
#   OSC 133;C        a command started
#   OSC 133;D;<n>    the command finished with exit status n
#   OSC 7            current directory
#
# Kraken starts bash as `bash --rcfile <this file>`, so bash reads this
# instead of ~/.bashrc. We first load the user's own startup files exactly as
# bash would have (login or not), then add our hooks. Works on bash 3.2
# (/bin/bash on macOS) and later.

# --- 1. The user's startup files ---------------------------------------------
if [ -n "${KRAKEN_BASH_INJECT+x}" ]; then
  if [ "$KRAKEN_BASH_INJECT" = login ]; then
    builtin unset KRAKEN_BASH_INJECT
    # What a login shell reads (--rcfile is ignored by login shells, so
    # Kraken starts a normal interactive shell and we do this ourselves).
    [ -r /etc/profile ] && builtin . /etc/profile
    if [ -r "$HOME/.bash_profile" ]; then
      builtin . "$HOME/.bash_profile"
    elif [ -r "$HOME/.bash_login" ]; then
      builtin . "$HOME/.bash_login"
    elif [ -r "$HOME/.profile" ]; then
      builtin . "$HOME/.profile"
    fi
  else
    builtin unset KRAKEN_BASH_INJECT
    [ -r "$HOME/.bashrc" ] && builtin . "$HOME/.bashrc"
  fi
fi

# --- 2. Hooks -------------------------------------------------------------------
# Interactive shells only, and only once (this file may be sourced again).
if [ -z "${_KRAKEN_INTEGRATED-}" ] && [[ $- == *i* ]]; then
  _KRAKEN_INTEGRATED=1
  _kraken_running=0   # a command line is running
  _kraken_at_prompt=0 # the prompt is up, waiting for input
  _kraken_status=0

  _kraken_precmd() {
    _kraken_status=$?
    _kraken_at_prompt=0
    if [ "$_kraken_running" = 1 ]; then
      builtin printf '\033]133;D;%s\007' "$_kraken_status"
      _kraken_running=0
    fi
    builtin printf '\033]7;file://%s%s\007' "${HOSTNAME-}" "${PWD//\%/%25}"
    builtin printf '\033]133;A\007'
    builtin return "$_kraken_status"
  }

  # Last in PROMPT_COMMAND: from here on, the next command is the user's.
  _kraken_prompt_ready() {
    _kraken_at_prompt=1
    builtin return "$_kraken_status"
  }

  _kraken_preexec() {
    if [ "$_kraken_at_prompt" = 1 ]; then
      # Pressing Enter on an empty line runs PROMPT_COMMAND straight away.
      case "$BASH_COMMAND" in _kraken_precmd*) builtin return 0 ;; esac
      _kraken_at_prompt=0
      _kraken_running=1
      builtin printf '\033]133;C\007'
    fi
    builtin return 0
  }

  _kraken_setret() { builtin return "$1"; }

  if [ -n "${bash_preexec_imported-}${__bp_imported-}" ]; then
    # bash-preexec owns the DEBUG trap and PROMPT_COMMAND; use its hooks
    # (it gives every precmd function the command's real $?).
    precmd_functions=(_kraken_precmd "${precmd_functions[@]}" _kraken_prompt_ready)
    preexec_functions=(_kraken_preexec "${preexec_functions[@]}")
  else
    # PROMPT_COMMAND: we go first (to see the real $?) and last, keeping
    # whatever the user has in between. Bash 5.1+ allows it to be an array.
    _kraken_last='builtin eval "${_kraken_trap_setup-}"; _kraken_prompt_ready'
    if [[ "$(builtin declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
      PROMPT_COMMAND=(_kraken_precmd "${PROMPT_COMMAND[@]}" "$_kraken_last")
    elif [ -n "${PROMPT_COMMAND-}" ]; then
      PROMPT_COMMAND="_kraken_precmd
${PROMPT_COMMAND}
${_kraken_last}"
    else
      PROMPT_COMMAND="_kraken_precmd
${_kraken_last}"
    fi
    builtin unset _kraken_last

    # DEBUG trap for "command started", chained after any existing one. Bash
    # puts the DEBUG trap back when a sourced file or function returns, so
    # this runs at the first prompt, at top level, from PROMPT_COMMAND.
    builtin read -r -d '' _kraken_trap_setup <<'KRAKEN_EOF'
_kraken_prev_debug=$(builtin trap -p DEBUG)
if [ -n "$_kraken_prev_debug" ]; then
  _kraken_prev_debug=${_kraken_prev_debug#trap -- }
  _kraken_prev_debug=${_kraken_prev_debug% DEBUG}
  builtin eval "_kraken_prev_debug=$_kraken_prev_debug"
  builtin trap '_kraken_dbg_s=$?; _kraken_preexec; _kraken_setret "$_kraken_dbg_s"; '"$_kraken_prev_debug" DEBUG
else
  builtin trap '_kraken_preexec' DEBUG
fi
builtin unset _kraken_prev_debug _kraken_trap_setup
KRAKEN_EOF
  fi
fi
# Start the first prompt with $? = 0 (not whatever our setup returned).
builtin true
