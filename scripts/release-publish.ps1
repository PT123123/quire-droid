# Publish a release: bump the patch, build the APK, put it on GitHub.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release-publish.ps1
#
# Obtainium installs this app by watching this repository's GitHub releases, so
# the publish *is* the delivery. The steps are ordered so nothing ships without
# its bookkeeping:
#
#   1. [package] version's patch +1 in Cargo.toml. versionName and versionCode
#      are both derived from it by cargo-apk, so this is what makes Android —
#      and Obtainium — see the update at all.
#   2. `cargo apk build --release` (android-build.ps1 -Task apk), which also
#      refreshes the version Cargo.lock records.
#   3. The bump is committed (Cargo.toml + Cargo.lock, nothing else) and pushed,
#      so the tag created next points at a commit carrying the version it names.
#   4. `gh release create v<version>` attaches quire-<version>.apk.
#
# The signed artifact is quire_shell.apk, cargo-apk's own name; the published
# asset is the copy whose file name carries the version. Re-running after a
# failed publish finds the tag already there and re-uploads over it rather than
# erroring out.

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$root = (Get-Location).Path

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    throw "gh is not on PATH — install the GitHub CLI and 'gh auth login' first"
}

# ── 1: the bump ─────────────────────────────────────────────────────────────
# The first top-level `version` key in the manifest is [package]'s: dependency
# tables sit below it and indent their keys. The [dependencies] check turns that
# convention into a guard, so a reshuffled manifest fails loudly instead of
# quietly bumping some library.
$manifest = Join-Path $root 'Cargo.toml'
$text = [System.IO.File]::ReadAllText($manifest)
$m = [regex]::Match($text, '(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"')
if (-not $m.Success) { throw "no [package] version line in Cargo.toml" }
$deps = $text.IndexOf('[dependencies]')
if ($deps -ge 0 -and $m.Index -gt $deps) {
    throw "the first top-level 'version' key sits below [dependencies] — refusing to bump the wrong table"
}
$version = "{0}.{1}.{2}" -f $m.Groups[1].Value, $m.Groups[2].Value, ([int]$m.Groups[3].Value + 1)
# splice exactly the three digits — group 1 starts at the major, group 3 ends at
# the patch — so every other byte of the manifest is untouched
$vStart = $m.Groups[1].Index
$vEnd = $m.Groups[3].Index + $m.Groups[3].Length
$text = $text.Substring(0, $vStart) + $version + $text.Substring($vEnd)
$utf8 = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText($manifest, $text, $utf8)
# read it back before anything expensive happens on top of a bad manifest
$check = [regex]::Match([System.IO.File]::ReadAllText($manifest), '(?m)^version\s*=\s*"([^"]+)"')
if (-not $check.Success -or $check.Groups[1].Value -ne $version) {
    throw ("the bump wrote '{0}', expected '{1}' — aborting before the build" -f `
        $check.Groups[1].Value, $version)
}
Write-Output "==> version -> $version"

# ── 2: the build ────────────────────────────────────────────────────────────
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'android-build.ps1') -Task apk
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$apk = Join-Path $root 'target\release\apk\quire_shell.apk'
if (-not (Test-Path $apk)) { throw "no APK at $apk — android-build.ps1 reported success but left nothing" }
$named = Join-Path $root ("target\release\apk\quire-{0}.apk" -f $version)
Copy-Item $apk $named -Force

# ── 3: the bump goes in as its own commit ───────────────────────────────────
git add Cargo.toml Cargo.lock
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
git commit -m "chore(release): $version"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
git push
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# ── 4: the release ──────────────────────────────────────────────────────────
$repo = (& gh repo view --json nameWithOwner | ConvertFrom-Json).nameWithOwner
$tag = "v$version"
$existing = & gh release list --json tagName --limit 200 | ConvertFrom-Json
if (@($existing | Where-Object { $_.tagName -eq $tag }).Count -gt 0) {
    Write-Output "==> release $tag already exists; replacing its APK"
    & gh release upload $tag $named --clobber
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
    $notes = @"
Quire $version for Android (arm64-v8a).

- package: dev.quire.android (minSdk 24, targetSdk 34)
- signed with the throwaway key scripts/android-build.ps1 generates (see the
  signing note in Cargo.toml), so an update installs cleanly only over a build
  carrying this same signature

Obtainium: add https://github.com/$repo and it picks up the APK attached to
every release.
"@
    & gh release create $tag $named --target master --title "Quire $version" --notes $notes
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
Write-Output "==> published $tag ($named)"
