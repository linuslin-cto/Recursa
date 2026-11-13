# OpenCode JSONL session shim for Recursa CommandTransport.
# Usage:
#   pwsh -File scripts/opencode-session.ps1 --model <id> --label <l> --mission <text> [--context <dir>]
#
# Env:
#   BUN            bun binary (default: bun)
#   OPENCODE_DIR   OpenCode checkout; if unset/missing, emit dry-run fixture JSONL
#   OPENCODE_BIN   Optional direct CLI path (skips bun+index.ts resolution)

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

function Write-FixtureJsonl([string]$mission, [string]$context) {
    $snippet = if ($mission) {
        $n = [Math]::Min(80, $mission.Length)
        $mission.Substring(0, $n)
    } else {
        "ok"
    }
    $payload = [ordered]@{
        type = "text"
        part = [ordered]@{
            type = "text"
            text = "echo: $snippet"
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
        Set-Content -Path (Join-Path $out "answer.md") -Value "echo: $snippet" -NoNewline
    }
}

$model = Get-Arg "--model"
$label = Get-Arg "--label"
$mission = Get-Arg "--mission"
$context = Get-Arg "--context"

$bun = if ($env:BUN) { $env:BUN } else { "bun" }
$opencode = $env:OPENCODE_DIR
$opencodeBin = $env:OPENCODE_BIN

if (-not $opencodeBin -and (-not $opencode -or -not (Test-Path -LiteralPath $opencode))) {
    Write-FixtureJsonl -mission $mission -context $context
    exit 0
}

# Prefer an explicit CLI binary when provided.
if ($opencodeBin) {
    if (-not (Get-Command $opencodeBin -ErrorAction SilentlyContinue) -and -not (Test-Path -LiteralPath $opencodeBin)) {
        [Console]::Error.WriteLine("OPENCODE_BIN not found: $opencodeBin")
        exit 127
    }
    $argList = @("--model", $model, "--label", $label, "--mission", $mission)
    if ($context) { $argList += @("--context", $context) }
    & $opencodeBin @argList 2>&1 | ForEach-Object {
        if ($_ -is [System.Management.Automation.ErrorRecord]) {
            [Console]::Error.WriteLine($_.ToString())
        } else {
            Write-Output $_
        }
    }
    exit $LASTEXITCODE
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
& $bun @argList
exit $LASTEXITCODE
