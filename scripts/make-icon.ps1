# Draws the openiCUE icon (logo C, "glowing stick", chosen 2026-11-08) at every
# Windows icon size, packs it into assets\openicue.ico, and compiles
# assets\openicue.res for the linker. Re-run only when the logo changes.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Split-Path $PSScriptRoot -Parent
$assets = Join-Path $root 'assets'
New-Item -ItemType Directory -Force $assets | Out-Null

function RoundedRect([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = [Math]::Min($r * 2, [Math]::Min($w, $h))
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

function Color([string]$hex) { [System.Drawing.ColorTranslator]::FromHtml($hex) }

# All coordinates are in the design's 128-unit box, scaled to `size`.
function Draw([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.PixelOffsetMode = 'HighQuality'
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.ScaleTransform($size / 128.0, $size / 128.0)

    $tile = RoundedRect 0 0 128 128 28
    $g.FillPath((New-Object System.Drawing.SolidBrush (Color '#17191D')), $tile)

    $body = New-Object System.Drawing.SolidBrush (Color '#2A2D33')
    $blue = Color '#2F9BFF'; $orange = Color '#FF7A3D'
    if ($size -le 24) {
        # Small sizes: thick bar and plain body, so it stays readable.
        $barRect = New-Object System.Drawing.RectangleF 16, 30, 96, 30
        $bar = RoundedRect 16 30 96 30 8
        $g.FillPath($body, (RoundedRect 16 64 96 34 6))
    } else {
        $barRect = New-Object System.Drawing.RectangleF 20, 42, 88, 10
        $bar = RoundedRect 20 42 88 10 5
        if ($size -ge 48) {
            # Soft glow behind the light bar. System.Drawing has no blur, so
            # stack faint layers that grow outward: each adds a little colour,
            # and together they fade smoothly like the design's Gaussian blur.
            $layers = 14
            for ($i = $layers; $i -ge 1; $i--) {
                $spread = $i * 1.1
                $alpha = [int](10 + 4 * ($layers - $i) / $layers)
                $rect = New-Object System.Drawing.RectangleF (20 - $spread), (42 - $spread), (88 + 2 * $spread), (10 + 2 * $spread)
                $glow = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect, ([System.Drawing.Color]::FromArgb($alpha, $blue)), ([System.Drawing.Color]::FromArgb($alpha, $orange)), 0.0
                $g.FillPath($glow, (RoundedRect $rect.X $rect.Y $rect.Width $rect.Height (5 + $spread)))
            }
        }
        # Stick body with the key notch at the bottom.
        $pts = [System.Drawing.PointF[]]@(
            (New-Object System.Drawing.PointF 20, 50), (New-Object System.Drawing.PointF 108, 50),
            (New-Object System.Drawing.PointF 108, 86), (New-Object System.Drawing.PointF 66, 86),
            (New-Object System.Drawing.PointF 66, 80), (New-Object System.Drawing.PointF 62, 80),
            (New-Object System.Drawing.PointF 62, 86), (New-Object System.Drawing.PointF 20, 86))
        $g.FillPolygon($body, $pts)
        if ($size -ge 48) {
            $chip = New-Object System.Drawing.SolidBrush (Color '#17191D')
            foreach ($x in 28, 50, 72) { $g.FillPath($chip, (RoundedRect $x 60 16 14 2)) }
        }
    }
    $grad = New-Object System.Drawing.Drawing2D.LinearGradientBrush $barRect, $blue, $orange, 0.0
    $g.FillPath($grad, $bar)
    $g.Dispose()
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    return ,$ms.ToArray()
}

# ICO file: header, one 16-byte entry per size, then the PNG images.
$sizes = 16, 24, 32, 48, 64, 256
$images = @(); foreach ($s in $sizes) { $images += ,(Draw $s) }
$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $out
$w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $s = $sizes[$i]; $len = $images[$i].Length
    $dim = if ($s -ge 256) { 0 } else { $s }
    $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([UInt16]1); $w.Write([UInt16]32); $w.Write([UInt32]$len); $w.Write([UInt32]$offset)
    $offset += $len
}
foreach ($img in $images) { $w.Write($img) }
$w.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $assets 'openicue.ico'), $out.ToArray())
[System.IO.File]::WriteAllBytes((Join-Path $assets 'openicue-256.png'), $images[-1])

# Resource script: icon id 1, which Windows uses for the exe, shortcuts and the window.
Set-Content -Encoding ascii (Join-Path $assets 'openicue.rc') '1 ICON "openicue.ico"'
$rc = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\rc.exe" | Sort-Object FullName | Select-Object -Last 1
if (-not $rc) { throw 'rc.exe (Windows SDK) not found' }
Push-Location $assets
& $rc.FullName /nologo /fo openicue.res openicue.rc
Pop-Location
Get-ChildItem $assets | Select-Object Name, Length
