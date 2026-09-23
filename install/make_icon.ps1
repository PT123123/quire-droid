# Quire app icon — rasterised from the artwork in this folder, not drawn here.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File install\make_icon.ps1
#
# install\icon.svg is the artwork's source and install\icon_master_1024.png is
# its 1024 px raster: a document glyph on a **transparent** background — only
# the glyph's own pixels are opaque, so no frame ships a plate behind it. Every
# size below is that one picture scaled, which is what keeps them agreeing.
#
# Writes:
#   install\quire.ico      the exe/Start-menu icon: 16, 24, 32, 48, 64, 128 and
#                          256 px frames in one container
#   install\quire.png      the 256 px frame (nothing in the shell reads it —
#                          Slint 1.18 has no Window::set_icon — but the
#                          installer check and the docs name it)
#
# and, in this repo only, the Android launcher icon:
#   android\res\mipmap-*\ic_launcher.png
#                          the launcher icon, per density (48/72/96/144/192)
#   android\res\drawable-*\ic_launcher_foreground.png
#                          the adaptive icon's foreground: the same glyph inside
#                          the 108 dp canvas' central 72 dp safe zone, on
#                          transparent (a launcher masks the outer ring away, so
#                          a glyph drawn to the full canvas would lose its edges)
#   android\res\mipmap-anydpi-v26\ic_launcher.xml
#                          the adaptive icon itself (API 26+)
#   android\res\values\ic_launcher_background.xml
#                          its background — transparent, matching the art: the
#                          mark is the whole picture, so no plate is painted
#
# The Android half exists because cargo-apk ships no icon unless `resources`
# names a `res/` tree: the generated AndroidManifest had no `android:icon` at
# all, so a launcher showed the generic Android placeholder (M9 FEEDBACK:
# "应用图标怎么没了"). Cargo.toml points `metadata.android.resources` here and
# `metadata.android.application.icon` at `@mipmap/ic_launcher`.

Add-Type -AssemblyName System.Drawing

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent $here

$masterPath = Join-Path $here 'icon_master_1024.png'
if (-not (Test-Path $masterPath)) { throw "no artwork at $masterPath" }
$master = [System.Drawing.Bitmap]::FromFile($masterPath)

# Where the glyph actually is inside the master. The art carries its own margin,
# so the adaptive foreground — which has to fit a *safe zone* rather than the
# whole canvas — is sized from these bounds instead of from the file's edges.
function Get-ContentBox($bmp) {
    $rect = New-Object System.Drawing.Rectangle 0, 0, $bmp.Width, $bmp.Height
    $data = $bmp.LockBits($rect,
        [System.Drawing.Imaging.ImageLockMode]::ReadOnly,
        [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $stride = $data.Stride
        $bytes = New-Object byte[] ($stride * $bmp.Height)
        [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
    } finally {
        $bmp.UnlockBits($data)
    }
    $minX = $bmp.Width; $minY = $bmp.Height; $maxX = -1; $maxY = -1
    for ($y = 0; $y -lt $bmp.Height; $y++) {
        $row = $y * $stride
        for ($x = 0; $x -lt $bmp.Width; $x++) {
            if ($bytes[$row + $x * 4 + 3] -gt 8) {
                if ($x -lt $minX) { $minX = $x }
                if ($x -gt $maxX) { $maxX = $x }
                if ($y -lt $minY) { $minY = $y }
                if ($y -gt $maxY) { $maxY = $y }
            }
        }
    }
    if ($maxX -lt 0) { throw "the master is fully transparent" }
    New-Object System.Drawing.Rectangle $minX, $minY, ($maxX - $minX + 1), ($maxY - $minY + 1)
}

$box = Get-ContentBox $master

function New-Canvas([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceOver
    $g.Clear([System.Drawing.Color]::Transparent)
    ,@($bmp, $g)
}

function Save-Png($bmp) {
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bytes = $ms.ToArray()
    $ms.Dispose(); $bmp.Dispose()
    ,$bytes
}

# The legacy frame: the picture, edge to edge, on a transparent square. Windows
# draws these itself (the shell paints its own tile behind an icon), so nothing
# is added here but the art.
function New-FramePng([int]$size) {
    $c = New-Canvas $size
    $bmp = $c[0]; $g = $c[1]
    $g.DrawImage($master, (New-Object System.Drawing.Rectangle 0, 0, $size, $size))
    $g.Dispose()
    Save-Png $bmp
}

# The adaptive icon's foreground is a 108 dp canvas whose outer ring a launcher
# may mask away, so the glyph is fitted to the central 72 dp (2/3) safe zone
# instead of to the canvas.
function New-ForegroundPng([int]$size) {
    $c = New-Canvas $size
    $bmp = $c[0]; $g = $c[1]
    $scale = ($size * 0.66) / [Math]::Max($box.Width, $box.Height)
    $w = [int][Math]::Round($box.Width * $scale)
    $h = [int][Math]::Round($box.Height * $scale)
    $dest = New-Object System.Drawing.Rectangle ([int](($size - $w) / 2)), ([int](($size - $h) / 2)), $w, $h
    $g.DrawImage($master, $dest, $box, [System.Drawing.GraphicsUnit]::Pixel)
    $g.Dispose()
    Save-Png $bmp
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$frames = @{}
foreach ($size in $sizes) {
    $frames[$size] = New-FramePng $size
    "  {0,3}px  {1,6:n0} KB png" -f $size, ($frames[$size].Length / 1KB)
}

# .ico: a 6-byte header, one 16-byte directory entry per frame, then the PNG
# payloads back to back. Windows has read PNG-compressed entries since Vista,
# and the target here is the Windows 10/11 shell.
$out = Join-Path $here 'quire.ico'
$fs = New-Object System.IO.FileStream $out, ([System.IO.FileMode]::Create)
$w = New-Object System.IO.BinaryWriter $fs
$w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
foreach ($size in $sizes) {
    $bytes = $frames[$size]
    $w.Write([Byte]($(if ($size -ge 256) { 0 } else { $size })))  # width, 0 = 256
    $w.Write([Byte]($(if ($size -ge 256) { 0 } else { $size })))
    $w.Write([Byte]0); $w.Write([Byte]0)                          # palette, reserved
    $w.Write([UInt16]1)                                           # planes
    $w.Write([UInt16]32)                                         # bit depth
    $w.Write([UInt32]$bytes.Length)
    $w.Write([UInt32]$offset)
    $offset += $bytes.Length
}
foreach ($size in $sizes) { $w.Write($frames[$size]) }
$w.Flush(); $w.Close(); $fs.Close()

$png = Join-Path $here 'quire.png'
[System.IO.File]::WriteAllBytes($png, $frames[256])

"wrote {0} ({1:n0} bytes) and {2} ({3:n0} bytes)" -f `
    $out, (Get-Item $out).Length, $png, (Get-Item $png).Length

# self-check: the shell must be able to read the container back
$probe = New-Object System.Drawing.Icon $out, 32, 32
"icon loads: {0}x{1}" -f $probe.Width, $probe.Height
$probe.Dispose()

# ── Android ─────────────────────────────────────────────────────────────────
# One density bucket per launcher icon size, the same five Android ships.
$res = Join-Path $root 'android\res'
$densities = [ordered]@{
    'mdpi'    = 48
    'hdpi'    = 72
    'xhdpi'   = 96
    'xxhdpi'  = 144
    'xxxhdpi' = 192
}

foreach ($d in $densities.Keys) {
    $dir = Join-Path $res "mipmap-$d"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    [System.IO.File]::WriteAllBytes((Join-Path $dir 'ic_launcher.png'), (New-FramePng $densities[$d]))

    # the adaptive foreground is a 108 dp canvas; the bucket sizes above are the
    # 48 dp legacy frame, so this one is 108/48 of it
    $fg = Join-Path $res "drawable-$d"
    New-Item -ItemType Directory -Force -Path $fg | Out-Null
    $fgSize = [int]($densities[$d] * 108 / 48)
    [System.IO.File]::WriteAllBytes((Join-Path $fg 'ic_launcher_foreground.png'), (New-ForegroundPng $fgSize))
    "  android {0,-8} launcher {1,3}px  foreground {2,3}px" -f $d, $densities[$d], $fgSize
}

$anydpi = Join-Path $res 'mipmap-anydpi-v26'
New-Item -ItemType Directory -Force -Path $anydpi | Out-Null
$adaptive = @'
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@color/ic_launcher_background"/>
    <foreground android:drawable="@drawable/ic_launcher_foreground"/>
</adaptive-icon>
'@
[System.IO.File]::WriteAllText((Join-Path $anydpi 'ic_launcher.xml'), $adaptive)

$values = Join-Path $res 'values'
New-Item -ItemType Directory -Force -Path $values | Out-Null
$colors = @'
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <color name="ic_launcher_background">#00000000</color>
</resources>
'@
[System.IO.File]::WriteAllText((Join-Path $values 'ic_launcher_background.xml'), $colors)

"wrote android\res (5 densities, adaptive icon, transparent background)"
$master.Dispose()
