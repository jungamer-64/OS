function Parse-ArgumentStringRegex {
    param([string]$Str)
    if ([string]::IsNullOrWhiteSpace($Str)) { return @() }
    $pattern = @'
("([^"\\]|\\.)*"|'([^'\\]|\\.)*'|\S+)
'@
    $matches = [regex]::Matches($Str, $pattern)
    $tokens = @()
    foreach ($m in $matches) {
        $tok = $m.Groups[1].Value
        if ($tok.Length -gt 1) {
            $first = $tok.Substring(0,1)
            $last = $tok.Substring($tok.Length - 1, 1)
            if (($first -eq '"' -and $last -eq '"') -or ($first -eq "'" -and $last -eq "'")) {
                $tok = $tok.Substring(1, $tok.Length - 2)
                $tok = [System.Text.RegularExpressions.Regex]::Unescape($tok)
            }
        }
        $tokens += $tok
    }
    return $tokens
}

# Test strings
$strs = @('-drive file="D:\foo bar\kernel.img" -nographic', "-drive file='D:\foo bar\kernel.img' -nographic", '-drive file=D:\foo\bar\kernel.img -nographic', '-device "virtio-blk-pci,drive=hd0,serial=1234" -nographic')
foreach ($s in $strs) {
    Write-Host "Input: $s" -ForegroundColor Cyan
    $out = Parse-ArgumentStringRegex $s
    for ($i=0; $i -lt $out.Count; $i++) { Write-Host "[$i] $($out[$i])" -ForegroundColor Yellow }
}
