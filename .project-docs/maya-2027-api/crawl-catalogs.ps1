# Retrieves the official Autodesk Maya catalog index pages and extracts every
# linked symbol name into machine-readable text inventories.
# Usage: powershell -File crawl-catalogs.ps1
# Output (in this directory, one symbol per line plus a header comment):
#   commands.txt / commands-2026.txt
#   nodes.txt / nodes-2026.txt
#   cpp-classes.txt / cpp-classes-2026.txt
#   py2-classes.txt / py2-classes-2026.txt
# Note: cpp/py class lists mix namespace rows with class rows, mirroring the
# Doxygen page structure; the header documents the exact source page.

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path

function Get-Catalog {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$AnchorPattern,
        [Parameter(Mandatory = $true)][string]$OutFile
    )

    $response = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 120
    $html = $response.Content

    $names = New-Object System.Collections.Generic.List[string]
    foreach ($match in [regex]::Matches($html, $AnchorPattern)) {
        $name = $match.Groups['name'].Value.Trim()
        if ($name.Length -gt 0) { $names.Add($name) }
    }

    # Preserve first-seen page order. The commands/nodes catalogs are
    # alphabetical; the Doxygen annotated pages are NOT strictly alphabetical
    # because namespace rows group their child classes with the parent, so the
    # flattened lists contain order breaks by design. Do not re-sort here:
    # the order documents the source page structure.
    $unique = $names | Select-Object -Unique
    $header = "# source: $Url`n# retrieved: $((Get-Date).ToUniversalTime().ToString('o'))`n# count: $($unique.Count)"
    $path = Join-Path $here $OutFile
    [System.IO.File]::WriteAllLines($path, @($header) + @($unique))
    Write-Host ("{0}: {1} symbols -> {2}" -f $Name, $unique.Count, $OutFile)
}

$catalogs = @(
    @{ Name = 'commands'; Base = 'https://help.autodesk.com/cloudhelp/{0}/ENU/Maya-Tech-Docs/Commands/index_all.html'; Pattern = 'href="(?<href>[^"]+\.html)"[^>]*>(?<name>[^<]+)</a>'; Out = 'commands{0}.txt' },
    @{ Name = 'nodes'; Base = 'https://help.autodesk.com/cloudhelp/{0}/ENU/Maya-Tech-Docs/Nodes/index_all.html'; Pattern = 'href="(?<href>[^"]+\.html)"[^>]*>(?<name>[^<]+)</a>'; Out = 'nodes{0}.txt' },
    @{ Name = 'cpp-classes'; Base = 'https://help.autodesk.com/cloudhelp/{0}/ENU/MAYA-API-REF/cpp_ref/annotated.html'; Pattern = '<a class="el" href="[^"]+"[^>]*>(?<name>[^<]+)</a>'; Out = 'cpp-classes{0}.txt' },
    @{ Name = 'py2-classes'; Base = 'https://help.autodesk.com/cloudhelp/{0}/ENU/MAYA-API-REF/py_ref/annotated.html'; Pattern = '<a class="el" href="[^"]+"[^>]*>(?<name>[^<]+)</a>'; Out = 'py2-classes{0}.txt' }
)

foreach ($release in @('', '-2026')) {
    $year = if ($release -eq '') { '2027' } else { '2026' }
    foreach ($catalog in $catalogs) {
        Get-Catalog -Name $catalog.Name `
            -Url ($catalog.Base -f $year) `
            -AnchorPattern $catalog.Pattern `
            -OutFile ($catalog.Out -f $release)
    }
}

