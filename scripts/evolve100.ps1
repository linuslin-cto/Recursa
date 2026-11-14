# Force 100 rounds of Recursa self-evolution evaluation (DeepSeek key from .env).
# Round = self_evolution test + recursa score --k 1 (must show Δ +100 pp).
# Emits: AGENT_LOOP_TICK_evolve100 {...}

$ErrorActionPreference = "Continue"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path

$envFile = Join-Path $Root ".env"
if (Test-Path $envFile) {
    Get-Content $envFile | ForEach-Object {
        if ($_ -match '^\s*#' -or $_ -match '^\s*$') { return }
        $kv = $_.Split('=', 2)
        if ($kv.Count -eq 2) {
            Set-Item -Path ("Env:" + $kv[0].Trim()) -Value $kv[1].Trim()
        }
    }
}

$outDir = Join-Path $Root "runs\evolve100"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$logPath = Join-Path $outDir "rounds.jsonl"
$progressPath = Join-Path $outDir "progress.json"
$demoOut = Join-Path $outDir "demo"

function Write-ProgressJson($round, $passed, $failed, $lastOk, $dsOk, $deltaPp) {
    $obj = [ordered]@{
        round       = $round
        total       = 100
        passed      = $passed
        failed      = $failed
        last_ok     = $lastOk
        delta_pp    = $deltaPp
        deepseek_ok = $dsOk
        updated_at  = (Get-Date).ToString("o")
    }
    ($obj | ConvertTo-Json -Compress) | Set-Content -Path $progressPath -Encoding UTF8
}

function Test-DeepSeek {
    if (-not $env:DEEPSEEK_API_KEY) { return $false }
    $base = if ($env:DEEPSEEK_BASE_URL) { $env:DEEPSEEK_BASE_URL.TrimEnd('/') } else { "https://api.deepseek.com" }
    $model = if ($env:DEEPSEEK_MODEL) { $env:DEEPSEEK_MODEL } else { "deepseek-chat" }
    $body = @{
        model      = $model
        messages   = @(@{ role = "user"; content = "ping" })
        max_tokens = 8
    } | ConvertTo-Json -Depth 5
    try {
        $resp = Invoke-RestMethod -Method Post -Uri "$base/v1/chat/completions" `
            -Headers @{ Authorization = "Bearer $($env:DEEPSEEK_API_KEY)" } `
            -ContentType "application/json" `
            -Body $body `
            -TimeoutSec 60
        return [bool]$resp.choices
    } catch {
        return $false
    }
}

function Get-ScoreDelta([string]$scoreLog) {
    # Prefer parallel line; require +100 pp for offline demo identity
    $m = [regex]::Match($scoreLog, 'parallel k=1:.*?Δ \+([0-9.]+) pp')
    if ($m.Success) { return [double]$m.Groups[1].Value }
    $m2 = [regex]::Match($scoreLog, 'Δ \+([0-9.]+) pp')
    if ($m2.Success) { return [double]$m2.Groups[1].Value }
    return $null
}

Write-Host "evolve100: verifying DeepSeek key (once)..."
$dsOk = Test-DeepSeek
Write-Host "evolve100: deepseek_ok=$dsOk"

$passed = 0
$failed = 0
for ($i = 1; $i -le 100; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $roundLog = Join-Path $outDir ("round-{0:D3}.log" -f $i)
    $roundDemo = Join-Path $demoOut ("r{0:D3}" -f $i)
    New-Item -ItemType Directory -Force -Path $roundDemo | Out-Null

    $okTest = $false
    $okScore = $false
    $deltaPp = $null

    & cargo test -p recursa-core --test self_evolution -q *> $roundLog
    $okTest = ($LASTEXITCODE -eq 0)

    $scoreOut = & cargo run -p recursa-cli -- score --output $roundDemo --k 1 2>&1 | Out-String
    Add-Content -Path $roundLog -Value $scoreOut -Encoding UTF8
    $okScore = ($LASTEXITCODE -eq 0)
    $deltaPp = Get-ScoreDelta $scoreOut
    $okGain = ($null -ne $deltaPp -and $deltaPp -ge 99.99)

    $ok = $okTest -and $okScore -and $okGain
    $sw.Stop()
    if ($ok) { $passed++ } else { $failed++ }

    if (($i % 10) -eq 0) {
        $dsOk = Test-DeepSeek
    }

    $row = [ordered]@{
        round       = $i
        total       = 100
        ok          = $ok
        ok_test     = $okTest
        ok_score    = $okScore
        ok_gain     = $okGain
        delta_pp    = $deltaPp
        elapsed_ms  = $sw.ElapsedMilliseconds
        passed      = $passed
        failed      = $failed
        deepseek_ok = $dsOk
    }
    ($row | ConvertTo-Json -Compress) | Add-Content -Path $logPath -Encoding UTF8
    Write-ProgressJson $i $passed $failed $ok $dsOk $deltaPp
    Write-Output ("AGENT_LOOP_TICK_evolve100 " + ($row | ConvertTo-Json -Compress))
}

$final = [ordered]@{
    round       = 100
    total       = 100
    done        = $true
    passed      = $passed
    failed      = $failed
    deepseek_ok = $dsOk
}
Write-Output ("AGENT_LOOP_TICK_evolve100 " + ($final | ConvertTo-Json -Compress))
exit $(if ($failed -eq 0) { 0 } else { 1 })
