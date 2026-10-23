# Windows installation and signed updates

`installer/noram.iss` builds a per-user x64 Inno Setup installer. It checks for the
Evergreen WebView2 Runtime, installs the executable and import helper, creates
shortcuts, registers HTTP/HTTPS capabilities, and offers Windows default-app
settings. It does not overwrite Windows' user-selected default browser. Uninstall
removes registration and installed files while retaining the user's profile.

Development CI compiles an **unsigned** installer as an artifact. An unsigned
development build deliberately cannot install application updates.

For official signed packages, configure these secrets in the GitHub `release`
environment, then manually run **Signed Windows package** on the intended branch:

- `WINDOWS_SIGNING_PFX_BASE64`: the publisher's code-signing PFX, Base64 encoded.
- `WINDOWS_SIGNING_PASSWORD`: its password.

The workflow imports the certificate temporarily, pins its thumbprint into the
Rust build using `NORAM_UPDATE_SIGNER_THUMBPRINT`, signs and timestamps the
executable and installer, verifies the installer, uploads an artifact, and removes
the temporary certificate material. A trusted code-signing certificate is required;
none is generated or bundled by this repository.

After reviewing the artifact, publish a stable GitHub release tagged `vX.Y.Z`
matching `Cargo.toml`, with the installer asset named **NoRAM-Setup-x64.exe**.
Publishing a release is a separate maintainer action; this workflow only packages.

Normal windows check the repository's latest stable release at startup when the
setting is enabled. Installation is an explicit Settings action. The updater:

1. Rejects drafts, prereleases and versions no newer than the running application.
2. Accepts an installer only from this repository's HTTPS release-download path.
3. Limits the download size and verifies GitHub's SHA-256 asset digest.
4. Uses Windows Authenticode validation and checks the embedded publisher pin.
5. Opens the verified installer; the user completes installation after closing NoRAM.

Failures leave the installed app unchanged. Private windows do not check for or
install updates. Certificate rotation requires a migration release signed by the
previous trusted publisher that embeds the new pin. Clean-machine install,
uninstall, and a real signed upgrade still require release validation on a clean
Windows VM; they are not established by compilation alone.
