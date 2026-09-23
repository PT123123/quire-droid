# Quire app icon — drawn here, not hand-made, so the asset is reproducible.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File install\make_icon.ps1
#
# Writes, all from the one picture below (a gradient field and a `Q` drawn as a
# ring plus a tail stroke, so nothing depends on an installed font):
#
#   install\quire.ico          the exe/Start-menu icon: 16, 24, 32, 48, 64, 128,
#                              256 px frames in one container
#   install\quire.png          the 256 px frame, for Slint's `Window.icon`
#   android\res\mipmap-*\ic_launcher.png
#                              the launcher icon, per density (48/72/96/144/192)
#   android\res\drawable-*\ic_launcher_foreground.png
#                              the adaptive icon's foreground: the Q alone on
#                              transparent, drawn inside the 108 dp canvas'
#                              72 dp safe zone (162/216/324/432/108 px)
#   android\res\mipmap-anydpi-v26\ic_launcher.xml
#                              the adaptive icon itself (API 26+)
#   android\res\values\ic_launcher_background.xml
#                              its background, the same navy the gradient starts
#                              from
#
# The Android half exists because cargo-apk ships no icon unless `resources`
# names a `res/` tree: the generated AndroidManifest had no `android:icon` at
# all, so a launcher showed the generic Android placeholder (M9 FEEDBACK:
# "应用图标怎么没了"). Cargo.toml points `metadata.android.resources` here and
# `metadata.android.application.icon` at `@mipmap/ic_launcher`.

Add-Type -AssemblyName System.Drawing

$top = [System.Drawing.Color]::FromArgb(255, 14, 27, 46)      # #0E1B2E
$bottom = [System.Drawing.Color]::FromArgb(255, 10, 36, 66)   # #0A2442
$ink = [System.Drawing.Color]::FromArgb(255, 245, 247, 250)   # near white
$accent = [System.Drawing.Color]::FromArgb(255, 94, 234, 212) # #5EEAD4

# The Q mark, inside a reference square of `$size` placed at ($ox, $oy). Every
# length is a fraction of that square, which is what keeps the ring readable at
# 16 px and keeps the adaptive foreground and the legacy icon the same drawing.
function Draw-QuireMark($g, [double]$ox, [double]$oy, [double]$size) {
    $stroke = [Math]::Max(1.5, $size * 0.10)
    $ring = $size * 0.30
    $cx = $ox + $size * 0.5
    $cy = $oy + $size * 0.46

    $pen = New-Object System.Drawing.Pen($ink, $stroke)
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $g.DrawEllipse($pen, ($cx - $ring), ($cy - $ring), ($ring * 2), ($ring * 2))

    $tail = New-Object System.Drawing.Pen($accent, $stroke)
    $tail.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $tail.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $edge = [Math]::Sqrt(2) / 2
    $g.DrawLine($tail,
        ($cx + $ring * $edge * 0.7), ($cy + $ring * $edge * 1.1),
        ($cx + $ring * $edge * 2.0), ($cy + $ring * $edge * 2.2))
    $pen.Dispose(); $tail.Dispose()
}

function Save-Png($bmp) {
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bytes = $ms.ToArray()
    $ms.Dispose(); $bmp.Dispose()
    ,$bytes
}

# The rounded-square app icon: 22% corner radius, the same shape at every size.
function New-FramePng([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    $radius = $size * 0.22
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $radius * 2
    $path.AddArc(0, 0, $d, $d, 180, 90)
    $path.AddArc($size - $d, 0, $d, $d, 270, 90)
    $path.AddArc($size - $d, $size - $d, $d, $d, 0, 90)
    $path.AddArc(0, $size - $d, $d, $d, 90, 90)
    $path.CloseFigure()

    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
        (New-Object System.Drawing.RectangleF(0, 0, $size, $size)),
        $top, $bottom, 90)
    $g.FillPath($brush, $path)
    $brush.Dispose(); $path.Dispose()

    Draw-QuireMark $g 0 0 $size

    $g.Dispose()
    Save-Png $bmp
}

# The adaptive icon's foreground: transparent, and the mark sized to the inner
# two-thirds of the canvas. The launcher masks the outer third away, so a mark
# drawn to the full canvas would lose its ring to the mask.
function New-ForegroundPng([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    $safe = $size / 3.0
    Draw-QuireMark $g $safe $safe $safe

    $g.Dispose()
    Save-Png $bmp
}

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent $here

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
    <color name="ic_launcher_background">#0E1B2E</color>
</resources>
'@
[System.IO.File]::WriteAllText((Join-Path $values 'ic_launcher_background.xml'), $colors)

"wrote android\res (5 densities, adaptive icon, background)"
