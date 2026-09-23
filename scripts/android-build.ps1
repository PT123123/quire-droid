# M9.pre: the local entry for the Android shell. There is no CI for this on
# purpose, so this file is where the toolchain block lives (the desktop's own
# entry is the `just` recipes).
#
#   powershell -File scripts\android-build.ps1 -Task check          # aarch64, compile only
#   powershell -File scripts\android-build.ps1 -Task lib            # aarch64, link the cdylib
#   powershell -File scripts\android-build.ps1 -Task apk            # one APK, aarch64 inside
#
# `-Abi both` (or x86_64) still works — the toolchain block below covers either
# — but the default is arm64 since 2026-09-24: the target is a TB320FC, the
# x86_64 arm was for an emulator nobody runs, and a second ABI doubles the cold
# pass. `build_targets` in Cargo.toml is what cargo-apk packages, and it is
# arm64-only too, so `-Abi` decides what gets *compiled* rather than what ships.
#
# The one flag an Android build needs is the one that turns the desktop default
# off: `default` is FemtoVG, and Slint cfg's FemtoVG out of this platform.
param(
    [ValidateSet("x86_64", "aarch64", "both")] [string]$Abi = "aarch64",
    [ValidateSet("check", "lib", "apk")] [string]$Task = "check",
    [string]$Ndk = "30.0.15729638",
    [string]$Api = "24"
)

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$ndkRoot = Join-Path $env:LOCALAPPDATA "Android\Sdk\ndk\$Ndk"
if (-not (Test-Path $ndkRoot)) { throw "no NDK at $ndkRoot" }
$bin = Join-Path $ndkRoot "toolchains\llvm\prebuilt\windows-x86_64\bin"
$env:ANDROID_NDK_HOME = $ndkRoot

$abris = @()
if ($Abi -eq "both") { $abris = @("x86_64", "aarch64") } else { $abris = @($Abi) }

# The renderer that ships on this platform — Skia on GLES — is named in
# Cargo.toml's Android dependency section, so there is nothing to add here:
# measured 2026-09-23, a build carrying only `--no-default-features` links.
$features = @("--no-default-features")

# cargo-apk packages every `build_targets` entry from Cargo.toml in one go and
# reads the toolchain from the environment, so both triples are described even
# when one ABI was asked for.
foreach ($a in $abris) {
    $triple = "$a-linux-android"
    $envKey = $triple.ToUpper().Replace("-", "_")
    $clang = Join-Path $bin "$triple$Api-clang.cmd"
    $clangxx = Join-Path $bin "$triple$Api-clang++.cmd"
    Set-Item "env:CC_$envKey" $clang
    Set-Item "env:CXX_$envKey" $clangxx
    Set-Item "env:AR_$envKey" (Join-Path $bin "llvm-ar.exe")
    Set-Item "env:CARGO_TARGET_${envKey}_LINKER" $clang
    Write-Host "==> $triple ($(Split-Path -Leaf $clang))"
}

if ($Task -eq "apk") {
    # cargo-apk will not sign a release package with the debug key it ships, and
    # the manifest names the key it wants instead (Cargo.toml's signing note).
    # That key is a real, stable identity kept outside this repository, which is
    # what makes an update installable: Android refuses one whose signature
    # differs. So this checks it is there rather than making anything — a key
    # generated here would sign a *different app*, and the release would be one
    # nobody can update.
    $keystore = "C:\Users\ted\keystores\debug.keystore"
    if (-not (Test-Path $keystore)) {
        throw "no release keystore at $keystore — see the signing note in Cargo.toml"
    }
    # `--lib` is not decoration. cargo-apk 0.10 reads the manifest itself (not
    # cargo's build plan) and packages *every* artifact it finds — the lib and
    # each `[[bin]]` — and its artifact->filename table only knows how to name a
    # cdylib, so the first binary it reaches panics with "Bin is not compatible
    # with Cdylib" (cargo-subcommand 0.12 artifact.rs:51). That panic lands
    # *after* the library's APK has been aligned and signed, which is why the
    # check below reads the file rather than the exit code. Naming the one
    # target this platform can package says so up front: no panic, and no
    # `quire-typing` linked for a phone that will never run it.
    #
    # The artifact is the claim: `lib_name` in the generated manifest, the packaged
    # `.so` **and the APK's own file name** are all derived from the library
    # target's name (`quire_shell`, Cargo.toml's `[lib]`), so a fresh file here is
    # evidence the whole chain agreed on it. Measured 2026-09-23: renaming the
    # library moved the output from `quire.apk` to `quire_shell.apk`, and the run
    # that still looked for the old name reported failure over a package that had
    # been built, aligned and signed correctly two lines earlier.
    $apk = Join-Path "target\release\apk" "quire_shell.apk"
    $before = if (Test-Path $apk) { (Get-Item $apk).LastWriteTime } else { [datetime]::MinValue }
    cargo apk build --release --lib @features
    if (-not (Test-Path $apk)) {
        throw "no APK at $apk (cargo-apk exited $LASTEXITCODE; look for a signing or packaging error above)"
    }
    $info = Get-Item $apk
    if ($info.LastWriteTime -le $before) {
        throw "$apk is not from this run ($($info.LastWriteTime)) — cargo-apk bailed before writing it"
    }
    Write-Host (
        "==> {0} = {1} B ({2:N2} MiB), arm64-v8a inside, signed with {3}" -f
        $apk, $info.Length, ($info.Length / 1MB), $keystore
    )
    exit 0
} else {
    foreach ($a in $abris) {
        $target = "$a-linux-android"
        if ($Task -eq "check") {
            cargo check --target $target --all-targets @features
        } else {
            # `check` never links, and the claim under test here is that a
            # cdylib loads on Android — so this one has to be a real build.
            cargo build --release --lib --target $target @features
        }
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
}
exit $LASTEXITCODE
