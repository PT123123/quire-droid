set shell := ["powershell.exe", "-NoProfile", "-Command"]

# list available recipes
default:
    @just --list

# release build (default FemtoVG renderer)
build:
    cargo build --release

# run in debug; pass cargo flags as one quoted string
# e.g. just run "--no-default-features --features skia"
run *args:
    cargo run {{ args }}

# local CI replacement (the GitHub workflow was removed on purpose):
# everything a push would run, before you commit.
# `--workspace` survived the two-crate week and is now decoration twice over:
# this manifest declares no workspace at all (cargo-apk 0.10 refuses to read one,
# ADR-0095), so the flag names exactly this package — and `quire-core` is a
# *dependency*, which no cargo flag run from here will test. Its 431 tests belong
# to that repository (`cargo test --all-targets` there). So a green `just check`
# here means the shell compiles against the pinned rev and its own suite passes —
# never quote it as "the whole app is green".
check:
    cargo check --workspace --all-targets
    cargo test --workspace
    cargo build --workspace --release

# Android (M9.pre). Same no-CI decision, so these are the local entry; they need
# NDK 30 and there is still no device behind any of them.
android-check:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\android-build.ps1 -Task check

# the claim `check` cannot make: a cdylib that links (cargo check never links)
android-lib:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\android-build.ps1 -Task lib

# one APK, both ABIs inside, signed with the throwaway keystore the script
# generates into .scratch/ (Cargo.toml's [package.metadata.android.signing] points
# cargo-apk at it — the debug key cargo-apk embeds is refused by --release)
android-apk:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\android-build.ps1 -Task apk

# push the APK android-apk left in target\release\apk onto the attached device
# or emulator, replacing any install already there (`adb install -r`, which keeps
# app data). It builds nothing — run `android-apk` first. The file name is [lib]
# name's (`quire_shell`), and adb is expected on PATH (the SDK's platform-tools).
# Both guards fail before adb does, so an empty `adb devices` reads as a missing
# device rather than as a package-manager error.
install:
    $apk = "target\release\apk\quire_shell.apk"; if (-not (Test-Path $apk)) { throw "no APK at $apk; run 'just android-apk' first" }; if (-not (adb devices | Select-String "device$")) { throw "no adb device; connect a phone or start an emulator, then check 'adb devices'" }; adb install -r $apk; exit $LASTEXITCODE

# headless visual shot: software-rendered PNG of the real UI, no window.
# Scene names: default dark palette search-notes menu rename settings dialog empty
shot scene="default":
    cargo build --features software --bin quire-shot
    .\target\debug\quire-shot.exe --out .scratch\shots\latest.bmp --scene {{ scene }}
    powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks\scripts\shot2png.ps1

# package the release exe into a zip (M8-lite; installer comes later)
dist:
    cargo build --release
    powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks\scripts\dist.ps1

# installer end-to-end regression (D8/A6): build iss, silent install,
# verify, silent uninstall, residue check (needs Inno Setup)
# --portable end-to-end regression (A1 follow-up): 24 checks over the
# portable layout, log following, migration suppression, --db precedence
verify-portable:
    powershell -NoProfile -ExecutionPolicy Bypass -File install\verify-portable.ps1

verify-install:
    powershell -NoProfile -ExecutionPolicy Bypass -File install\verify-installer.ps1

# remove build artifacts: ./target + skia/wgpu benchmark target dirs
clean:
    cargo clean
    Remove-Item -Recurse -Force target-skia, target-wgpu -ErrorAction SilentlyContinue
