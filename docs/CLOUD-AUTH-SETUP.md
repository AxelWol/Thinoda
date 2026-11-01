# Publisher setup for Thinoda cloud sign-in

This is the maintainer's registration and build guide. **There are currently no
Thinoda publisher OAuth registrations.** An unconfigured build must say cloud
sign-in is unavailable before opening a provider browser or sending a request.
Ordinary users press Connect and consent to the publisher's branded app; they
do not enter client IDs, register apps, or type callback URLs.

This guide prepares supported public-client configurations; it does not create
external registrations, grant permissions, obtain tokens, or establish live
provider acceptance. Public IDs are identifiers, not secrets. Never put client
secrets, generated access tokens, refresh tokens, or account passwords in source,
build variables, binaries, static files, examples, or logs.

## Supported protocol and target combinations

The deterministic `cloud_auth::effective_oauth_config(provider, target)` checks
the following boundaries **before** any browser launch, listener, credential
lookup, or network request. Missing or malformed configuration and blocked
combinations return `AuthError::BuildConfiguration`; adding an ID does not
bypass the protocol gates.

| Provider | Native build preparation | Static browser/SPA preparation |
|---|---|---|
| OneDrive | Public native code + S256 PKCE, personal Microsoft accounts; exact publisher-owned IP loopback URI/port | Code + S256 PKCE with an Entra **SPA** redirect type (needed for token-endpoint CORS) |
| Dropbox | Public code + S256 PKCE; exact registered loopback URI/port | Public code + S256 PKCE; exact HTTPS redirect, short-lived online access |
| Google Drive | **Desktop app** public client, code + S256 PKCE and ephemeral IP loopback; current Google installed-app token reference marks `client_secret` optional | **Disabled.** Google's documented browser code model exchanges codes on a backend. An ID/allowed JavaScript origin does not enable our direct Fetch code exchange. |
| Box | **Disabled.** Current Box code-exchange documentation requires `client_secret` and does not document a supported secret-free PKCE exchange for this implementation. | **Disabled**, for the same reason; API CORS settings alone do not solve confidential token exchange. |
| Nextcloud / WebDAV | Separate explicit Login Flow v2 or app-password flow; no publisher OAuth client | Same flow, with self-hosted server CORS; not enabled by any of these IDs |

For Box, a future reviewed confidential broker would be a new architecture, not
an embedded-secret workaround. For Google browser access, a reviewed Google
Identity Services (GIS) token-model integration or confidential backend would
also be separate work. This implementation adds neither, and never falls back
to implicit grants, device flow, copied tokens, or client-credentials grants.

Protocol eligibility is not a live success guarantee. Publisher consent-screen
verification, production approval, tenant policies, CORS, registered redirects,
API restrictions and token revocation must still be verified on real accounts
before advertising the corresponding build as accepted.

## Public build inputs

`PublisherOAuthRegistrations::from_build()` reads these variables using Rust
`option_env!`. They must be set **when thinoda-platform is compiled**, before
`cargo build`, release packaging, or `trunk build`. Changing a user's runtime
environment or saved legacy OAuth fields has no effect on normal sign-in.

| Variable | Purpose |
|---|---|
| `THINODA_OAUTH_ONEDRIVE_NATIVE_CLIENT_ID` | Entra public native application ID |
| `THINODA_OAUTH_ONEDRIVE_NATIVE_REDIRECT_URI` | Exact registered `http://127.0.0.1:PORT/callback` |
| `THINODA_OAUTH_ONEDRIVE_SPA_CLIENT_ID` | Entra SPA application ID |
| `THINODA_OAUTH_DROPBOX_NATIVE_CLIENT_ID` | Dropbox native app key |
| `THINODA_OAUTH_DROPBOX_NATIVE_REDIRECT_URI` | Exact registered `http://127.0.0.1:PORT/callback` |
| `THINODA_OAUTH_DROPBOX_SPA_CLIENT_ID` | Dropbox browser app key |
| `THINODA_OAUTH_GOOGLE_DRIVE_NATIVE_CLIENT_ID` | Google **Desktop app** client ID |
| `THINODA_OAUTH_GOOGLE_DRIVE_SPA_CLIENT_ID` | Reserved public input; cannot enable the unsupported browser code flow |
| `THINODA_OAUTH_BOX_NATIVE_CLIENT_ID` | Reserved public input; cannot enable unsupported Box public exchange |
| `THINODA_OAUTH_BOX_SPA_CLIENT_ID` | Reserved public input; cannot enable unsupported Box browser exchange |

Unset, empty or malformed IDs disable the corresponding target; native and SPA
IDs never fall back to each other. IDs must be ASCII letters/digits or `-`, `.`,
`_`, with no whitespace, and at most 512 bytes. Obtain actual assigned IDs from
the provider consoles; do not substitute names, URLs or example IDs. There are
no checked-in IDs or invented placeholder defaults.

Native Microsoft/Dropbox URIs must be literal `127.0.0.1`, HTTP, an explicit
unprivileged port (1024–65535), exactly `/callback`, and no query, fragment or
userinfo. A publisher can choose, for example, port **53682** and register
`http://127.0.0.1:53682/callback`. This is a port example, not an existing
registration. The listener binds that exact port; if occupied, Connect fails
explicitly rather than choosing an unregistered port. It never binds an
external interface. Google Desktop retains ephemeral `127.0.0.1` ports.

### Release and Pages build wiring

Public registrations are publisher configuration, not GitHub Secrets. Once
registrations have actually been created, put their public IDs and exact native
URIs in repository/environment **Variables**, with separate deployment/client
registrations where appropriate. Explicitly map these Variables into build-step
`env`. For example, the native Cargo/packaging step needs:

```yaml
env:
  THINODA_OAUTH_ONEDRIVE_NATIVE_CLIENT_ID: ${{ vars.THINODA_OAUTH_ONEDRIVE_NATIVE_CLIENT_ID }}
  THINODA_OAUTH_ONEDRIVE_NATIVE_REDIRECT_URI: ${{ vars.THINODA_OAUTH_ONEDRIVE_NATIVE_REDIRECT_URI }}
  THINODA_OAUTH_DROPBOX_NATIVE_CLIENT_ID: ${{ vars.THINODA_OAUTH_DROPBOX_NATIVE_CLIENT_ID }}
  THINODA_OAUTH_DROPBOX_NATIVE_REDIRECT_URI: ${{ vars.THINODA_OAUTH_DROPBOX_NATIVE_REDIRECT_URI }}
  THINODA_OAUTH_GOOGLE_DRIVE_NATIVE_CLIENT_ID: ${{ vars.THINODA_OAUTH_GOOGLE_DRIVE_NATIVE_CLIENT_ID }}
```

The Pages **web artifact build** step, not merely `actions/deploy-pages`, needs:

```yaml
env:
  THINODA_OAUTH_ONEDRIVE_SPA_CLIENT_ID: ${{ vars.THINODA_OAUTH_ONEDRIVE_SPA_CLIENT_ID }}
  THINODA_OAUTH_DROPBOX_SPA_CLIENT_ID: ${{ vars.THINODA_OAUTH_DROPBOX_SPA_CLIENT_ID }}
```

Repository Variables are not automatically inherited as process environment
variables. `ci.yml` maps native inputs into its native test/build jobs and maps
SPA inputs into its web artifact build only on pushes to `main`, which publish
the canonical Pages deployment. Pull-request web previews stay unconfigured.
`package-target.yml`, called by `release.yml` for every native package, maps the
native inputs at compilation. These mappings do not create registrations or
populate repository Variables; no assigned publisher IDs exist yet.
After registration, set the Variables, rebuild and verify the produced binary/
site. Other deployment workflows must apply equivalent mappings before
compilation. Keep preview builds unconfigured unless their exact HTTPS
deployment has separately been registered. Do not reuse arbitrary preview
origins or production grants.

For local publisher builds, export the same public variables in the shell
that runs Cargo/Trunk. A self-hosted deployment owner uses their own registered
public native/SPA IDs in **their build**; there is no ordinary settings override.
If inputs change, rebuild the relevant platform crate/artifacts; publishing an
old cached artifact does not change its compiled registration.

## Static deployment callback

The app supplies the current deployment URL (browser `location.href`) to
`OAuthTarget::Web { deployment_url }`. `web_callback_from_deployment` derives:

- `https://example.org/` → `https://example.org/`
- `https://example.org/Thinoda/index.html?view=map#selection`
  → `https://example.org/Thinoda/`
- `https://example.org:8443/tools/thinoda/?code=…&state=…`
  → `https://example.org:8443/tools/thinoda/`

The helper removes query/fragment and a terminal `index.html`, preserves the
HTTPS origin, explicit non-default port, case and deployment directory, and
rejects HTTP, userinfo, non-directory deep routes and other file entrypoints.
Serve the directory's `index.html` at its trailing-slash URL. A missing slash,
custom filename or SPA route requires deployment correction, not user typing.
There is no `/callback` rewrite requirement on a static host.

For the documented Thinoda Pages deployment the exact URI to register is
**`https://axelwol.github.io/Thinoda/`**. This is the existing published site URL,
not evidence that any provider registration exists. Register each production
deployment's canonical URI once in the publisher console. Providers must see
the same URI in authorization and exchange; scheme, host, path, case, slash
and port matter. Do not register query/fragment/wildcard callbacks.

Normal Connect calls the effective helper first, stores PKCE/state only in
this tab's sessionStorage, and navigates the **current tab**. On return, consume
the pending transaction once and scrub callback query/fragment before routing
or analytics. Invalid/error callbacks end that transaction; retry begins fresh.
After a valid, unexpired, provider/client-bound one-use callback, the app can
automatically exchange the code and discover the account as completion of the
original explicit Connect and provider consent. A second "Complete sign-in"
click is not required. Unsolicited, cancelled, expired or mismatched callbacks
must not initiate exchange/discovery, and ordinary startup must not look up
stored credentials or silently reconnect.
The callback verifier checks the stored exact origin/path, not a user-supplied
destination. Access tokens stay in sessionStorage, not localStorage; this does
not provide protection against same-origin XSS.

## OneDrive registration

In the Microsoft Entra admin center, prepare a branded Thinoda registration
supporting **personal Microsoft accounts**. The implementation uses
`https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize` and `/token`;
it does not support organizational OneDrive/SharePoint in this slice.

Prefer separate native and SPA application IDs:

1. Native: configure a **Mobile and desktop applications/public native** redirect
   matching the publisher IP loopback URI. Microsoft documents that HTTP
   `127.0.0.1` redirects may require editing `replyUrlsWithType` in the application
   manifest rather than the portal redirect textbox. Do not register it as
   `Web` or `SPA`.
2. SPA: register the canonical HTTPS deployment URI as **Single-page application**
   (`spa`). The token endpoint permits browser CORS for that platform type; a
   `Web`/native registration with the same ID is not a substitute.
3. Request delegated Microsoft Graph **`Files.ReadWrite.AppFolder`** and
   **`offline_access`**. Do not add application-wide file permissions,
   `Files.ReadWrite.All`, profile scopes, or admin grants for this slice.
4. Use authorization **code + S256 PKCE** only. Leave implicit access/ID-token
   grants disabled. No client secret, certificate or client-credentials flow is
   required or provisioned.

Microsoft's localhost exception says ports are ignored for **localhost** URI
matching; its IP-literal guidance prefers `127.0.0.1` but warns the portal HTTP
textbox may reject it. Do not assume `localhost` and `127.0.0.1` or arbitrary IP
ports are interchangeable. Thinoda's effective native configuration therefore
uses an **exact registered IP/port** for Microsoft too, avoiding reliance on
port-ignore interpretation. Verify acceptance in the actual public registration.
Native socket binding never changes to `localhost`.

There is no public OneDrive token-revocation endpoint in these primitives.
Disconnect deletes local credentials even if remote revocation is unsupported;
users can remove the publisher consent in Microsoft account controls.

## Dropbox registration

Create a branded **scoped API app** in the Dropbox App Console. Choose
**App Folder**, not Full Dropbox, for Thinoda's own content. Enable only:

- `files.metadata.read` — listing and revision metadata;
- `files.content.read` — opening/content versions;
- `files.content.write` — explicit upload/update/create.

Authorization explicitly requests this subset; optional sharing APIs require a
separate minimal-scope/consent decision, not automatic permission expansion.
Register the exact native URI and/or canonical HTTPS browser URI. Dropbox
requires exact registered redirects, including loopback; do not assume wildcard
or ephemeral port acceptance. Separate native/browser apps are preferred, though
an owner may deliberately supply the same public app key if its registration
has both exact redirects.

Use `https://www.dropbox.com/oauth2/authorize`, `response_type=code`,
`code_challenge_method=S256`, and exchange at
`https://api.dropboxapi.com/oauth2/token` using the verifier and public app key,
**without a secret**. Disable legacy implicit grants. Effective browser config
requests `token_access_type=online` (short-lived access, no refresh token);
native requests `offline`. Browser token/API CORS and App Folder behavior still
need deployment testing. Production distribution requires the provider's
production/approval requirements, not instructing each user to register an app.

## Google Drive registration

Prepare a publisher Google Cloud project, enable Drive API, configure the
branded OAuth consent screen/audience and required verification, then create a
**Desktop app** OAuth client for native builds. Supply its public client ID.
The native code uses system-browser authorization, S256 PKCE and an ephemeral
`http://127.0.0.1:PORT/callback`; Google documents loopback for desktop clients.
Only **`https://www.googleapis.com/auth/drive.file`** is requested (selected/
app-created files, not unrestricted Drive). Authorization requests offline
access/consent; tokens may be refreshed only through explicit app connection use.

The current installed-app token reference lists `client_secret` as **optional**.
Thinoda never accepts or ships one. Verify both secret-free exchange and refresh
for the actual Desktop registration. A registration/policy that demands a secret
is not a reason to add it to the binary. DPoP-bound tokens are not supported in
this slice. Do not reuse a Web application client for native loopback.

Do **not** enable the reserved SPA ID expecting the same code flow. Google's GIS
browser **code model** expects a backend code-exchange endpoint. GIS's separate
browser token model is not implemented here, nor is a confidential broker.
Allowed JavaScript origins, a HTTPS redirect and API CORS do not turn a
confidential Web code client into a supported direct-browser PKCE exchange.

## Box boundary

Box's documented Platform App OAuth 2.0 code exchange requires `client_secret`.
It does not document the secret-free public PKCE exchange needed by this native/
static implementation. Do not create a Box app expecting that a client ID alone
will enable it. Both reserved Box inputs remain protocol-gated. Any future
broker would need reviewed server-side secret management, exact registered
redirects, minimal file permissions and enterprise approval; that work is not
part of this preparation.

## Offline verification and publisher acceptance

Offline config tests inject clearly named fixture IDs, never production accounts.
They cover absent/invalid registration, separate target selection, permanently
blocked combinations even with IDs, clean subpath callbacks, exact callback
validation and registered-port binding/occupied-port failure. They neither
launch browsers nor obtain credentials.

Before enabling a release, the publisher still needs to perform the external
registrations above; wire public variables into the build; check branded consent,
approved scopes and redirect/CORS configuration; test secret-free exchange,
expiry/refresh, cancellation, offline errors, local deletion/revocation and
deployed native/browser behavior. Do not describe compile-time or mock-test
success as live registration, provider certification or independent approval.

### Official protocol references

Reviewed for this preparation on 2026-11-01:

- [Microsoft authorization code + PKCE and SPA CORS](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow)
- [Microsoft redirect/localhost/IP-literal restrictions](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)
- [Dropbox authorization, exact redirects and secret-free PKCE exchange](https://docs.dropboxapi.com/dropbox-api/docs/get-started/authorization)
- [Dropbox public/native/browser OAuth guidance and App Folder scopes](https://docs.dropboxapi.com/dropbox-api/docs/oauth)
- [Google installed-app PKCE, Desktop registration and optional secret](https://developers.google.com/identity/protocols/oauth2/native-app)
- [Google browser code model and backend exchange](https://developers.google.com/identity/oauth2/web/guides/use-code-model)
- [Box OAuth exchange requirements](https://developer.box.com/guides/authentication/oauth2/without-sdk/)

Provider requirements change. Recheck these official references and the actual
registration before enabling a target; do not lift protocol gates solely
because a build variable is populated.
