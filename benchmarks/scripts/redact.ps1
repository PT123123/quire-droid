# Written into the committed benchmark rows, so a machine path here is a
# machine path on a public repository. The history pass of 2026-09-23 cleaned
# the rows already in; these two functions are what keeps new runs from putting
# them back. The placeholders are the same ones that pass wrote, so a row from
# before it and a row from after read alike.
#
# Dot-source it: `. (Join-Path $PSScriptRoot 'redact.ps1')`
#
# Redact a field BEFORE its backslashes are doubled for JSON - the rules below
# are built from real path strings, which carry one separator, not two.

# Resolved in this file's own scope, where $PSScriptRoot is unambiguous: this
# directory is <repo>\benchmarks\scripts.
# A caller that hands this function a forward-slash path (a bash-style launch
# argument, a URI-ish constant) still points at the same directory, so a rule
# that only matches backslashes lets the absolute prefix through and the
# username fallback below redacts it to <user> - a row that reads scrubbed and
# is not. Match either separator.
$prefixes = @(
    @{ Path = (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)); Placeholder = '<repo>' },
    @{ Path = $env:TEMP;         Placeholder = '<temp>' },
    @{ Path = $env:LOCALAPPDATA; Placeholder = '<localappdata>' },
    @{ Path = $env:USERPROFILE;  Placeholder = '<user>' }
) | Where-Object { $_.Path } |
  ForEach-Object { [pscustomobject]@{
      # Each segment escaped on its own, then rejoined with a class that
      # accepts either separator - going through -replace here would eat the
      # doubled backslash as an escape of its own.
      Pattern = (((($_.Path -split '[\\/]') | ForEach-Object { [regex]::Escape($_) }) -join '[/\\]'))
      Replacement = $_.Placeholder
      Weight = $_.Path.Length
  } }

# %TEMP% lives inside %LOCALAPPDATA%, which lives inside %USERPROFILE%, so the
# longest prefix has to be tried first or the outer one wins and the row reads
# <user>\AppData\Local\Temp\... for no benefit.
$redactRules = @($prefixes | Sort-Object -Property Weight -Descending)

# A home directory on another drive, or one this shell's variables do not
# describe, still must not publish the account name.
if ($env:USERNAME) {
    foreach ($sep in @('\', '/')) {
        $redactRules += [pscustomobject]@{
            Pattern = [regex]::Escape("Users$sep$env:USERNAME")
            Replacement = "Users$sep<user>"
            Weight = 0
        }
    }
}

function Redact-MachinePath {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Text)
    $out = $Text
    foreach ($rule in $redactRules) {
        $out = $out -replace $rule.Pattern, $rule.Replacement
    }
    # An account name left as a whole path segment is a prefix no rule covers.
    # The 2026-09-23 leak survived two scans because the quiet answer reads the
    # same as the true one, so this one says out loud instead.
    if ($env:USERNAME -and $out -match ("[\\/]" + [regex]::Escape($env:USERNAME) + "[\\/]")) {
        Write-Warning "a machine path survived redaction - add a prefix rule to benchmarks/scripts/redact.ps1"
    }
    $out
}

function Open-JsonPlaceholders {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Json)
    # PowerShell 5.1's ConvertTo-Json escapes '<' and '>' (and 7.x does not), and
    # three of the writers re-serialize bench.ps1's line through it. Unescaping
    # the two placeholders is a no-op for any JSON reader and keeps every
    # results file spelled the same. A doubled backslash means the text really
    # contained "\u003c", so those stay alone.
    $Json -replace '(?<!\\)\\u003[cC]', '<' -replace '(?<!\\)\\u003[eE]', '>'
}
