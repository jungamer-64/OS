function Parse-ArgumentString {
    param([string]$Str)
    if ([string]::IsNullOrWhiteSpace($Str)) { return @() }
    $pattern = @'("([^\