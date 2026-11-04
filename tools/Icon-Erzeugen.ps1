# Zweck:   Erzeugt das Programm-Icon von Feldbuch-NT (blaue Kachel, Zielkreuz mit orangen Marken, "NT"):
#            icons\app_icon.png  (256 x 256, fuer Fenster und Info-Seite)
#            icons\app.ico       (16, 24, 32, 48, 64, 128, 256 px, fuer die EXE; wird vom Build eingebettet)
#          Die kleinen Groessen (16 bis 32 px) bekommen eine vereinfachte Zeichnung ohne inneren Ring und Fadenkreuz.
# Aufruf:  powershell -ExecutionPolicy Bypass -File tools\Icon-Erzeugen.ps1
# Autor:   Johann Gerner

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
$ziel = Join-Path (Split-Path $PSScriptRoot -Parent) "icons"
$S = 256

$blau   = [System.Drawing.Color]::FromArgb(255, 42, 72, 130)
$linie  = [System.Drawing.Color]::FromArgb(150, 131, 149, 182)
$weiss  = [System.Drawing.Color]::White
$orange = [System.Drawing.Color]::FromArgb(255, 238, 146, 30)

function Runde-Ecken([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $r * 2
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

# Text als Pfad, auf die Zielbreite skaliert und in der Zielmitte zentriert
function Text-Zeichnen($g, [string]$text, [float]$mitteX, [float]$mitteY, [float]$breite) {
    $fam = New-Object System.Drawing.FontFamily "Segoe UI"
    $pfad = New-Object System.Drawing.Drawing2D.GraphicsPath
    $pfad.AddString($text, $fam, [int][System.Drawing.FontStyle]::Bold, 100, (New-Object System.Drawing.PointF 0, 0), [System.Drawing.StringFormat]::GenericTypographic)
    $r = $pfad.GetBounds()
    $m = New-Object System.Drawing.Drawing2D.Matrix
    $f = $breite / $r.Width
    $m.Translate(-($r.X + $r.Width / 2), -($r.Y + $r.Height / 2))
    $m.Scale($f, $f, "Append")
    $m.Translate($mitteX, $mitteY, "Append")
    $pfad.Transform($m)
    $pinsel = New-Object System.Drawing.SolidBrush $weiss
    $g.FillPath($pinsel, $pfad)
    $pinsel.Dispose(); $pfad.Dispose()
}

# Zeichnet das Icon in 256 x 256; $einfach = vereinfachte Fassung fuer kleine Groessen
function Zeichne-Icon([bool]$einfach) {
    $b = New-Object System.Drawing.Bitmap $S, $S, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($b)
    $g.SmoothingMode = "AntiAlias"
    $g.TextRenderingHint = "AntiAliasGridFit"
    $g.Clear([System.Drawing.Color]::Transparent)

    $kachel = Runde-Ecken 0 0 $S $S 36
    $g.FillPath((New-Object System.Drawing.SolidBrush $blau), $kachel)

    if ($einfach) {
        $ring = New-Object System.Drawing.Pen $linie, 14
        $g.DrawEllipse($ring, 30, 30, 196, 196)
        $marke = New-Object System.Drawing.Pen $orange, 26
        $laenge = 46
    } else {
        $ring = New-Object System.Drawing.Pen $linie, 6
        $g.DrawEllipse($ring, 28, 28, 200, 200)
        $g.DrawEllipse($ring, 62, 62, 132, 132)
        $kreuz = New-Object System.Drawing.Pen $linie, 5
        $g.DrawLine($kreuz, 128, 14, 128, 242)
        $g.DrawLine($kreuz, 14, 128, 242, 128)
        $marke = New-Object System.Drawing.Pen $orange, 12
        $laenge = 26
    }
    $marke.StartCap = "Round"; $marke.EndCap = "Round"
    $a = 14; $e = 242
    $g.DrawLine($marke, 128, $a, 128, $a + $laenge)
    $g.DrawLine($marke, 128, $e - $laenge, 128, $e)
    $g.DrawLine($marke, $a, 128, $a + $laenge, 128)
    $g.DrawLine($marke, $e - $laenge, 128, $e, 128)

    Text-Zeichnen $g "NT" 128 128 $(if ($einfach) { 150 } else { 132 })
    $g.Dispose()
    return $b
}

function Skaliere($quelle, [int]$n) {
    $b = New-Object System.Drawing.Bitmap $n, $n, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($b)
    $g.InterpolationMode = "HighQualityBicubic"
    $g.SmoothingMode = "HighQuality"
    $g.PixelOffsetMode = "HighQuality"
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($quelle, 0, 0, $n, $n)
    $g.Dispose()
    return $b
}

function Png-Bytes($bild) {
    $ms = New-Object System.IO.MemoryStream
    $bild.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    return $ms.ToArray()
}

$voll = Zeichne-Icon $false
$einfach = Zeichne-Icon $true
$voll.Save((Join-Path $ziel "app_icon.png"), [System.Drawing.Imaging.ImageFormat]::Png)

# ICO mit PNG-Eintraegen (ab Windows Vista ueblich, auch fuer 256 x 256)
$groessen = @(16, 24, 32, 48, 64, 128, 256)
$daten = @()
foreach ($n in $groessen) {
    $quelle = if ($n -le 32) { $einfach } else { $voll }
    $klein = Skaliere $quelle $n
    $daten += ,(Png-Bytes $klein)
    $klein.Dispose()
}
$ms = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $ms
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$groessen.Count)
$offset = 6 + 16 * $groessen.Count
for ($i = 0; $i -lt $groessen.Count; $i++) {
    $n = $groessen[$i]
    $w.Write([byte]$(if ($n -ge 256) { 0 } else { $n }))
    $w.Write([byte]$(if ($n -ge 256) { 0 } else { $n }))
    $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32)
    $w.Write([uint32]$daten[$i].Length)
    $w.Write([uint32]$offset)
    $offset += $daten[$i].Length
}
foreach ($d in $daten) { $w.Write([byte[]]$d) }
$w.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $ziel "app.ico"), $ms.ToArray())
$voll.Dispose(); $einfach.Dispose()
Get-ChildItem $ziel -Filter "app*" | Select-Object Name, Length
