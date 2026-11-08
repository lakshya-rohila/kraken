# Kraken shell integration for zsh.
#   OSC 133;A        prompt is about to be drawn
#   OSC 133;C        a command started
#   OSC 133;D;<n>    the command finished with exit status n
#   OSC 7            current directory
[[ -o interactive ]] || return 0
[[ -n "$_KRAKEN_INTEGRATED" ]] && return 0
typeset -g _KRAKEN_INTEGRATED=1
typeset -gi _kraken_running=0

_kraken_precmd() {
  local ret=$?
  if (( _kraken_running )); then
    builtin printf '\e]133;D;%s\a' "$ret"
    _kraken_running=0
  fi
  builtin printf '\e]7;file://%s%s\a' "${HOST}" "${PWD//\%/%25}"
  builtin printf '\e]133;A\a'
  # Stay first in line, so prompt themes added later can't change $? before we read it.
  precmd_functions=(_kraken_precmd ${precmd_functions:#_kraken_precmd})
}

_kraken_preexec() {
  _kraken_running=1
  builtin printf '\e]133;C\a'
}

autoload -Uz add-zsh-hook
add-zsh-hook precmd _kraken_precmd
add-zsh-hook preexec _kraken_preexec
