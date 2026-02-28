
function ParseHawkWorkspace {
    $metadata = cargo metadata --no-deps --offline | ConvertFrom-Json
    $env:HAWK_WORKSPACE = $metadata.workspace_root
    $env:RELEASE_VERSION = $metadata.packages | Where-Object { $_.name -eq "hawk" } | Select-Object -ExpandProperty version
}
