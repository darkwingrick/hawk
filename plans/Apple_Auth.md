# Update Auth for Github release

## Problem 
Confirmed root cause: the DMG was built without Apple notarization/signing, so Gatekeeper blocks it when downloaded from GitHub.

## Solution
Next you need to configure secrets, push, and rerun.

## Plan
1. Get Apple credentials/assets required for signing + notarization.
2. Convert them into the exact 5 GitHub secrets your workflow expects.
3. Add a quick validation check before rerunning release.

### 1) Get `MACOS_CERTIFICATE` + `MACOS_CERTIFICATE_PASSWORD`
1. Join/verify Apple Developer Program membership (required for Developer ID distribution).
2. In Apple Developer portal, create a **Developer ID Application** certificate if you don’t already have one.
3. Install cert in Keychain Access (double-click `.cer`).
4. In Keychain Access, under **My Certificates**, find your Developer ID cert identity, export it as `.p12`, and set an export password.
5. Convert that `.p12` to one-line base64:
```bash
openssl base64 -A -in /path/to/DeveloperID.p12
```
6. Use output as `MACOS_CERTIFICATE`.
7. Use the `.p12` export password as `MACOS_CERTIFICATE_PASSWORD`.

### 2) Get `APPLE_NOTARIZATION_KEY`, `APPLE_NOTARIZATION_KEY_ID`, `APPLE_NOTARIZATION_ISSUER_ID`
1. Go to App Store Connect → **Users and Access** → **Integrations** → **App Store Connect API**.
2. Create a **Team API key** (or use existing key).
3. Download the `.p8` private key (one-time download).
4. Copy:
   - Key ID → `APPLE_NOTARIZATION_KEY_ID`
   - Issuer ID → `APPLE_NOTARIZATION_ISSUER_ID`
5. Open the `.p8` file and copy full contents (including `-----BEGIN PRIVATE KEY-----` / `-----END PRIVATE KEY-----`) into `APPLE_NOTARIZATION_KEY`.

### 3) Add all 5 secrets in GitHub
Repo → Settings → Secrets and variables → Actions → New repository secret:
1. `MACOS_CERTIFICATE` (base64 of `.p12`)
2. `MACOS_CERTIFICATE_PASSWORD`
3. `APPLE_NOTARIZATION_KEY` (raw `.p8` text)
4. `APPLE_NOTARIZATION_KEY_ID`
5. `APPLE_NOTARIZATION_ISSUER_ID`

### 4) Important repo-specific check
Your signing script hardcodes identity name here: [bundle-mac](/Users/rray/Dev/hawk/script/bundle-mac:13).  
If your Developer ID certificate common name is not exactly `Hawk Industries, Inc.`, signing will fail. If needed, update that constant to your cert identity string.

### 5) Validate and rerun
1. Trigger `Release` workflow.
2. Confirm `Validate macOS signing and notarization secrets` passes.
3. Confirm notarization/stapling passes in `Build Hawk macOS bundle`.
4. Download new DMG and test open on a clean Mac.

### Official references
- Developer ID certificates: [developer.apple.com/help/account/certificates/create-developer-id-certificates](https://developer.apple.com/help/account/certificates/create-developer-id-certificates)  
- Developer ID / Gatekeeper overview: [developer.apple.com/developer-id](https://developer.apple.com/developer-id/)  
- Notarization workflow (`notarytool`): [developer.apple.com/documentation/security/customizing-the-notarization-workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)  
- App Store Connect API keys: [developer.apple.com/help/app-store-connect/get-started/app-store-connect-api](https://developer.apple.com/help/app-store-connect/get-started/app-store-connect-api)  
- Keychain export items (for `.p12` workflow): [support.apple.com/guide/keychain-access/kyca35961/mac](https://support.apple.com/guide/keychain-access/kyca35961/mac)

**Summary**
- You need 5 secrets: 2 from Developer ID cert export (`.p12`) and 3 from App Store Connect API key creation.
- Biggest pitfalls: wrong secret format (`.p8` should be raw text, `.p12` should be base64) and certificate identity mismatch with [bundle-mac](/Users/rray/Dev/hawk/script/bundle-mac:13).
- After secrets are set, rerun release; if it fails, share the first `codesign` or `notarytool` error line.
