function Parse-ArgumentStringChar {
    param([string]$Str)
    if ([string]::IsNullOrWhiteSpace($Str)) { return @() }
    $tokens = @()
    $sb = [System.Text.StringBuilder]::new()
    $inQuote = $false
    $quoteChar = $null
    $len = $Str.Length
    $i = 0
    while ($i -lt $len) {
        $c = $Str[$i]
        if ([char]::IsWhiteSpace($c) -and -not $inQuote) {
            if ($sb.Length -gt 0) { $tokens += $sb.ToString(); $sb.Clear() }
        }
        elseif (($c -eq '"' -or $c -eq "'") -and -not $inQuote) {
            $inQuote = $true; $quoteChar = $c
        }
        elseif ($c -eq $quoteChar -and $inQuote) {
            $inQuote = $false; $quoteChar = $null
        }
        else { $null = $sb.Append($c) }
        $i++
    }
    if ($sb.Length -gt 0) { $tokens += $sb.ToString() }
    return $tokens
}

$str = '-drive file="D:\foo bar\kernel.img" -nographic'
Write-Host "Input: $str"
$out = Parse-ArgumentStringChar $str
for ($i=0; $i -lt $out.Count; $i++) { Write-Host "[$i] $($out[$i])" }

$str2 = '-drive file=D:\foo\bar\kernel.img -nographic'
Write-Host "Input2: $str2"
$out2 = Parse-ArgumentStringChar $str2
for ($i=0; $i -lt $out2.Count; $i++) { Write-Host "[$i] $($out2[$i])" }

$str3 = '-nographic'
Write-Host "Input3: $str3"
$out3 = Parse-ArgumentStringChar $str3
for ($i=0; $i -lt $out3.Count; $i++) { Write-Host "[$i] $($out3[$i])" }
