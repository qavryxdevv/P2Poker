# One machine's values for the tools: the far test machine's ssh target and key,
# and where tools\deploy.ps1 puts the client. They live in tools\machine.local.psd1,
# which git ignores; tools\machine.example.psd1 shows the shape. S1-EN: they used to
# be the scripts' own defaults, and the repository names nobody's machines.
#
# Dot-source it, then ask:
#     . (Join-Path $PSScriptRoot 'machine.ps1')
#     $Target = Get-MachineValue 'FarTarget'

$script:MachineValuesFile = Join-Path $PSScriptRoot 'machine.local.psd1'

function Get-MachineValue {
    param([Parameter(Mandatory)][string]$Name)
    if (-not (Test-Path $script:MachineValuesFile)) { return $null }
    $values = Import-PowerShellDataFile -Path $script:MachineValuesFile
    return $values[$Name]
}
