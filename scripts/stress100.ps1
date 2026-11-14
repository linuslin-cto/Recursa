# Force 100 rounds of workspace tests. Loads DeepSeek key from .env (never prints it).
# Emits: AGENT_LOOP_TICK_stress100 {"round":N,"total":100,"ok":true/false,...}

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

$outDir = Join-Path $Root "runs\stress100"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$logPath = Join-Path $outDir "rounds.jsonl"
$progressPath = Join-Path $outDir "progress.json"

function Write-ProgressJson($round, $passed, $failed, $lastOk, $dsOk) {
    $obj = [ordered]@{
        round       = $round
        total       = 100
        passed      = $passed
        failed      = $failed
        last_ok     = $lastOk
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
        model    = $model
        messages = @(@{ role = "user"; content = "ping" })
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

Write-Host "stress100: verifying DeepSeek key (once)..."
$dsOk = Test-DeepSeek
Write-Host "stress100: deepseek_ok=$dsOk"

$passed = 0
$failed = 0
for ($i = 1; $i -le 100; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $roundLog = Join-Path $outDir ("round-{0:D3}.log" -f $i)
    & cargo test --workspace -q *> $roundLog
    $ok = ($LASTEXITCODE -eq 0)
    $sw.Stop()
    if ($ok) { $passed++ } else { $failed++ }

    # Every 10 rounds re-ping DeepSeek (forced live check); rounds 1 already did once
    if (($i % 10) -eq 0) {
        $dsOk = Test-DeepSeek
    }

    $row = [ordered]@{
        round       = $i
        total       = 100
        ok          = $ok
        elapsed_ms  = $sw.ElapsedMilliseconds
        passed      = $passed
        failed      = $failed
        deepseek_ok = $dsOk
    }
    ($row | ConvertTo-Json -Compress) | Add-Content -Path $logPath -Encoding UTF8
    Write-ProgressJson $i $passed $failed $ok $dsOk
    Write-Output ("AGENT_LOOP_TICK_stress100 " + ($row | ConvertTo-Json -Compress))

    # Forced: never abort the campaign on a single failure
}

$final = [ordered]@{
    round       = 100
    total       = 100
    done        = $true
    passed      = $passed
    failed      = $failed
    deepseek_ok = $dsOk
}
Write-Output ("AGENT_LOOP_TICK_stress100 " + ($final | ConvertTo-Json -Compress))
exit $(if ($failed -eq 0) { 0 } else { 1 })
