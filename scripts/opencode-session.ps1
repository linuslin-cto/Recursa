# OpenCode JSONL session shim for Recursa CommandTransport (fork-safe).
# Usage:
#   pwsh -File scripts/opencode-session.ps1 --model <id> --label <l> --mission <text> [--context <dir>]
#
# Env:
#   BUN                      bun binary (default: bun)
#   OPENCODE_DIR             OpenCode checkout; if unset/missing, emit dry-run fixture JSONL
#   OPENCODE_BIN             Optional direct CLI path (skips bun+index.ts resolution)
#   RECURSA_IMPROVER_MODEL   fallback when --model is empty
#   RECURSA_FORCE_FIXTURE    if 1/true, always emit fixture JSONL (safe under parallel forks)
#   RECURSA_SESSION_TIMEOUT_SEC  live spawn timeout seconds (default 600)

param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Rest
)

$ErrorActionPreference = "Stop"

function Get-Arg([string]$name) {
    for ($i = 0; $i -lt $Rest.Count; $i++) {
        if ($Rest[$i] -eq $name -and ($i + 1) -lt $Rest.Count) {
            return $Rest[$i + 1]
        }
    }
    return $null
}

function Write-FixtureJsonl([string]$mission, [string]$context, [string]$label) {
    $snippet = if ($mission) {
        $n = [Math]::Min(80, $mission.Length)
        $mission.Substring(0, $n)
    } else {
        "ok"
    }
    $text = if ($label) { "echo[$label]: $snippet" } else { "echo: $snippet" }
    $payload = [ordered]@{
        type = "text"
        part = [ordered]@{
            type = "text"
            text = $text
        }
    }
    $cost = [ordered]@{
        type = "step_finish"
        part = [ordered]@{
            type = "step-finish"
            cost = 0.0
        }
    }
    Write-Output ($payload | ConvertTo-Json -Compress -Depth 5)
    Write-Output ($cost | ConvertTo-Json -Compress -Depth 5)
    if ($context) {
        $out = Join-Path $context "output"
        New-Item -ItemType Directory -Force -Path $out | Out-Null
        Set-Content -Path (Join-Path $out "answer.md") -Value $text -NoNewline
    }
}

function Invoke-ProcessTimeout([string]$FilePath, [string[]]$Arguments, [int]$TimeoutSec, [string]$WorkDir) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $FilePath
    $psi.Arguments = ($Arguments | ForEach-Object {
        if ($_ -match '[\s"]') { '"' + ($_ -replace '\\', '\\' -replace '"', '\"') + '"' } else { $_ }
    }) -join ' '
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    if ($WorkDir -and (Test-Path -LiteralPath $WorkDir)) {
        $psi.WorkingDirectory = $WorkDir
    }
    $p = New-Object System.Diagnostics.Process
    $p.StartInfo = $psi
    [void]$p.Start()
    $stdoutTask = $p.StandardOutput.ReadToEndAsync()
    $stderrTask = $p.StandardError.ReadToEndAsync()
    if (-not $p.WaitForExit($TimeoutSec * 1000)) {
        try { $p.Kill($true) } catch {}
        [Console]::Error.WriteLine("OpenCode session timed out after ${TimeoutSec}s")
        exit 124
    }
    $stdout = $stdoutTask.Result
    $stderr = $stderrTask.Result
    if ($stderr) { [Console]::Error.WriteLine($stderr.TrimEnd()) }
    if ($stdout) { Write-Output $stdout.TrimEnd() }
    exit $p.ExitCode
}

$model = Get-Arg "--model"
$label = Get-Arg "--label"
$mission = Get-Arg "--mission"
$context = Get-Arg "--context"

if (-not $model -and $env:RECURSA_IMPROVER_MODEL) {
    $model = $env:RECURSA_IMPROVER_MODEL
}

$bun = if ($env:BUN) { $env:BUN } else { "bun" }
$opencode = $env:OPENCODE_DIR
$opencodeBin = $env:OPENCODE_BIN
$forceFixture = $env:RECURSA_FORCE_FIXTURE -in @("1", "true", "TRUE", "yes", "YES")
$timeoutSec = 600
if ($env:RECURSA_SESSION_TIMEOUT_SEC) {
    [void][int]::TryParse($env:RECURSA_SESSION_TIMEOUT_SEC, [ref]$timeoutSec)
}

# Per-fork session marker under context (parallel Propose stages).
if ($context) {
    New-Item -ItemType Directory -Force -Path $context | Out-Null
    $safeLabel = if ($label) { ($label -replace '[\\/:*?"<>|]', '_') } else { "session" }
    $marker = Join-Path $context ".recursa_session_$safeLabel.lock"
    Set-Content -Path $marker -Value ("pid={0}`nstarted={1}`n" -f $PID, (Get-Date).ToString("o")) -NoNewline
}

if ($forceFixture -or (-not $opencodeBin -and (-not $opencode -or -not (Test-Path -LiteralPath $opencode)))) {
    Write-FixtureJsonl -mission $mission -context $context -label $label
    exit 0
}

$workDir = $null
if ($context -and (Test-Path -LiteralPath (Join-Path $context "harness"))) {
    $workDir = Join-Path $context "harness"
} elseif ($context) {
    $workDir = $context
}

if ($opencodeBin) {
    if (-not (Get-Command $opencodeBin -ErrorAction SilentlyContinue) -and -not (Test-Path -LiteralPath $opencodeBin)) {
        [Console]::Error.WriteLine("OPENCODE_BIN not found: $opencodeBin")
        exit 127
    }
    $argList = @("--model", $model, "--label", $label, "--mission", $mission)
    if ($context) { $argList += @("--context", $context) }
    Invoke-ProcessTimeout -FilePath $opencodeBin -Arguments $argList -TimeoutSec $timeoutSec -WorkDir $workDir
}

$entry = Join-Path $opencode "packages/opencode/src/index.ts"
if (-not (Test-Path -LiteralPath $entry)) {
    $entry = Join-Path $opencode "src/index.ts"
}
if (-not (Test-Path -LiteralPath $entry)) {
    $entry = Join-Path $opencode "index.ts"
}
if (-not (Test-Path -LiteralPath $entry)) {
    [Console]::Error.WriteLine("OpenCode entry not found under OPENCODE_DIR=$opencode")
    exit 127
}

$bunCmd = Get-Command $bun -ErrorAction SilentlyContinue
if (-not $bunCmd -and -not (Test-Path -LiteralPath $bun)) {
    [Console]::Error.WriteLine("bun not found (set BUN or install bun): $bun")
    exit 127
}

$argList = @("run", $entry, "--model", $model, "--label", $label, "--mission", $mission)
if ($context) { $argList += @("--context", $context) }
Invoke-ProcessTimeout -FilePath $bun -Arguments $argList -TimeoutSec $timeoutSec -WorkDir $workDir
