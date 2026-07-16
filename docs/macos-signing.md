# macOS Signing and Notarization

Rau Studio can build macOS bundles without an Apple Developer certificate, but unsigned builds do not pass Gatekeeper for end users. Ad-hoc signing helps avoid broken bundles on Apple Silicon, but it does not replace **Developer ID Application** signing or Apple notarization.

## Current State

- The release workflow builds separate Apple Silicon and Intel artifacts.
- Each macOS app is signed with **Developer ID Application** and notarized.
- The nested `ffmpeg` and `ffprobe` sidecars are signed and verified before the
  outer app bundle is accepted.
- The workflow creates a signed `.dmg`, submits it to Apple, staples the ticket,
  and verifies it with Gatekeeper before uploading it.
- macOS jobs fail before building when a required repository secret is missing.
- GitHub Actions does not read the local `.env`; the values must be configured as
  repository secrets.

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
APPLE_SIGNING_IDENTITY     # "Developer ID Application: Name (TEAMID)"
```

Generate `APPLE_CERTIFICATE`:

```sh
openssl base64 -A -in DeveloperIDApplication.p12 -out apple_certificate_base64.txt
```

## Recommended Flow

1. Create a **Developer ID Application** certificate in Apple Developer.
2. Export it from Keychain as a password-protected `.p12`.
3. Store the base64 `.p12` in the `APPLE_CERTIFICATE` repository secret.
4. Configure all Apple repository secrets listed above.
5. Run the workflow manually or push a `v*` tag.
6. Confirm both macOS jobs report `Accepted` from `notarytool` and upload `.dmg`
   artifacts.

The user-facing macOS artifacts are signed and notarized `.dmg` files.

## Services That Simplify the Process

- **Codemagic**: simplifies code signing and App Store Connect setup for macOS.
- **CrabNebula Cloud**: strong fit for Tauri apps, especially if an updater/distribution platform is needed later.
- **GitHub Actions**: sufficient for this project once Apple secrets are configured.

No serious service removes the need for a paid Apple Developer account and Developer ID certificate. They automate credential handling, signing, notarization, and publishing.
