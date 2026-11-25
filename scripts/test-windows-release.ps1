#Requires -Version 7.0
param(
    [Parameter(Mandatory = $true)][string]$Path
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Reflection.Metadata

$exe = (Resolve-Path -LiteralPath $Path).Path
$stream = [System.IO.File]::OpenRead($exe)
try {
    $reader = [System.Reflection.PortableExecutable.PEReader]::new($stream)
    try {
        $imports = $reader.PEHeaders.PEHeader.ImportTableDirectory
        if ($imports.Size -gt 0) {
            $table = $reader.GetSectionData($imports.RelativeVirtualAddress)
            # Each PE import descriptor is 20 bytes; the DLL name RVA is at offset 12.
            for ($offset = 0; $offset + 20 -le [Math]::Min($imports.Size, $table.Length); $offset += 20) {
                $descriptor = [byte[]]$table.GetContent($offset, 20)
                $nameRva = [BitConverter]::ToInt32($descriptor, 12)
                if ($nameRva -eq 0) { break }

                $name = $reader.GetSectionData($nameRva)
                $bytes = [byte[]]$name.GetContent(0, [Math]::Min(256, $name.Length))
                $end = [Array]::IndexOf($bytes, [byte]0)
                if ($end -le 0) { throw 'Invalid DLL name in the PE import table.' }
                $dll = [Text.Encoding]::ASCII.GetString($bytes, 0, $end)
                if ($dll -match '^(vcruntime|msvcp|msvcr|concrt|ucrtbase|api-ms-win-crt-)') {
                    throw "The Windows release executable imports $dll. Build it with static MSVC CRT linkage."
                }
            }
        }
    }
    finally {
        $reader.Dispose()
    }
}
finally {
    $stream.Dispose()
}

& $exe --version
if ($LASTEXITCODE -ne 0) {
    throw "The Windows release executable failed to start (exit code $LASTEXITCODE)."
}