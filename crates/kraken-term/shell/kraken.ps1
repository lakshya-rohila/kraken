# Kraken shell integration for PowerShell.
#   OSC 133;A        prompt is about to be drawn
#   OSC 133;C        a command started (needs PSReadLine)
#   OSC 133;D;<n>    the command finished with exit status n
#   OSC 7            current directory
#
# Kraken starts `pwsh -NoExit -Command ". '<this file>'"`: the user's $PROFILE
# loads first as usual, then this wraps whatever `prompt` it defined.
if ($global:__KrakenIntegrated) { return }
$global:__KrakenIntegrated = $true

$global:__KrakenOrigPrompt = $function:prompt
$global:__KrakenHostName = [System.Net.Dns]::GetHostName()
$global:__KrakenLastHistoryId = 0
$__krakenLast = Get-History -Count 1
if ($__krakenLast) { $global:__KrakenLastHistoryId = $__krakenLast.Id }
Remove-Variable __krakenLast

function global:prompt {
    # Read these first: anything we run changes them.
    $ok = $global:?
    $code = $global:LASTEXITCODE
    $esc = [char]27
    $bel = [char]7
    $out = ''

    # A command ran since the last prompt (not just an empty Enter).
    $last = Get-History -Count 1
    if ($last -and $last.Id -ne $global:__KrakenLastHistoryId) {
        $global:__KrakenLastHistoryId = $last.Id
        $status = if ($ok) { 0 } elseif ($code -is [int] -and $code -ne 0) { $code } else { 1 }
        $out += "$esc]133;D;$status$bel"
    }
    if ($PWD.Provider.Name -eq 'FileSystem') {
        $path = $PWD.ProviderPath -replace '%', '%25'
        $out += "$esc]7;file://$($global:__KrakenHostName)$path$bel"
    }
    $out += "$esc]133;A$bel"
    [Console]::Write($out)

    # Give the user's prompt the same $? and $LASTEXITCODE we saw.
    $global:LASTEXITCODE = $code
    if (-not $ok) { Write-Error 'kraken' -ErrorAction Ignore }
    if ($global:__KrakenOrigPrompt) { & $global:__KrakenOrigPrompt } else { "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) " }
}

# Command start: PSReadLine calls the history handler when a line is accepted,
# just before it runs. Keep any handler already set (including its default).
if (Get-Module PSReadLine) {
    $global:__KrakenPrevHistoryHandler = (Get-PSReadLineOption).AddToHistoryHandler
    Set-PSReadLineOption -AddToHistoryHandler {
        param([string]$line)
        [Console]::Write("$([char]27)]133;C$([char]7)")
        if ($global:__KrakenPrevHistoryHandler) { return $global:__KrakenPrevHistoryHandler.Invoke($line) }
        return $true
    }
}
