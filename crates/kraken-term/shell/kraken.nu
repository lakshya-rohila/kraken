# Kraken shell integration for nushell.
#
# Kraken prepends its own data dir to XDG_DATA_DIRS, so nu loads this file
# from <dir>/nushell/vendor/autoload after the user's env.nu and config.nu.
# Put XDG_DATA_DIRS back first, so programs started from nu don't see our dir.
if 'KRAKEN_SHELL_DATA_DIR' in $env {
    if 'KRAKEN_ORIG_XDG_DATA_DIRS' in $env {
        $env.XDG_DATA_DIRS = $env.KRAKEN_ORIG_XDG_DATA_DIRS
        hide-env KRAKEN_ORIG_XDG_DATA_DIRS
    } else {
        hide-env XDG_DATA_DIRS
    }
    hide-env KRAKEN_SHELL_DATA_DIR
}

# nu emits the marks Kraken reads by itself; make sure they're on:
#   OSC 133;A / 133;C / 133;D;<exit>   prompt, command start, command end
#   OSC 7                               current directory
# Hooks would only duplicate them.
$env.config.shell_integration.osc133 = true
$env.config.shell_integration.osc7 = true
