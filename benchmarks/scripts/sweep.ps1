param(
    [string]$OutDir = ".scratch/sweep",
    [int]$W = 1280,
    [int]$H = 800,
    # comma-separated scene list; empty = every scene apply_scene understands
    [string]$Scenes = "",
    # a previous sweep directory: its manifest is diffed, so a re-judge only
    # touches scenes whose pixels actually moved
    [string]$Baseline = ""
)
$ErrorActionPreference = "SilentlyContinue"
Add-Type -AssemblyName System.Drawing

# A4 · full visual regression sweep (SPEC §二十一): render every declared
# scene through the headless software renderer into $OutDir/<scene>.png.
# Nothing here judges the pixels — the PNGs plus .scratch/sweep/report.md are
# the deliverable, reviewed by hand against the checklist.
#
# The manifest (one "<md5>  <scene>.png" line per shot, plus the commit) turns a
# second sweep into a *diff*: the first re-sweep caught two original findings
# that turned out to be 1:1 eyeball misreads of scenes whose pixels had never
# moved, so "identical bytes" is now the evidence that a verdict carries over.
$shot = "target\debug\quire-shot.exe"
if (-not (Test-Path $shot)) {
    Write-Output "build it first: cargo build --features software --bin quire-shot"
    exit 1
}

# light as declared, plus every dark-* the controller knows
$all = @(
    "default", "edit", "empty", "nest", "toggle", "toggle-fold", "rename", "title-edit", "recovered",
    "find", "find-grid", "find-cols", "find-callout", "marks", "marks-wrap", "block-colors", "page-block", "link-block", "image", "image-half", "file",
    "table", "table-edit", "table-marks",
    "math", "math-inline",
    "toc",
    "embed", "embed-empty",
    "code-hl",
    "mention", "date", "backlinks", "backlinks-open", "backlinks-small", "dangling",
    "synced", "synced-source-gone",
    "columns", "columns-3", "columns-marks",
    "slash", "plus", "block-menu", "move-to", "move-to-tall", "page-move-to", "page-style", "text-color",
    "bg-color", "link", "menu", "palette", "palette-nav", "search", "search-notes",
    "touch-menu",
    "dialog", "settings",
    "style-serif", "style-mono", "style-small", "style-full", "style-tight",
    "page-icon", "icon-picker",
    "page-cover", "page-cover-white", "page-cover-icon",
    "page-lock", "page-lock-menu", "page-lock-block-menu",
    "page-templates", "page-template-pick", "slash-template",
    "page-versions", "page-versions-diff",
    "dark", "dark-slash", "dark-find", "dark-marks", "dark-link", "dark-code-hl",
    "dark-block-menu", "dark-block-colors", "dark-title-edit", "dark-style-serif",
    "dark-page-icon", "dark-page-cover", "dark-page-cover-white", "dark-page-lock",
    "dark-page-templates", "dark-page-versions", "dark-mention", "dark-date",
    "dark-backlinks", "dark-dangling", "dark-synced", "dark-synced-source-gone",
    "dark-move-to-tall",
    # SPEC §三十九 (M14): the database's own scenes. They were run out of a
    # separate list for six slices, which is how two of them spent those slices
    # photographing the *same* table — a scene nobody sweeps is a scene nobody
    # measures. In the default list they now stay.
    "database-table", "database-filter", "database-formula", "database-search",
    "database-linked", "database-template",
    "database-board", "database-list", "database-calendar", "database-gallery",
    "database-timeline", "database-form", "database-chart",
    "database-kinds", "database-relation", "database-rollup",
    "dark-database-table", "dark-database-filter", "dark-database-formula",
    "dark-database-search", "dark-database-linked", "dark-database-template",
    "dark-database-board", "dark-database-list", "dark-database-calendar",
    "dark-database-gallery", "dark-database-timeline", "dark-database-form",
    "dark-database-chart", "dark-database-kinds", "dark-database-relation",
    "dark-database-rollup"
)
$targets = if ($Scenes -eq "") { $all } else { $Scenes -split ',' }

function Get-Md5([string]$path) {
    (Get-FileHash -Algorithm MD5 -Path $path).Hash
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$rows = @()
foreach ($s in $targets) {
    $bmp = Join-Path $OutDir "$s.bmp"
    $png = Join-Path $OutDir "$s.png"
    if (Test-Path $png) { Remove-Item $png }
    if (Test-Path $bmp) { Remove-Item $bmp }
    $sceneArg = if ($s -eq "default") { @() } else { @("--scene", $s) }
    & $shot --out $bmp --w $W --h $H @sceneArg | Out-Null
    if (-not (Test-Path $bmp)) {
        Write-Output ("{0,-20} RENDER-FAIL" -f $s)
        continue
    }
    $img = [System.Drawing.Image]::FromFile((Resolve-Path $bmp))
    $img.Save((Join-Path (Get-Location) $png), [System.Drawing.Imaging.ImageFormat]::Png)
    $img.Dispose()
    Remove-Item $bmp
    $md5 = Get-Md5 $png
    $rows += [pscustomobject]@{ scene = $s; hash = $md5 }
    Write-Output ("{0,-20} {1} px {2} KB {3}" -f $s, "$W`x$H", [math]::Round((Get-Item $png).Length / 1KB), $md5.Substring(0, 8))
}
$commit = (git rev-parse --short HEAD) -join ''
$manifest = Join-Path $OutDir "manifest.txt"
("commit $commit", "size ${W}x${H}") + ($rows | ForEach-Object { "$($_.hash)  $($_.scene).png" }) |
    Set-Content -Path $manifest
Write-Output "sweep at commit: $commit"
Write-Output "pngs in: $OutDir (manifest: $manifest)"

if ($Baseline -ne "" -and (Test-Path (Join-Path $Baseline "manifest.txt"))) {
    $prev = @{}
    Get-Content (Join-Path $Baseline "manifest.txt") | ForEach-Object {
        if ($_ -match '^([0-9A-F]{32})  (.+\.png)$') { $prev[$Matches[2]] = $Matches[1] }
    }
    $changed = @(); $same = @(); $missing = @()
    foreach ($r in $rows) {
        $name = "$($r.scene).png"
        if (-not $prev.ContainsKey($name)) { $missing += $name }
        elseif ($prev[$name] -ne $r.hash) { $changed += $name }
        else { $same += $name }
    }
    Write-Output ""
    Write-Output "vs baseline $Baseline ($($prev.Count) scenes):"
    Write-Output ("  changed    {0}: {1}" -f $changed.Count, ($changed -join ' '))
    Write-Output ("  identical  {0}: {1}" -f $same.Count, ($same -join ' '))
    if ($missing.Count) { Write-Output ("  new        {0}: {1}" -f $missing.Count, ($missing -join ' ')) }
    Write-Output "  (identical PNGs carry their previous verdict: re-judge only the changed list)"
}
