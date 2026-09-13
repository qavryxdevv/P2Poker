# Copy this file to tools\machine.local.psd1, which git ignores, and fill it in.
# Every value is optional: a script that needs one it cannot find says which (S1-EN).
@{
    # The far test machine, for tools\table-run-split.ps1 and tools\two-network-*.ps1:
    # the ssh login and host, and the private key that reaches it.
    FarTarget  = 'user@far-machine'
    FarKeyPath = 'X:\keys\far-machine-key'

    # Where tools\deploy.ps1 copies the client. Without it: <profile>\p2p-poker.
    DeployTo   = 'C:\Games\P2Poker'
}
