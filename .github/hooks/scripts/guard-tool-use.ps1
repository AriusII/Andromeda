$ErrorActionPreference = 'Stop'

$raw = [Console]::In.ReadToEnd()
try {
    $event = $raw | ConvertFrom-Json
} catch {
    exit 0
}

$toolName = ''
if ($event.PSObject.Properties.Name -contains 'toolName') { $toolName = [string]$event.toolName }
elseif ($event.PSObject.Properties.Name -contains 'tool_name') { $toolName = [string]$event.tool_name }
$argsRaw = $null
if ($event.PSObject.Properties.Name -contains 'toolArgs') { $argsRaw = $event.toolArgs }
elseif ($event.PSObject.Properties.Name -contains 'tool_args') { $argsRaw = $event.tool_args }

$toolArgs = $null
if ($argsRaw -is [string]) {
    try { $toolArgs = $argsRaw | ConvertFrom-Json } catch { $toolArgs = [pscustomobject]@{} }
} elseif ($null -ne $argsRaw) {
    $toolArgs = $argsRaw
} else {
    $toolArgs = [pscustomobject]@{}
}

$shortName = ($toolName -split '\.')[-1]

function Deny([string]$Reason) {
    [pscustomobject]@{
        permissionDecision = 'deny'
        permissionDecisionReason = $Reason
    } | ConvertTo-Json -Compress
    exit 0
}

function Normalize-GuardPath([object]$Value) {
    $path = [string]$Value
    $path = $path.Replace('\', '/')
    while ($path.StartsWith('./')) { $path = $path.Substring(2) }
    return $path
}

function Get-PathReason([string]$Path) {
    if ($Path -match '^docs/') { return 'docs/ is read-only by repository policy' }
    if ($Path -match '^\.codex/') { return '.codex/ is read-only by repository policy' }
    if ($Path -match '^\.github/copilot-instructions\.md$') { return '.github/copilot-instructions.md is managed and must not be edited' }
    if ($Path -match '^Cargo\.lock$') {
        $explicit = $false
        foreach ($name in @('explicitLockUpdate', 'lockUpdateTask')) {
            if ($event.PSObject.Properties.Name -contains $name -and $event.$name) { $explicit = $true }
            if ($toolArgs.PSObject.Properties.Name -contains $name -and $toolArgs.$name) { $explicit = $true }
        }
        if (-not $explicit) { return 'Cargo.lock changes require an explicit lock-update task' }
    }
    return $null
}

function Get-GuardPaths([object]$Value) {
    if ($null -eq $Value) { return @() }
    if ($Value -is [string]) { return @($Value) }
    if ($Value -is [System.Collections.IEnumerable] -and -not ($Value -is [string])) {
        $items = @()
        foreach ($item in $Value) { $items += Get-GuardPaths $item }
        return $items
    }
    if ($Value.PSObject) {
        $items = @()
        foreach ($name in @('path', 'file_path', 'pathInProject', 'filePath', 'notebook_path')) {
            if ($Value.PSObject.Properties.Name -contains $name) { $items += Get-GuardPaths $Value.$name }
        }
        foreach ($name in @('edits', 'operations', 'files')) {
            if ($Value.PSObject.Properties.Name -contains $name) { $items += Get-GuardPaths $Value.$name }
        }
        return $items
    }
    return @()
}

if (@('edit', 'create', 'Write', 'MultiEdit', 'NotebookEdit') -contains $shortName -or @('edit', 'create', 'Write', 'MultiEdit', 'NotebookEdit') -contains $toolName) {
    foreach ($rawPath in Get-GuardPaths $toolArgs) {
        $path = Normalize-GuardPath $rawPath
        $reason = Get-PathReason $path
        if ($reason) { Deny $reason }
    }
}

if (@('bash', 'shell', 'execute', 'powershell') -contains $shortName -or @('bash', 'shell', 'execute', 'powershell') -contains $toolName) {
    $command = ''
    foreach ($name in @('command', 'cmd', 'script')) {
        if ($toolArgs.PSObject.Properties.Name -contains $name) { $command = [string]$toolArgs.$name; break }
    }
    $checks = @(
        @{ Pattern = 'rm\s+-rf\s+/'; Reason = 'Refusing dangerous recursive removal of filesystem root' },
        @{ Pattern = 'Remove-Item\s+-Recurse\s+-Force\s+C:\\'; Reason = 'Refusing dangerous recursive removal of C:\' },
        @{ Pattern = '\bformat\b'; Reason = 'Refusing dangerous format command' },
        @{ Pattern = 'DROP\s+TABLE'; Reason = 'Refusing destructive DROP TABLE command' },
        @{ Pattern = 'git\s+push\b(?=.*--force)(?=.*\b(main|master|trunk|production|prod|release)\b)'; Reason = 'Refusing force push to a protected branch' }
    )
    foreach ($check in $checks) {
        if ($command -match $check.Pattern) { Deny $check.Reason }
    }
}

exit 0
