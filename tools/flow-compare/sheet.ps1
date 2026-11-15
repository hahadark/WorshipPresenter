param([string]$Dir,[string]$Out,[int[]]$Items,[string]$Kind='main',[string]$Head='')
Add-Type -AssemblyName System.Drawing
$rep = Get-Content "$Dir\report.json" -Raw | ConvertFrom-Json
$rows = @(); foreach($n in $Items){ $f = Join-Path $Dir ('{0:d2}-1-{1}.png' -f $n,$Kind); if(Test-Path $f){ $rows += ,@($n,$f) } }
$w=1600; $rh=[int]($w/3840*1080); $lab=30
$bmp=New-Object Drawing.Bitmap $w,(($rh+$lab)*$rows.Count+34); $g=[Drawing.Graphics]::FromImage($bmp); $g.InterpolationMode='HighQualityBicubic'; $g.Clear([Drawing.Color]::FromArgb(32,32,32))
$f1=New-Object Drawing.Font 'Malgun Gothic',12; $fb=New-Object Drawing.Font 'Malgun Gothic',13,([Drawing.FontStyle]::Bold)
$g.DrawString("왼쪽 Electron  |  오른쪽 Rust   $Head",$fb,[Drawing.Brushes]::Gold,6,5)
$y=34
foreach($r in $rows){ $it=$rep.items[$r[0]-1]; $px=($it.shots[0].$Kind.px)
  $g.DrawString(("{0}. {1}   ({2}장 / {3}장, 그림 차이 {4}%)" -f $r[0],$it.title,$it.count[0],$it.count[1],$px),$f1,[Drawing.Brushes]::White,6,$y+4)
  $img=[Drawing.Image]::FromFile((Resolve-Path $r[1])); $g.DrawImage($img,0,$y+$lab,$w,$rh); $img.Dispose(); $g.DrawLine([Drawing.Pens]::Red,[int]($w/2),$y+$lab,[int]($w/2),$y+$lab+$rh); $y+=$rh+$lab }
$g.Dispose(); $bmp.Save($Out,[Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose(); "$Out $($rows.Count)줄"
