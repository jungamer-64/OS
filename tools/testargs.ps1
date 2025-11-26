$str = '-drive file="D:\foo bar\kernel.img" -nographic'

function Test-Parse {
	param([string]$Str)
	$s = $Str
	$len = $s.Length
	$i = 0
	$tokens = @()
	while ($i -lt $len) {
		# skip whitespace
		while ($i -lt $len -and [char]::IsWhiteSpace($s[$i])) { $i++ }
		if ($i -ge $len) { break }
		$c = $s[$i]
		$token = ''
		if ($c -eq '"' -or $c -eq "'") {
			$quote = $c; $i++;
			while ($i -lt $len) {
				$ch = $s[$i]
				if ($ch -eq '\\' -and $i + 1 -lt $len) { $token += $s[$i + 1]; $i += 2; continue }
				if ($ch -eq $quote) { $i++; break }
				$token += $ch; $i++
			}
			$tokens += $token; continue
		} else {
			while ($i -lt $len -and -not [char]::IsWhiteSpace($s[$i])) {
				$ch = $s[$i]
				if ($ch -eq '=' -and $i + 1 -lt $len -and ($s[$i + 1] -eq '"' -or $s[$i + 1] -eq "'")) {
					$token += '='; $i++;
					$quote = $s[$i]; $i++;
					while ($i -lt $len) {
						$ch2 = $s[$i]
						if ($ch2 -eq '\\' -and $i + 1 -lt $len) { $token += $s[$i + 1]; $i += 2; continue }
						if ($ch2 -eq $quote) { $i++; break }
						$token += $ch2; $i++
					}
					continue
				}
				$token += $ch; $i++
			}
			$tokens += $token
		}
	}
	return $tokens
}

$tokens = Test-Parse $str
foreach ($t in $tokens) { Write-Host "[$t]" }
