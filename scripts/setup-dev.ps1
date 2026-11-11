# Set up an rbx-forge clone for development. Run once per clone, from anywhere:
#   powershell -File scripts/setup-dev.ps1            # skills link + pre-commit hooks

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# Claude Code reads skills only from .claude/skills; the skills live in .agents/skills
# (docs/decisions/0001). A directory junction needs no admin rights and isn't committed.
$link = Join-Path $root ".claude\skills"
if (-not (Test-Path $link)) {
    New-Item -ItemType Directory -Force (Join-Path $root ".claude") | Out-Null
    New-Item -ItemType Junction -Path $link -Target (Join-Path $root ".agents\skills") | Out-Null
    Write-Host "linked .claude/skills -> .agents/skills"
}

# pre-commit refuses a commit on main; uvx runs it without a Python project
if (Get-Command uvx -ErrorAction SilentlyContinue) {
    uvx pre-commit install
} else {
    Write-Host "uv is needed for the pre-commit hooks: winget install astral-sh.uv" -ForegroundColor Yellow
}

Write-Host "ready"
