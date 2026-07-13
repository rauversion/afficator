# macOS Signing and Notarization

Rau Studio can build macOS bundles without an Apple Developer certificate, but unsigned builds do not pass Gatekeeper for end users. Ad-hoc signing helps avoid broken bundles on Apple Silicon, but it does not replace **Developer ID Application** signing or Apple notarization.

## Current State

- macOS artifacts are published as `.app.tar.gz`.
- They may not be signed with Developer ID.
- They may not be notarized.
- Local users may need to remove `com.apple.quarantine` after downloading.

```sh
cd ~/Downloads
tar -xzf RauStudio_0.1.8_arm64.app.tar.gz
xattr -dr com.apple.quarantine "Rau Studio.app"
open "Rau Studio.app"
```

If the app was already copied to `/Applications`:

```sh
xattr -dr com.apple.quarantine "/Applications/Rau Studio.app"
open "/Applications/Rau Studio.app"
```

## Requirements for Gatekeeper-Friendly Distribution

- Paid Apple Developer account.
- **Developer ID Application** certificate.
- Certificate exported from Keychain as `.p12`.
- Apple ID app-specific password or App Store Connect credentials.
- Apple Developer Team ID.

Recommended GitHub Actions secrets:

```text
APPLE_CERTIFICATE          # base64 content of the .p12
APPLE_CERTIFICATE_PASSWORD # password used when exporting the .p12
KEYCHAIN_PASSWORD          # temporary keychain password for the runner
APPLE_ID                   # Apple ID email
APPLE_PASSWORD             # app-specific password
APPLE_TEAM_ID              # Apple Developer Team ID
APPLE_SIGNING_IDENTITY     # optional: "Developer ID Application: Name (TEAMID)"
```

Generate `APPLE_CERTIFICATE`:

```sh
openssl base64 -A -in DeveloperIDApplication.p12 -out apple_certificate_base64.txt
```

## Recommended Flow

1. Create a **Developer ID Application** certificate in Apple Developer.
2. Export it from Keychain as `.p12`.
3. Store the base64 `.p12` in `APPLE_CERTIFICATE`.
4. Configure Apple secrets in GitHub.
5. Update the release workflow to sign with `APPLE_SIGNING_IDENTITY`.
6. Notarize the bundle with Apple before publishing.

Once this is active, the recommended user-facing macOS artifact should be a signed and notarized `.dmg`.

## Services That Simplify the Process

- **Codemagic**: simplifies code signing and App Store Connect setup for macOS.
- **CrabNebula Cloud**: strong fit for Tauri apps, especially if an updater/distribution platform is needed later.
- **GitHub Actions**: sufficient for this project once Apple secrets are configured.

No serious service removes the need for a paid Apple Developer account and Developer ID certificate. They automate credential handling, signing, notarization, and publishing.
