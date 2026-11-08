# Kraken shell integration for fish.
#   OSC 133;A        prompt is about to be drawn
#   OSC 133;C        a command started
#   OSC 133;D;<n>    the command finished with exit status n
#   OSC 7            current directory
#
# Kraken prepends its own data dir to XDG_DATA_DIRS, so fish loads this file
# from <dir>/fish/vendor_conf.d along with the user's normal config. Put
# XDG_DATA_DIRS back first, so programs started from fish don't see our dir.
if set -q KRAKEN_SHELL_DATA_DIR
    if set -q KRAKEN_ORIG_XDG_DATA_DIRS
        set -gx XDG_DATA_DIRS $KRAKEN_ORIG_XDG_DATA_DIRS
        set -e KRAKEN_ORIG_XDG_DATA_DIRS
    else
        set -e XDG_DATA_DIRS
    end
    set -e KRAKEN_SHELL_DATA_DIR
end

status is-interactive; or return 0
set -q _kraken_integrated; and return 0
set -g _kraken_integrated 1

# fish 4 marks prompts itself too; an extra mark is harmless.
function _kraken_prompt --on-event fish_prompt
    printf '\e]7;file://%s%s\a' $hostname (string replace -a '%' '%25' -- $PWD)
    printf '\e]133;A\a'
end

function _kraken_preexec --on-event fish_preexec
    printf '\e]133;C\a'
end

function _kraken_postexec --on-event fish_postexec
    printf '\e]133;D;%s\a' $status
end
