//! User-initiated cloud operations. No account lookup, HTTP request or credential
//! restore happens at startup, when selecting a provider, or when opening Settings.
//! Native futures run on a dedicated Tokio worker; web futures use spawn_local.
//! This module deliberately does not use the local-file or autosave destinations.

use std::{
    collections::HashMap,
    future::Future,
    path::Path,
    pin::Pin,
    sync::{Arc, mpsc},
    time::Duration,
};

use egui::{Context, Ui, ViewportId};
use thinoda_io::{ExportOptions, Format, ImportReport};
use thinoda_platform::{
    cloud_auth::{
        self, AppPassword, AuthClient, Cancellation, CloudAccountType, CloudCredential,
        CloudCredentialStore, EffectiveOAuthConfig, OAuthTarget, ProviderCredentialStore,
    },
    remote_storage::{
        Authentication, BoxConfig, BoxProvider, BoxShareAccess, DropboxConfig, DropboxProvider,
        GoogleDriveConfig, GoogleDriveProvider, GraphConfig, GraphDiscoveryConfig, GraphProvider,
        GraphShareScope, GraphStartLocation, RemoteVersion, ShareLink, WebDavConfig,
        WebDavProvider,
    },
    settings::{CloudApiProvider as CloudProvider, CloudSettings},
    storage::{
        CancelToken, Entry, ExpectedRevision, Revision, StorageError, StorageId, StorageProvider,
    },
};
use thinoda_ui::{Dialog, icons, tokens::Palette, widgets};

use crate::{
    ThinodaApp,
    doc::OpenDoc,
    i18n::{self, t},
};

/// Complete translation handoff: includes existing keys and provider product
/// names (keep those unchanged). The parent owns all locale catalogs.
#[allow(dead_code)]
pub(crate) const CLOUD_UI_KEYS: &[&str] = &[
    "Additional or unsupported links",
    "Box",
    "Browse cloud files…",
    "Browser document storage",
    "Browser storage information is unavailable.",
    "Browser storage is private to this site. Check quota or request persistence explicitly; the browser may decline.",
    "Browser storage persistence is granted.",
    "Browser storage persistence is not granted.",
    "Browser storage persistence request failed.",
    "Browser storage persistence status is unavailable.",
    "Cancel",
    "Check storage usage",
    "Cloud sign-in configuration is invalid. Check the server address or contact the publisher.",
    "Checking browser storage…",
    "Choose a backup folder outside detected cloud sync folders. This does not change the provider's synchronization or version history.",
    "Choose a single file name ending in .thinoda.",
    "Choose an existing folder outside every detected cloud sync folder.",
    "Choose backup folder…",
    "Clear",
    "Close",
    "Cloud accounts",
    "Cloud accounts are optional. Selecting a provider or opening Settings never connects.",
    "Cloud connection changed. Downloaded content was not installed.",
    "Cloud document",
    "Cloud document saved.",
    "Cloud save conflict",
    "The cloud revision changed or the new file name already exists. Your edits remain open. No overwrite or retry will be attempted. Cancel this save, then use Save As locally or upload a new copy with a different name.",
    "Cloud file changed. No overwrite was attempted; open the remote file separately or upload a new copy.",
    "Cloud file name",
    "Cloud files",
    "Cloud folder listed.",
    "Cloud operation",
    "Cloud operation cancelled. An upload may already have reached the server; reopen before saving again.",
    "Cloud provider",
    "Cloud request failed. Your local edits are unchanged.",
    "Cloud sign-in cancelled.",
    "Cloud sign-in failed. Live provider authentication has not been verified for this build.",
    "Cloud snapshot uploaded. Newer edits or the local source still need saving.",
    "Cloud worker could not start. No request was made.",
    "Cloud worker stopped. Your local edits are unchanged.",
    "Complete sign-in after the browser redirect.",
    "Connect",
    "Provider sign-in is not enabled in this build. The publisher must configure a registered OAuth application before you can connect.",
    "Connect opens the selected provider’s sign-in page so you can authorize Thinoda. Account folders are chosen automatically; no registration IDs are needed.",
    "Web sign-in redirects this tab. Save local work before connecting.",
    "Connect cloud account",
    "Connect contacts your configured provider and may open its sign-in page. Browsing sends folder identifiers; Open downloads content. Save and Upload send document content only when you request them.",
    "Connected. Choose List folder to browse; no files are downloaded automatically.",
    "Connecting cloud account…",
    "Connector attachments were detached",
    "Copy link",
    "Could not commit document drafts. Your edits are unchanged.",
    "Could not encode the cloud document. Your local edits are unchanged.",
    "Could not import the cloud document. No existing document was changed.",
    "Could not open the cloud sign-in page.",
    "Create restricted share link",
    "Create share link",
    "Creating restricted share link…",
    "Credentials use the OS keychain on desktop and this tab's session storage on web. No cloud autosave or background reconnection.",
    "Disconnect",
    "Disconnect cloud account",
    "Disconnected locally. Provider grants and remote files were not deleted; revoke access in the provider's account settings.",
    "Disconnecting cloud account…",
    "Downloading cloud document…",
    "Dropbox",
    "Dropbox's starting folder must be an absolute path or empty for the app root.",
    "File ID",
    "Folder",
    "Formatting and styles",
    "Google Drive",
    "I agree to this action",
    "I authorize Thinoda to contact this provider, open sign-in if needed, and store credentials securely. This does not browse, upload or share any document.",
    "Images and attachments",
    "Import requires confirmation",
    "List folder",
    "Listing cloud folder…",
    "Loading version metadata…",
    "Local backups",
    "Markers and task data",
    "Google Drive allows duplicate file names. This upload creates a distinct new file in the selected folder, even if the same name already exists. Existing files are not replaced. Later Save targets only the returned file ID.",
    "Cloud creation returned no stable identity or revision. The upload may have reached the server; reopen before saving again.",
    "Nextcloud / ownCloud / WebDAV",
    "Nextcloud default: remote.php/dav/files. Leave empty if the server URL is the complete DAV collection.",
    "No stored credentials. Start a new sign-in.",
    "None",
    "Not possible here",
    "OneDrive",
    "OneDrive connection locates the app folder and may create it. No document is transferred, and access is not broadened if discovery fails.",
    "OneDrive requires the drive ID and app-folder item ID for your registration.",
    "Open",
    "Open remote copy",
    "Other unsupported map data",
    "Parent folder",
    "Progress is indeterminate. Cancel cannot undo an upload already accepted by the provider.",
    "Remove this connection and its stored credentials on this device/tab. Documents and unsaved edits remain open. Remote files and provider grants are not deleted.",
    "Request persistent storage",
    "Requesting persistent storage…",
    "Restricted share link",
    "Restricted share link created. It was not opened or copied automatically.",
    "Revision",
    "Rich notes",
    "Root folder",
    "Save local work before redirecting this tab to sign in.",
    "Secure credential storage is unavailable. No connection was retained.",
    "Select and configure a cloud provider first.",
    "Storage quota (bytes)",
    "Storage used (bytes)",
    "Stored credentials do not match this configuration. Start a new sign-in.",
    "This changes sharing on the selected remote file. Box uses existing collaborators; OneDrive uses organization-only view access. No anonymous/public link is requested. The provider may reject this permission.",
    "This document's cloud connection was disconnected. Save As locally or upload a new copy; reconnecting does not retarget it.",
    "This folder has no safe new-file upload path. Choose another folder.",
    "This operation is unavailable for this provider or file size.",
    "Unavailable",
    "Unsupported source content may be omitted. The cloud original is not changed.",
    "Upload",
    "Upload a snapshot of the current document to the chosen folder as a new .thinoda file. Existing files are never overwritten. Local documents remain local; unsaved documents gain an explicit cloud Save destination.",
    "Upload current document",
    "Upload current document…",
    "Uploading…",
    "Use HTTPS without credentials, a query or a fragment.",
    "Use an app password instead of browser sign-in",
    "Use previously stored credentials",
    "Version",
    "Version metadata loaded. No version content was downloaded.",
    "Versions",
    "Versions and sharing are unavailable where the adapter does not implement them.",
    "WebDAV app password",
    "WebDAV collection prefix",
    "WebDAV server URL",
    "WebDAV username",
];

type Outcome = Result<Completion, &'static str>;
#[cfg(not(target_arch = "wasm32"))]
type Job = Pin<Box<dyn Future<Output = Outcome> + Send>>;
#[cfg(target_arch = "wasm32")]
type Job = Pin<Box<dyn Future<Output = Outcome>>>;

fn provider_label(provider: CloudProvider) -> &'static str {
    match provider {
        CloudProvider::None => "None",
        CloudProvider::WebDav => "Nextcloud / ownCloud / WebDAV",
        CloudProvider::OneDrive => "OneDrive",
        CloudProvider::Dropbox => "Dropbox",
        CloudProvider::Box => "Box",
        CloudProvider::GoogleDrive => "Google Drive",
    }
}

fn account_type(provider: CloudProvider) -> Option<CloudAccountType> {
    match provider {
        CloudProvider::None => None,
        CloudProvider::WebDav => Some(CloudAccountType::WebDav),
        CloudProvider::OneDrive => Some(CloudAccountType::OneDrive),
        CloudProvider::Dropbox => Some(CloudAccountType::Dropbox),
        CloudProvider::Box => Some(CloudAccountType::Box),
        CloudProvider::GoogleDrive => Some(CloudAccountType::GoogleDrive),
    }
}

// Constructors run only inside a consented job. No credential-bearing type
// derives Debug; neither provider failures nor imported content reach logs.
enum Connection {
    WebDav(WebDavProvider),
    Graph(GraphProvider),
    Dropbox(DropboxProvider),
    Box(BoxProvider),
    Drive(GoogleDriveProvider),
    #[cfg(all(test, feature = "integrated-cloud-services"))]
    Mock(Box<dyn StorageProvider>),
    #[cfg(all(test, feature = "integrated-cloud-services"))]
    MockCreator(Box<dyn StorageProvider>, StorageId),
}
impl Connection {
    fn storage(&self) -> &dyn StorageProvider {
        match self {
            Self::WebDav(p) => p,
            Self::Graph(p) => p,
            Self::Dropbox(p) => p,
            Self::Box(p) => p,
            Self::Drive(p) => p,
            #[cfg(all(test, feature = "integrated-cloud-services"))]
            Self::Mock(p) => p.as_ref(),
            #[cfg(all(test, feature = "integrated-cloud-services"))]
            Self::MockCreator(p, _) => p.as_ref(),
        }
    }
    /// The UI and command use the same availability check. Creation support
    /// must reflect an implemented adapter operation, not provider branding.
    fn can_create_in(&self, folder: &Folder) -> bool {
        self.creation_parent(folder).is_some()
    }

    fn creation_parent(&self, folder: &Folder) -> Option<StorageId> {
        match self {
            Self::WebDav(_) | Self::Graph(_) | Self::Box(_) | Self::Drive(_)
                if !folder.id.0.is_empty() =>
            {
                Some(folder.id.clone())
            }
            Self::Dropbox(_) => {
                // Prefer the actual folder ID returned by listing. Root paths
                // remain supported, but creation never reconstructs a nested
                // destination from user-visible folder names when an ID exists.
                if folder
                    .id
                    .0
                    .strip_prefix("id:")
                    .is_some_and(|id| !id.is_empty())
                {
                    Some(folder.id.clone())
                } else {
                    folder
                        .dropbox_path
                        .as_ref()
                        .map(|path| StorageId(path.clone()))
                }
            }
            #[cfg(all(test, feature = "integrated-cloud-services"))]
            Self::MockCreator(_, _) => Some(folder.id.clone()),
            _ => None,
        }
    }

    async fn create(
        &self,
        parent: &StorageId,
        name: &str,
        bytes: &[u8],
        cancel: CancelToken,
    ) -> Result<Entry, StorageError> {
        match self {
            Self::WebDav(p) => p.create_file(parent, name, bytes, cancel).await,
            Self::Graph(p) => p.create_file(parent, name, bytes, cancel).await,
            Self::Dropbox(p) => p.create_file(parent, name, bytes, cancel).await,
            Self::Box(p) => p.create_file(parent, name, bytes, cancel).await,
            Self::Drive(p) => p.create_file(parent, name, bytes, cancel).await,
            #[cfg(all(test, feature = "integrated-cloud-services"))]
            Self::MockCreator(p, result_id) => {
                let revision = p
                    .write(parent, bytes, ExpectedRevision::Missing, cancel)
                    .await?;
                Ok(Entry {
                    id: result_id.clone(),
                    name: name.into(),
                    revision: Some(revision),
                    size: bytes.len() as u64,
                    is_dir: false,
                    availability: thinoda_platform::storage::Availability::OnlineOnly,
                })
            }
            #[cfg(all(test, feature = "integrated-cloud-services"))]
            Self::Mock(_) => Err(StorageError::Unsupported),
        }
    }
    fn upload_warning(&self) -> Option<&'static str> {
        matches!(self, Self::Drive(_)).then_some(
            "Google Drive allows duplicate file names. This upload creates a distinct new file in the selected folder, even if the same name already exists. Existing files are not replaced. Later Save targets only the returned file ID.",
        )
    }
    async fn versions(
        &self,
        id: &StorageId,
        cancel: CancelToken,
    ) -> Result<Vec<RemoteVersion>, StorageError> {
        match self {
            Self::Graph(p) => p.list_versions(id, cancel).await,
            Self::Box(p) => p.list_versions(id, cancel).await,
            _ => Err(StorageError::Unsupported),
        }
    }
    async fn share(&self, id: &StorageId, cancel: CancelToken) -> Result<ShareLink, StorageError> {
        match self {
            Self::Graph(p) => {
                p.create_share_link(id, GraphShareScope::Organization, cancel)
                    .await
            }
            Self::Box(p) => {
                p.create_share_link(id, BoxShareAccess::Collaborators, cancel)
                    .await
            }
            _ => Err(StorageError::Unsupported),
        }
    }
}

#[derive(Clone)]
struct Folder {
    id: StorageId,
    name: String,
    // Dropbox root configuration can be a path. Nested creation prefers its
    // listed folder ID; display names are not used to reconstruct that ID.
    dropbox_path: Option<String>,
}

#[derive(Clone)]
struct Source {
    /// A connection session, NOT just a provider. Reconnecting must never
    /// silently associate an old document with a different signed-in account.
    session: u64,
    id: StorageId,
    revision: Revision,
    name: String,
    format: Format,
}

struct Save {
    key: String,
    state: u64,
    source: Source,
    /// Uploading a local document is a copy, not a replacement of its local save.
    attach: bool,
}

struct Opened {
    report: ImportReport,
    source: Source,
    viewport: ViewportId,
}

struct Conflict {
    save: Save,
    can_open_remote: bool,
}

enum Completion {
    Connected(Connection, Folder),
    Disconnected,
    Listed(Folder, Vec<Entry>),
    Opened(Opened),
    Saved(Save, Revision),
    Conflict(Conflict),
    Versions(Vec<RemoteVersion>),
    Shared(ShareLink),
    #[cfg(target_arch = "wasm32")]
    Estimate(thinoda_platform::browser_storage::StorageEstimate),
    #[cfg(target_arch = "wasm32")]
    Persistence(bool),
}

#[derive(Clone, Copy)]
enum Consent {
    Connect,
    Disconnect,
    Upload,
    Share,
}

/// App-session state only. Secrets are transient inputs or secure-store values,
/// never persisted as settings or included in Debug output.
#[derive(Default)]
pub(crate) struct CloudProgram {
    connection: Option<Arc<Connection>>,
    session: u64,
    sources: HashMap<String, Source>,
    folders: Vec<Folder>,
    entries: Vec<Entry>,
    selected: Option<Entry>,
    browser_open: bool,
    consent: Option<Consent>,
    consented: bool,
    username: String,
    password: String,
    use_stored: bool,
    app_password_login: bool,
    server_draft: Option<String>,
    upload_name: String,
    pending: Option<Job>,
    running: Option<mpsc::Receiver<Outcome>>,
    cancel: CancelToken,
    auth_cancel: Cancellation,
    busy_label: Option<&'static str>,
    saving_key: Option<String>,
    status: Option<&'static str>,
    import: Option<Opened>,
    conflict: Option<Conflict>,
    versions: Vec<RemoteVersion>,
    share_link: Option<ShareLink>,
    #[cfg(target_arch = "wasm32")]
    estimate: Option<thinoda_platform::browser_storage::StorageEstimate>,
}

impl Drop for CloudProgram {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.auth_cancel.cancel();
    }
}

impl CloudProgram {
    fn busy(&self) -> bool {
        self.pending.is_some() || self.running.is_some()
    }

    pub(crate) fn modal_open(&self) -> bool {
        self.consent.is_some()
            || self.browser_open
            || self.import.is_some()
            || self.conflict.is_some()
    }

    /// Separate from dialogs.conflict: this owner is a remote revision conflict,
    /// not a local filesystem path. Continuations must wait while it is present.
    pub(crate) fn conflict_for(&self, key: &str) -> bool {
        self.conflict
            .as_ref()
            .is_some_and(|conflict| conflict.save.key == key)
    }

    /// Call after successful local Save As and on tab close. Never changes
    /// document contents or dirty state. Shared save guards prohibit concurrent
    /// local/cloud writes; completion additionally checks the local path.
    pub(crate) fn forget_document(&mut self, key: &str) {
        self.sources.remove(key);
        if self.conflict_for(key) {
            self.conflict = None;
        }
    }

    pub(crate) fn saving_key(&self, key: &str) -> bool {
        self.saving_key.as_deref() == Some(key)
    }

    pub(crate) fn saving(&self) -> bool {
        self.saving_key.is_some()
    }

    fn ask(&mut self, consent: Consent) {
        self.consent = Some(consent);
        self.consented = false;
    }

    fn queue(
        &mut self,
        label: &'static str,
        job: impl Future<Output = Outcome> + 'static + JobSend,
    ) {
        self.busy_label = Some(label);
        self.status = None;
        self.pending = Some(Box::pin(job));
    }

    fn new_cancellation(&mut self) -> (CancelToken, Cancellation) {
        self.cancel = CancelToken::default();
        self.auth_cancel = Cancellation::default();
        (self.cancel.clone(), self.auth_cancel.clone())
    }

    fn detach(&mut self) {
        self.cancel.cancel();
        self.auth_cancel.cancel();
        self.connection = None;
        self.entries.clear();
        self.folders.clear();
        self.selected = None;
        self.versions.clear();
        self.share_link = None;
        self.browser_open = false;
        self.username.clear();
        self.password.clear();
        // Keep source identities: ordinary Save must not fall through to a
        // local write or bind to a different account after Disconnect.
    }
}

// Keeps the executor bound out of the wasm implementation.
#[cfg(not(target_arch = "wasm32"))]
trait JobSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> JobSend for T {}
#[cfg(target_arch = "wasm32")]
trait JobSend {}
#[cfg(target_arch = "wasm32")]
impl<T> JobSend for T {}

fn storage_failure(error: StorageError) -> &'static str {
    match error {
        StorageError::Conflict { .. } => {
            "Cloud file changed. No overwrite was attempted; open the remote file separately or upload a new copy."
        }
        StorageError::Cancelled => {
            "Cloud operation cancelled. An upload may already have reached the server; reopen before saving again."
        }
        StorageError::Unsupported => {
            "This operation is unavailable for this provider or file size."
        }
        _ => "Cloud request failed. Your local edits are unchanged.",
    }
}
fn auth_failure(error: cloud_auth::AuthError) -> &'static str {
    match error {
        cloud_auth::AuthError::Cancelled => "Cloud sign-in cancelled.",
        cloud_auth::AuthError::Storage => {
            "Secure credential storage is unavailable. No connection was retained."
        }
        cloud_auth::AuthError::Configuration => {
            "Cloud sign-in configuration is invalid. Check the server address or contact the publisher."
        }
        cloud_auth::AuthError::BuildConfiguration => {
            "Provider sign-in is not enabled in this build. The publisher must configure a registered OAuth application before you can connect."
        }
        _ => {
            "Cloud sign-in failed. Live provider authentication has not been verified for this build."
        }
    }
}

/// Pure build/deployment configuration lookup. Legacy preferences never act as
/// a fallback registration and this function never reads credentials or does I/O.
fn consumer_oauth_config(provider: CloudProvider) -> Result<EffectiveOAuthConfig, &'static str> {
    let provider = account_type(provider).ok_or("Select and configure a cloud provider first.")?;
    #[cfg(not(target_arch = "wasm32"))]
    let target = OAuthTarget::Native;
    #[cfg(target_arch = "wasm32")]
    let deployment = web_sys::window()
        .and_then(|window| window.location().href().ok())
        .ok_or("Provider sign-in is not enabled in this build. The publisher must configure a registered OAuth application before you can connect.")?;
    #[cfg(target_arch = "wasm32")]
    let target = OAuthTarget::Web {
        deployment_url: &deployment,
    };
    cloud_auth::effective_oauth_config(provider, target).map_err(auth_failure)
}

fn consumer_settings(
    settings: &CloudSettings,
    config: Option<&EffectiveOAuthConfig>,
) -> CloudSettings {
    let mut settings = settings.clone();
    // These are transient worker inputs, never written back to preferences.
    settings.client_id = config.map_or_else(String::new, |config| config.oauth.client_id().into());
    settings.redirect_uri = config
        .and_then(|config| config.web_redirect_uri.clone())
        .unwrap_or_default();
    settings.onedrive_drive_id.clear();
    settings.root_folder_id.clear();
    settings
}

/// Reject accidental credentials/signed URLs before public configuration enters
/// Settings. Adapter/auth URL validation remains the final authority.
fn public_url(raw: &str) -> bool {
    let raw = raw.trim();
    let Some((scheme, rest)) = raw.split_once("://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    let loopback = authority == "127.0.0.1"
        || authority
            .strip_prefix("127.0.0.1:")
            .is_some_and(|p| p.parse::<u16>().is_ok())
        || authority == "[::1]"
        || authority
            .strip_prefix("[::1]:")
            .is_some_and(|p| p.parse::<u16>().is_ok());
    (scheme == "https" || (scheme == "http" && loopback))
        && !authority.is_empty()
        && !raw.contains(['@', '?', '#', '\\'])
        && !raw.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn segment(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn leaf_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.len() <= 240
        && !matches!(name, "." | "..")
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
}

fn dav_collection(
    settings: &CloudSettings,
    credential: &AppPassword,
) -> Result<String, &'static str> {
    if !public_url(&credential.server)
        || credential.username.is_empty()
        || credential.username.contains(':')
        || credential.password.is_empty()
        || !leaf_name(&credential.username)
    {
        return Err(
            "Cloud sign-in configuration is invalid. Check the server address or contact the publisher.",
        );
    }
    let prefix = settings.webdav_collection.trim_matches('/');
    if prefix.is_empty() {
        return Ok(format!("{}/", credential.server.trim_end_matches('/')));
    }
    if prefix
        .split('/')
        .any(|part| !leaf_name(part) || part.contains(['?', '#', '%']))
    {
        return Err(
            "Cloud sign-in configuration is invalid. Check the server address or contact the publisher.",
        );
    }
    Ok(format!(
        "{}/{}/{}/",
        credential.server.trim_end_matches('/'),
        prefix,
        segment(&credential.username)
    ))
}

fn connect_from_credential(
    settings: &CloudSettings,
    credential: CloudCredential,
) -> Result<(Connection, Folder), &'static str> {
    let expected =
        account_type(settings.provider).ok_or("Select and configure a cloud provider first.")?;
    if credential.provider() != expected {
        return Err("Stored credentials do not match this configuration. Start a new sign-in.");
    }
    let root = settings.root_folder_id.trim();
    let (connection, id, dropbox_path) = match credential {
        CloudCredential::WebDav(password) => {
            if password.server.trim_end_matches('/') != settings.webdav_server.trim_end_matches('/')
            {
                return Err(
                    "Stored credentials do not match this configuration. Start a new sign-in.",
                );
            }
            let collection_url = dav_collection(settings, &password)?;
            let provider = WebDavProvider::new(WebDavConfig {
                collection_url,
                authentication: Authentication::Basic {
                    username: password.username,
                    password: password.password,
                },
            })
            .map_err(storage_failure)?;
            let id = provider.id("").map_err(storage_failure)?;
            (Connection::WebDav(provider), id, None)
        }
        CloudCredential::OAuth(token) => {
            if token.client_id != settings.client_id {
                return Err(
                    "Stored credentials do not match this configuration. Start a new sign-in.",
                );
            }
            let access_token = token.access_token;
            match settings.provider {
                CloudProvider::OneDrive => {
                    if settings.onedrive_drive_id.trim().is_empty() || root.is_empty() {
                        return Err(
                            "OneDrive requires the drive ID and app-folder item ID for your registration.",
                        );
                    }
                    (
                        Connection::Graph(
                            GraphProvider::new(GraphConfig {
                                api_url: "https://graph.microsoft.com/v1.0/".into(),
                                drive_id: settings.onedrive_drive_id.clone(),
                                access_token,
                            })
                            .map_err(storage_failure)?,
                        ),
                        StorageId(root.into()),
                        None,
                    )
                }
                CloudProvider::Dropbox => {
                    if !root.is_empty()
                        && (!root.starts_with('/')
                            || root.ends_with('/')
                            || root.split('/').skip(1).any(|s| !leaf_name(s)))
                    {
                        return Err(
                            "Dropbox's starting folder must be an absolute path or empty for the app root.",
                        );
                    }
                    (
                        Connection::Dropbox(
                            DropboxProvider::new(DropboxConfig {
                                api_url: "https://api.dropboxapi.com/2/".into(),
                                content_url: "https://content.dropboxapi.com/2/".into(),
                                access_token,
                            })
                            .map_err(storage_failure)?,
                        ),
                        StorageId(root.into()),
                        Some(root.into()),
                    )
                }
                CloudProvider::Box => (
                    Connection::Box(
                        BoxProvider::new(BoxConfig {
                            api_url: "https://api.box.com/2.0/".into(),
                            upload_url: "https://upload.box.com/api/2.0/".into(),
                            access_token,
                        })
                        .map_err(storage_failure)?,
                    ),
                    StorageId(if root.is_empty() { "0" } else { root }.into()),
                    None,
                ),
                CloudProvider::GoogleDrive => (
                    Connection::Drive(
                        GoogleDriveProvider::new(GoogleDriveConfig {
                            api_url: "https://www.googleapis.com/drive/v3/".into(),
                            upload_url: "https://www.googleapis.com/upload/drive/v3/".into(),
                            access_token,
                        })
                        .map_err(storage_failure)?,
                    ),
                    StorageId(if root.is_empty() { "root" } else { root }.into()),
                    None,
                ),
                _ => return Err("Select and configure a cloud provider first."),
            }
        }
    };
    Ok((
        connection,
        Folder {
            id,
            name: t("Root folder").into(),
            dropbox_path,
        },
    ))
}

async fn finish_connect(
    settings: CloudSettings,
    credential: CloudCredential,
    cancel: Cancellation,
    storage_cancel: CancelToken,
) -> Outcome {
    if cancel.is_cancelled() {
        return Err("Cloud sign-in cancelled.");
    }
    let provider = credential.provider();
    let (connection, folder) = if let CloudCredential::OAuth(token) = &credential
        && settings.provider == CloudProvider::OneDrive
    {
        if token.provider != CloudAccountType::OneDrive || token.client_id != settings.client_id {
            return Err("Stored credentials do not match this configuration. Start a new sign-in.");
        }
        let discovered = GraphProvider::discover(
            GraphDiscoveryConfig {
                api_url: "https://graph.microsoft.com/v1.0/".into(),
                access_token: token.access_token.clone(),
                location: GraphStartLocation::AppFolder,
            },
            storage_cancel,
        )
        .await
        .map_err(storage_failure)?;
        (
            Connection::Graph(discovered.provider),
            Folder {
                id: discovered.root,
                name: t("Root folder").into(),
                dropbox_path: None,
            },
        )
    } else {
        connect_from_credential(&settings, credential.clone())?
    };
    ProviderCredentialStore
        .write(&credential)
        .map_err(auth_failure)?;
    if cancel.is_cancelled() {
        let _ = ProviderCredentialStore.delete(provider);
        return Err("Cloud sign-in cancelled.");
    }
    Ok(Completion::Connected(connection, folder))
}

async fn sign_in(
    settings: CloudSettings,
    config: Option<EffectiveOAuthConfig>,
    username: String,
    password: String,
    stored: bool,
    flow: bool,
    cancellation: (CancelToken, Cancellation),
) -> Outcome {
    let (storage_cancel, cancel) = cancellation;
    let provider =
        account_type(settings.provider).ok_or("Select and configure a cloud provider first.")?;
    if cancel.is_cancelled() {
        return Err("Cloud sign-in cancelled.");
    }
    let client = AuthClient::new().map_err(auth_failure)?;
    let credential = if stored {
        let mut credential = ProviderCredentialStore
            .read(provider)
            .map_err(auth_failure)?
            .ok_or("No stored credentials. Start a new sign-in.")?;
        if let CloudCredential::OAuth(tokens) = &credential {
            let now = web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if tokens
                .expires_at
                .is_some_and(|expires| expires <= now.saturating_add(60))
            {
                let config = &config.as_ref().ok_or("Provider sign-in is not enabled in this build. The publisher must configure a registered OAuth application before you can connect.")?.oauth;
                credential = CloudCredential::OAuth(
                    client
                        .refresh(config, tokens, &cancel)
                        .await
                        .map_err(auth_failure)?,
                );
            }
        }
        credential
    } else if provider == CloudAccountType::WebDav {
        if flow {
            let login = client
                .nextcloud_start(&settings.webdav_server, &cancel)
                .await
                .map_err(auth_failure)?;
            #[cfg(not(target_arch = "wasm32"))]
            login.launch().map_err(auth_failure)?;
            #[cfg(target_arch = "wasm32")]
            thinoda_platform::system::open_url(login.login_url().as_str())
                .map_err(|_| "Could not open the cloud sign-in page.")?;
            CloudCredential::WebDav(
                client
                    .nextcloud_poll(
                        login,
                        Duration::from_secs(2),
                        Duration::from_secs(600),
                        &cancel,
                    )
                    .await
                    .map_err(auth_failure)?,
            )
        } else {
            CloudCredential::WebDav(AppPassword {
                server: settings.webdav_server.clone(),
                username,
                password,
            })
        }
    } else {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let config = &config.as_ref().ok_or("Provider sign-in is not enabled in this build. The publisher must configure a registered OAuth application before you can connect.")?.oauth;
            let login = cloud_auth::NativeLogin::bind(config)
                .await
                .map_err(auth_failure)?;
            login.launch(config).map_err(auth_failure)?;
            let grant = login
                .receive(config, Duration::from_secs(600), &cancel)
                .await
                .map_err(auth_failure)?;
            CloudCredential::OAuth(
                client
                    .exchange(config, grant, &cancel)
                    .await
                    .map_err(auth_failure)?,
            )
        }
        #[cfg(target_arch = "wasm32")]
        {
            return Err("Complete sign-in after the browser redirect.");
        }
    };
    finish_connect(settings, credential, cancel, storage_cancel).await
}

#[cfg(any(test, target_arch = "wasm32"))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CallbackAction {
    Ignore,
    ScrubOnly,
    Consume,
}

#[cfg(any(test, target_arch = "wasm32"))]
fn callback_action(query: &str, enabled: bool, inline: bool) -> CallbackAction {
    if !query
        .trim_start_matches('?')
        .split('&')
        .any(|s| s.starts_with("code=") || s.starts_with("error="))
    {
        CallbackAction::Ignore
    } else if enabled && !inline {
        CallbackAction::Consume
    } else {
        CallbackAction::ScrubOnly
    }
}

#[cfg(test)]
mod callback_tests {
    use super::*;

    #[test]
    fn callback_scrubbing_is_independent_of_integration_and_execution_mode() {
        for query in ["?code=stale&state=old", "?error=access_denied&state=old"] {
            assert_eq!(
                callback_action(query, false, false),
                CallbackAction::ScrubOnly
            );
            assert_eq!(
                callback_action(query, false, true),
                CallbackAction::ScrubOnly
            );
            assert_eq!(
                callback_action(query, true, true),
                CallbackAction::ScrubOnly
            );
            assert_eq!(callback_action(query, true, false), CallbackAction::Consume);
        }
        for query in ["", "?view=map", "?state=old", "?postcode=123"] {
            assert_eq!(callback_action(query, false, false), CallbackAction::Ignore);
            assert_eq!(callback_action(query, true, false), CallbackAction::Ignore);
        }
    }
}

impl ThinodaApp {
    fn cloud_upload_available(&self) -> bool {
        cfg!(feature = "integrated-cloud-services")
            && !self.file_ops.inline()
            && !self.file_ops.saving()
            && !self.cloud_program.busy()
            && !self.cloud_program.saving()
            && self.cloud_program.conflict.is_none()
            && self.doc().is_some()
            && self.cloud_program.folders.last().is_some_and(|folder| {
                self.cloud_program
                    .connection
                    .as_ref()
                    .is_some_and(|connection| connection.can_create_in(folder))
            })
    }

    /// Returns false: public preference persistence remains with Settings' existing
    /// gesture transaction. No credentials are written by drawing this section.
    pub(crate) fn cloud_settings_section(&mut self, ui: &mut Ui, activation_allowed: bool) -> bool {
        let allowed = activation_allowed
            && widgets::touch_activation_allowed(ui.ctx())
            && !self.file_ops.inline();
        #[cfg(not(target_arch = "wasm32"))]
        widgets::page_section(ui, t("Local backups"), |ui| {
            widgets::caption(
                ui,
                t(
                    "Choose a backup folder outside detected cloud sync folders. This does not change the provider's synchronization or version history.",
                ),
            );
            if let Some(folder) = &self.settings.cloud.backup_folder {
                widgets::caption(ui, folder.display().to_string());
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button(t("Choose backup folder…")).clicked()
                    && allowed
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    if backup_folder_allowed(&folder, &self.services.cloud) {
                        self.settings.cloud.backup_folder = Some(folder);
                    } else {
                        self.cloud_program.status = Some(
                            "Choose an existing folder outside every detected cloud sync folder.",
                        );
                    }
                }
                if ui
                    .add_enabled(
                        self.settings.cloud.backup_folder.is_some(),
                        egui::Button::new(t("Clear")),
                    )
                    .clicked()
                    && allowed
                {
                    self.settings.cloud.backup_folder = None;
                }
            });
        });
        #[cfg(target_arch = "wasm32")]
        widgets::page_section(ui, t("Browser document storage"), |ui| {
            widgets::caption(
                ui,
                t(
                    "Browser storage is private to this site. Check quota or request persistence explicitly; the browser may decline.",
                ),
            );
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !self.cloud_program.busy(),
                        egui::Button::new(t("Check storage usage")),
                    )
                    .clicked()
                    && allowed
                {
                    self.cloud_program.new_cancellation();
                    self.cloud_program
                        .queue("Checking browser storage…", async {
                            Ok(Completion::Estimate(
                                thinoda_platform::browser_storage::storage_estimate()
                                    .await
                                    .map_err(|_| "Browser storage information is unavailable.")?,
                            ))
                        });
                }
                if ui
                    .add_enabled(
                        !self.cloud_program.busy(),
                        egui::Button::new(t("Request persistent storage")),
                    )
                    .clicked()
                    && allowed
                {
                    self.cloud_program.new_cancellation();
                    self.cloud_program
                        .queue("Requesting persistent storage…", async {
                            Ok(Completion::Persistence(
                                thinoda_platform::browser_storage::request_persistence()
                                    .await
                                    .map_err(|_| "Browser storage persistence request failed.")?,
                            ))
                        });
                }
            });
            if let Some(estimate) = &self.cloud_program.estimate {
                for (label, amount) in [
                    ("Storage used (bytes)", estimate.usage),
                    ("Storage quota (bytes)", estimate.quota),
                ] {
                    widgets::caption(
                        ui,
                        format!(
                            "{}: {}",
                            t(label),
                            amount.map_or_else(|| t("Unavailable").into(), |n| n.to_string())
                        ),
                    );
                }
                widgets::caption(
                    ui,
                    t(match estimate.persisted {
                        Some(true) => "Browser storage persistence is granted.",
                        Some(false) => "Browser storage persistence is not granted.",
                        None => "Browser storage persistence status is unavailable.",
                    }),
                );
            }
        });
        let locked = self.cloud_program.busy() || self.cloud_program.connection.is_some();
        if !cfg!(feature = "integrated-cloud-services") {
            return false;
        }
        widgets::page_section(ui, t("Cloud accounts"), |ui| {
            widgets::caption(
                ui,
                t(
                    "Cloud accounts are optional. Selecting a provider or opening Settings never connects.",
                ),
            );
            ui.add_enabled_ui(!locked, |ui| {
                widgets::property_field(ui, t("Cloud provider"), |ui| {
                    egui::ComboBox::from_id_salt("cloud-provider")
                        .selected_text(t(provider_label(self.settings.cloud.provider)))
                        .show_ui(ui, |ui| {
                            for provider in [CloudProvider::None, CloudProvider::WebDav, CloudProvider::OneDrive, CloudProvider::Dropbox, CloudProvider::Box, CloudProvider::GoogleDrive] {
                                ui.selectable_value(&mut self.settings.cloud.provider, provider, t(provider_label(provider)));
                            }
                        }).response.accessible_name(t("Cloud provider"))
                });
                if self.settings.cloud.provider == CloudProvider::WebDav {
                    let server = self.cloud_program.server_draft.get_or_insert_with(|| self.settings.cloud.webdav_server.clone());
                    text_field(ui, "WebDAV server URL", server, false);
                    if public_url(server) || server.is_empty() {
                        self.settings.cloud.webdav_server = server.trim().into();
                    } else {
                        widgets::caption(ui, t("Use HTTPS without credentials, a query or a fragment."));
                    }
                    text_field(ui, "WebDAV collection prefix", &mut self.settings.cloud.webdav_collection, false);
                    widgets::caption(ui, t("Nextcloud default: remote.php/dav/files. Leave empty if the server URL is the complete DAV collection."));
                    widgets::toggle(ui, &mut self.cloud_program.app_password_login, t("Use an app password instead of browser sign-in"));
                    if self.cloud_program.app_password_login {
                        text_field(ui, "WebDAV username", &mut self.cloud_program.username, false);
                        text_field(ui, "WebDAV app password", &mut self.cloud_program.password, true);
                    }
                } else if self.settings.cloud.provider != CloudProvider::None {
                    widgets::caption(ui, t("Connect opens the selected provider’s sign-in page so you can authorize Thinoda. Account folders are chosen automatically; no registration IDs are needed."));
                    #[cfg(target_arch = "wasm32")]
                    widgets::caption(ui, t("Web sign-in redirects this tab. Save local work before connecting."));
                    if consumer_oauth_config(self.settings.cloud.provider).is_err() {
                        widgets::caption(ui, t("Provider sign-in is not enabled in this build. The publisher must configure a registered OAuth application before you can connect."));
                    }
                }
                widgets::toggle(ui, &mut self.cloud_program.use_stored, t("Use previously stored credentials"));
            });
            widgets::caption(
                ui,
                t(
                    "Connect contacts your configured provider and may open its sign-in page. Browsing sends folder identifiers; Open downloads content. Save and Upload send document content only when you request them.",
                ),
            );
            widgets::caption(
                ui,
                t(
                    "Credentials use the OS keychain on desktop and this tab's session storage on web. No cloud autosave or background reconnection.",
                ),
            );
            if let Some(doc) = self.doc()
                && let Some(source) = self.cloud_program.sources.get(&doc.key)
            {
                widgets::caption(ui, format!("{}: {}", t("Cloud document"), source.name));
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !locked && self.settings.cloud.provider != CloudProvider::None,
                        egui::Button::new(t("Connect")),
                    )
                    .clicked()
                    && allowed
                {
                    self.cloud_program.ask(Consent::Connect);
                }
                if ui
                    .add_enabled(
                        !self.cloud_program.busy() && self.cloud_program.connection.is_some(),
                        egui::Button::new(t("Disconnect")),
                    )
                    .clicked()
                    && allowed
                {
                    self.cloud_program.ask(Consent::Disconnect);
                }
                if ui
                    .add_enabled(
                        self.cloud_program.connection.is_some(),
                        egui::Button::new(t("Browse cloud files…")),
                    )
                    .clicked()
                    && allowed
                {
                    self.cloud_program.browser_open = true;
                    // Opening the browser window itself is also network-free.
                }
            });
            if let Some(status) = self.cloud_program.status {
                ui.label(egui::RichText::new(t(status)).color(Palette::of_ui(ui).text_muted));
            }
        });
        false
    }

    /// Explicit Save interceptor. None means a local/non-writable imported
    /// document; Some(false) means queued, blocked or failed, never "saved".
    /// Successful completion invokes resolve_save_close_tab with the actual
    /// same-state result. Save As must bypass this method.
    pub(crate) fn cloud_save_active(&mut self) -> Option<bool> {
        if !cfg!(feature = "integrated-cloud-services") {
            return None;
        }
        let doc = self.docs.get(self.active)?;
        if doc.path.is_some() {
            self.cloud_program.forget_document(&doc.key);
            return None;
        }
        let source = self.cloud_program.sources.get(&doc.key)?.clone();
        if !crate::doc::is_native(source.format) {
            return None;
        }
        if self.cloud_program.conflict_for(&doc.key) {
            return Some(false);
        }
        if self.file_ops.inline() || self.cloud_program.busy() || self.file_ops.saving() {
            return Some(false);
        }
        if source.session != self.cloud_program.session || self.cloud_program.connection.is_none() {
            self.cloud_program.status = Some(
                "This document's cloud connection was disconnected. Save As locally or upload a new copy; reconnecting does not retarget it.",
            );
            self.toasts
                .warning(t(self.cloud_program.status.unwrap_or("Not possible here")));
            return Some(false);
        }
        self.queue_cloud_save(source, None);
        Some(false)
    }

    fn queue_cloud_save(&mut self, source: Source, creating_in: Option<Folder>) {
        if !cfg!(feature = "integrated-cloud-services") {
            return;
        }
        let Some(connection) = self.cloud_program.connection.clone() else {
            return;
        };
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        if doc.view.try_commit_edit(&mut doc.doc).is_err() || doc.commit_inspector_edit().is_err() {
            self.cloud_program.status =
                Some("Could not commit document drafts. Your edits are unchanged.");
            return;
        }
        let mut save = Save {
            key: doc.key.clone(),
            state: doc.doc.state_id(),
            source,
            attach: doc.path.is_none(),
        };
        let workbook = doc.doc.workbook().clone();
        self.cloud_program.saving_key = Some(save.key.clone());
        let (cancel, _) = self.cloud_program.new_cancellation();
        self.cloud_program.queue("Uploading…", async move {
            let uploading_new = creating_in.is_some();
            cancel.check().map_err(storage_failure)?;
            let bytes =
                thinoda_io::export_bytes(&workbook, save.source.format, &ExportOptions::default())
                    .map_err(
                        |_| "Could not encode the cloud document. Your local edits are unchanged.",
                    )?;
            let result = if let Some(folder) = creating_in {
                let parent = connection.creation_parent(&folder)
                    .ok_or("This folder has no safe new-file upload path. Choose another folder.")?;
                match connection.create(&parent, &save.source.name, &bytes, cancel).await {
                    Ok(entry) => {
                        if entry.id.0.is_empty() || entry.is_dir || entry.name != save.source.name
                            || entry.revision.as_ref().is_none_or(|revision| revision.0.is_empty()) {
                            return Err("Cloud creation returned no stable identity or revision. The upload may have reached the server; reopen before saving again.");
                        }
                        save.source.id = entry.id;
                        Ok(entry.revision.expect("validated creation revision"))
                    }
                    Err(error) => Err(error),
                }
            } else {
                connection.storage().write(
                    &save.source.id, &bytes,
                    ExpectedRevision::Match(save.source.revision.clone()), cancel,
                ).await
            };
            match result {
                Ok(revision) => Ok(Completion::Saved(save, revision)),
                Err(StorageError::Conflict { .. }) => Ok(Completion::Conflict(Conflict {
                    save,
                    can_open_remote: !uploading_new,
                })),
                Err(error) => Err(storage_failure(error)),
            }
        });
    }

    fn cloud_list(&mut self, folder: Folder) {
        if !cfg!(feature = "integrated-cloud-services") {
            return;
        }
        if self.cloud_program.busy() || self.file_ops.inline() {
            return;
        }
        let Some(connection) = self.cloud_program.connection.clone() else {
            return;
        };
        let (cancel, _) = self.cloud_program.new_cancellation();
        self.cloud_program
            .queue("Listing cloud folder…", async move {
                let entries = connection
                    .storage()
                    .list(&folder.id, cancel)
                    .await
                    .map_err(storage_failure)?;
                Ok(Completion::Listed(folder, entries))
            });
    }

    fn cloud_open_selected(&mut self) {
        if !cfg!(feature = "integrated-cloud-services") {
            return;
        }
        if self.cloud_program.busy() || self.file_ops.inline() {
            return;
        }
        let Some(entry) = self
            .cloud_program
            .selected
            .clone()
            .filter(|entry| !entry.is_dir && leaf_name(&entry.name))
        else {
            return;
        };
        let Some(connection) = self.cloud_program.connection.clone() else {
            return;
        };
        let session = self.cloud_program.session;
        let viewport = self.current_viewport;
        let (cancel, _) = self.cloud_program.new_cancellation();
        self.cloud_program
            .queue("Downloading cloud document…", async move {
                let read = connection
                    .storage()
                    .read(&entry.id, cancel.clone())
                    .await
                    .map_err(storage_failure)?;
                cancel.check().map_err(storage_failure)?;
                let report = thinoda_io::import_named_bytes_with_report(
                    Path::new(&entry.name),
                    &read.bytes,
                )
                .map_err(
                    |_| "Could not import the cloud document. No existing document was changed.",
                )?;
                let source = Source {
                    session,
                    id: entry.id,
                    revision: read.revision,
                    name: entry.name,
                    format: report.format,
                };
                Ok(Completion::Opened(Opened {
                    report,
                    source,
                    viewport,
                }))
            });
    }

    fn cloud_upload_new(&mut self) {
        if !cfg!(feature = "integrated-cloud-services") {
            return;
        }
        if !self.cloud_upload_available() {
            return;
        }
        let Some(folder) = self.cloud_program.folders.last().cloned() else {
            return;
        };
        let name = self.cloud_program.upload_name.trim();
        if !leaf_name(name) || !name.to_ascii_lowercase().ends_with(".thinoda") {
            self.cloud_program.status = Some("Choose a single file name ending in .thinoda.");
            return;
        }
        let source = Source {
            session: self.cloud_program.session,
            // This is not a guessed file ID. The adapter returns the actual
            // created identity; a conflicting create never installs this source.
            id: folder.id.clone(),
            revision: Revision(String::new()),
            name: name.into(),
            format: Format::Thinoda,
        };
        self.queue_cloud_save(source, Some(folder));
    }

    fn cloud_install(&mut self, opened: Opened) {
        if opened.source.session != self.cloud_program.session
            || self.cloud_program.connection.is_none()
        {
            self.cloud_program.status =
                Some("Cloud connection changed. Downloaded content was not installed.");
            return;
        }
        // Never pass a provider ID/name to open_path/from_import_report: those
        // functions fingerprint paths and create local-file/autosave identities.
        let old_viewport = self.current_viewport;
        self.current_viewport = opened.viewport;
        self.new_doc(opened.report.workbook);
        self.current_viewport = old_viewport;
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.format = opened.source.format;
            if !crate::doc::is_native(doc.format) || !opened.report.warnings.is_empty() {
                doc.doc.mark_dirty();
            }
            self.cloud_program
                .sources
                .insert(doc.key.clone(), opened.source);
        }
        self.cloud_program.browser_open = false;
    }

    fn cloud_accept_consent(&mut self, consent: Consent) {
        if !cfg!(feature = "integrated-cloud-services")
            || self.file_ops.inline()
            || self.cloud_program.busy()
        {
            return;
        }
        match consent {
            Consent::Connect => {
                let config = if self.settings.cloud.provider != CloudProvider::WebDav {
                    match consumer_oauth_config(self.settings.cloud.provider) {
                        Ok(config) => Some(config),
                        Err(message) => {
                            self.cloud_program.status = Some(message);
                            return;
                        }
                    }
                } else {
                    None
                };
                let settings = consumer_settings(&self.settings.cloud, config.as_ref());
                if account_type(settings.provider).is_none() {
                    self.cloud_program.status =
                        Some("Select and configure a cloud provider first.");
                    return;
                }
                if settings.provider == CloudProvider::WebDav
                    && !public_url(&settings.webdav_server)
                {
                    self.cloud_program.status =
                        Some("Use HTTPS without credentials, a query or a fragment.");
                    return;
                }
                #[cfg(target_arch = "wasm32")]
                if settings.provider != CloudProvider::WebDav && !self.cloud_program.use_stored {
                    if self.docs.iter().any(OpenDoc::is_dirty) {
                        self.cloud_program.status =
                            Some("Save local work before redirecting this tab to sign in.");
                        return;
                    }
                    let result = config
                        .as_ref()
                        .ok_or(cloud_auth::AuthError::BuildConfiguration)
                        .and_then(|config| {
                            cloud_auth::web::begin(&config.oauth, &settings.redirect_uri)
                        })
                        .and_then(|url| {
                            web_sys::window()
                                .ok_or(cloud_auth::AuthError::Configuration)?
                                .location()
                                .set_href(url.as_str())
                                .map_err(|_| cloud_auth::AuthError::Transport)
                        });
                    if let Err(error) = result {
                        self.cloud_program.status = Some(auth_failure(error))
                    }
                    return;
                }
                let username = std::mem::take(&mut self.cloud_program.username);
                let password = std::mem::take(&mut self.cloud_program.password);
                let stored = self.cloud_program.use_stored;
                let flow = !self.cloud_program.app_password_login;
                let (storage_cancel, cancel) = self.cloud_program.new_cancellation();
                self.cloud_program.queue(
                    "Connecting cloud account…",
                    sign_in(
                        settings,
                        config,
                        username,
                        password,
                        stored,
                        flow,
                        (storage_cancel, cancel),
                    ),
                );
            }
            Consent::Disconnect => {
                let Some(provider) = account_type(self.settings.cloud.provider) else {
                    return;
                };
                self.cloud_program.detach();
                self.cloud_program.new_cancellation();
                self.cloud_program
                    .queue("Disconnecting cloud account…", async move {
                        ProviderCredentialStore
                            .delete(provider)
                            .map_err(auth_failure)?;
                        #[cfg(target_arch = "wasm32")]
                        cloud_auth::web::cancel_pending(provider).map_err(auth_failure)?;
                        Ok(Completion::Disconnected)
                    });
            }
            Consent::Upload => self.cloud_upload_new(),
            Consent::Share => {
                let Some(entry) = self.cloud_program.selected.clone().filter(|e| !e.is_dir) else {
                    return;
                };
                let Some(connection) = self.cloud_program.connection.clone() else {
                    return;
                };
                let (cancel, _) = self.cloud_program.new_cancellation();
                self.cloud_program
                    .queue("Creating restricted share link…", async move {
                        Ok(Completion::Shared(
                            connection
                                .share(&entry.id, cancel)
                                .await
                                .map_err(storage_failure)?,
                        ))
                    });
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn cloud_resume_sign_in(&mut self, grant: cloud_auth::AuthorizationGrant) {
        if !cfg!(feature = "integrated-cloud-services")
            || self.file_ops.inline()
            || self.cloud_program.busy()
        {
            return;
        }
        let config = match consumer_oauth_config(self.settings.cloud.provider) {
            Ok(config) => config,
            Err(message) => {
                self.cloud_program.status = Some(message);
                return;
            }
        };
        let settings = consumer_settings(&self.settings.cloud, Some(&config));
        let (storage_cancel, cancel) = self.cloud_program.new_cancellation();
        self.cloud_program
            .queue("Connecting cloud account…", async move {
                let tokens = AuthClient::new()
                    .map_err(auth_failure)?
                    .exchange(&config.oauth, grant, &cancel)
                    .await
                    .map_err(auth_failure)?;
                finish_connect(
                    settings,
                    CloudCredential::OAuth(tokens),
                    cancel,
                    storage_cancel,
                )
                .await
            });
    }

    /// Web startup hook, after interactive execution is enabled and before UI
    /// routing. Only a callback validated against the user's one-time pending
    /// PKCE transaction continues the already consented sign-in. Unsolicited,
    /// expired and mismatched callbacks are scrubbed without token exchange.
    pub(crate) fn cloud_consume_redirect(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            let callback = web_sys::window()
                .and_then(|w| w.location().search().ok())
                .unwrap_or_default();
            let action = callback_action(
                &callback,
                cfg!(feature = "integrated-cloud-services"),
                self.file_ops.inline(),
            );
            if action == CallbackAction::Ignore {
                return;
            }
            // Consumption needs the original URL, so validate before scrubbing.
            // Disabled/headless builds must not touch PKCE storage or account APIs.
            let result = (action == CallbackAction::Consume).then(|| {
                consumer_oauth_config(self.settings.cloud.provider).and_then(|config| {
                    cloud_auth::web::consume_redirect(&config.oauth).map_err(auth_failure)
                })
            });
            // Even a missing/invalid public registration must not leave an
            // authorization code visible in the address bar.
            if let Some(window) = web_sys::window()
                && let Ok(url) = window.location().href()
                && let Some(end) = url.find(['?', '#'])
                && let Ok(history) = window.history()
            {
                let _ = history.replace_state_with_url(
                    &wasm_bindgen::JsValue::NULL,
                    "",
                    Some(&url[..end]),
                );
            }
            match result {
                Some(Ok(grant)) => {
                    self.cloud_resume_sign_in(grant);
                }
                Some(Err(message)) => self.cloud_program.status = Some(message),
                None => {}
            }
        }
    }

    pub(crate) fn poll_cloud_program(&mut self, ctx: &Context) {
        self.cloud_program
            .sources
            .retain(|key, _| self.docs.iter().any(|d| &d.key == key && d.path.is_none()));
        if self.file_ops.inline() {
            return;
        }
        if let Some(job) = self.cloud_program.pending.take() {
            let (tx, rx) = mpsc::channel();
            let repaint = ctx.clone();
            #[cfg(not(target_arch = "wasm32"))]
            {
                let spawned = std::thread::Builder::new()
                    .name("thinoda-cloud".into())
                    .spawn(move || {
                        let outcome = match tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                        {
                            Ok(runtime) => runtime.block_on(job),
                            Err(_) => Err("Cloud worker could not start. No request was made."),
                        };
                        let _ = tx.send(outcome);
                        repaint.request_repaint_of(ViewportId::ROOT);
                    });
                if spawned.is_err() {
                    self.cloud_program.status =
                        Some("Cloud worker could not start. No request was made.");
                    self.cloud_program.busy_label = None;
                    if let Some(key) = self.cloud_program.saving_key.take()
                        && !self.resolve_save_all(ctx, &key, false)
                    {
                        self.resolve_save_close_tab(&key, false);
                    }
                    return;
                }
            }
            #[cfg(target_arch = "wasm32")]
            wasm_bindgen_futures::spawn_local(async move {
                let _ = tx.send(job.await);
                repaint.request_repaint_of(ViewportId::ROOT);
            });
            self.cloud_program.running = Some(rx);
        }
        let result = self
            .cloud_program
            .running
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Cloud worker stopped. Your local edits are unchanged."))
                }
            });
        let Some(result) = result else {
            if self.cloud_program.busy() {
                ctx.request_repaint_after(Duration::from_millis(50))
            }
            return;
        };
        self.cloud_program.running = None;
        self.cloud_program.busy_label = None;
        let saving_key = self.cloud_program.saving_key.take();
        match result {
            Err(message) => {
                self.cloud_program.status = Some(message);
                self.toasts.warning(t(message));
                if let Some(key) = saving_key
                    && !self.resolve_save_all(ctx, &key, false)
                {
                    self.resolve_save_close_tab(&key, false);
                }
            }
            Ok(Completion::Connected(connection, folder)) => {
                self.cloud_program.session = self.cloud_program.session.wrapping_add(1);
                self.cloud_program.connection = Some(Arc::new(connection));
                self.cloud_program.folders = vec![folder];
                self.cloud_program.entries.clear();
                self.cloud_program.status = Some(
                    "Connected. Choose List folder to browse; no files are downloaded automatically.",
                );
            }
            Ok(Completion::Disconnected) => {
                self.cloud_program.status = Some(
                    "Disconnected locally. Provider grants and remote files were not deleted; revoke access in the provider's account settings.",
                )
            }
            Ok(Completion::Listed(folder, mut entries)) => {
                entries.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                if let Some(index) = self
                    .cloud_program
                    .folders
                    .iter()
                    .position(|old| old.id == folder.id)
                {
                    self.cloud_program.folders.truncate(index + 1);
                } else {
                    self.cloud_program.folders.push(folder);
                }
                self.cloud_program.entries = entries;
                self.cloud_program.selected = None;
                self.cloud_program.versions.clear();
                self.cloud_program.share_link = None;
                self.cloud_program.status = Some("Cloud folder listed.");
            }
            Ok(Completion::Opened(opened)) => {
                if opened.report.warnings.is_empty() {
                    self.cloud_install(opened)
                } else {
                    self.cloud_program.import = Some(opened)
                }
            }
            Ok(Completion::Saved(save, revision)) => self.cloud_finish_save(ctx, save, revision),
            Ok(Completion::Conflict(conflict)) => {
                self.cloud_program.status = Some(
                    "Cloud file changed. No overwrite was attempted; open the remote file separately or upload a new copy.",
                );
                self.cloud_program.conflict = Some(conflict);
                // Retain Save All/tab-close ownership until an explicit choice.
                // Do not reuse self.dialogs.conflict, which expects local paths.
            }
            Ok(Completion::Versions(versions)) => {
                self.cloud_program.versions = versions;
                self.cloud_program.status =
                    Some("Version metadata loaded. No version content was downloaded.");
            }
            Ok(Completion::Shared(link)) => {
                self.cloud_program.share_link = Some(link);
                self.cloud_program.status = Some(
                    "Restricted share link created. It was not opened or copied automatically.",
                );
            }
            #[cfg(target_arch = "wasm32")]
            Ok(Completion::Estimate(estimate)) => self.cloud_program.estimate = Some(estimate),
            #[cfg(target_arch = "wasm32")]
            Ok(Completion::Persistence(granted)) => {
                if let Some(estimate) = &mut self.cloud_program.estimate {
                    estimate.persisted = Some(granted);
                }
                self.cloud_program.status = Some(if granted {
                    "Browser storage persistence is granted."
                } else {
                    "Browser storage persistence is not granted."
                });
            }
        }
    }

    fn cloud_finish_save(&mut self, ctx: &Context, mut save: Save, revision: Revision) {
        if save.source.session != self.cloud_program.session
            || self.cloud_program.connection.is_none()
        {
            return;
        }
        save.source.revision = revision;
        let mut saved = false;
        if let Some(doc) = self.docs.iter_mut().find(|d| d.key == save.key)
            && save.attach
            && doc.path.is_none()
        {
            saved = mark_same_state_saved(doc, save.state);
            self.cloud_program
                .sources
                .insert(save.key.clone(), save.source);
        }
        self.cloud_program.status = Some(if saved {
            "Cloud document saved."
        } else {
            "Cloud snapshot uploaded. Newer edits or the local source still need saving."
        });
        if !self.resolve_save_all(ctx, &save.key, saved) {
            self.resolve_save_close_tab(&save.key, saved);
        }
    }

    pub(crate) fn show_cloud_dialogs(&mut self, ctx: &Context) {
        let allowed = widgets::touch_activation_allowed(ctx) && !self.file_ops.inline();
        if let Some(conflict) = &self.cloud_program.conflict {
            let can_open = conflict.can_open_remote
                && conflict.save.source.session == self.cloud_program.session
                && self.cloud_program.connection.is_some();
            let name = conflict.save.source.name.clone();
            let mut decision = None;
            let response = Dialog::new(egui::Id::unique("cloud-save-conflict"), t("Cloud save conflict"))
                .icon(icons::CLOUD).labels(&i18n::dialog_labels()).modal(true)
                .default_width(520.0).show(ctx, |ui| {
                    ui.label(&name);
                    ui.label(t("The cloud revision changed or the new file name already exists. Your edits remain open. No overwrite or retry will be attempted. Cancel this save, then use Save As locally or upload a new copy with a different name."));
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(can_open, egui::Button::new(t("Open remote copy"))).clicked() && allowed {
                            decision = Some(true);
                        }
                        if ui.button(t("Cancel")).clicked() && allowed {
                            decision = Some(false);
                        }
                    });
                });
            if response.close_requested && allowed {
                decision = Some(false)
            }
            if let Some(open_remote) = decision {
                self.cloud_resolve_conflict(ctx, open_remote);
            }
            return;
        }
        if let Some(consent) = self.cloud_program.consent {
            let (title, message, action) = match consent {
                Consent::Connect => (
                    "Connect cloud account",
                    "I authorize Thinoda to contact this provider, open sign-in if needed, and store credentials securely. This does not browse, upload or share any document.",
                    "Connect",
                ),
                Consent::Disconnect => (
                    "Disconnect cloud account",
                    "Remove this connection and its stored credentials on this device/tab. Documents and unsaved edits remain open. Remote files and provider grants are not deleted.",
                    "Disconnect",
                ),
                Consent::Upload => (
                    "Upload current document",
                    "Upload a snapshot of the current document to the chosen folder as a new .thinoda file. Existing files are never overwritten. Local documents remain local; unsaved documents gain an explicit cloud Save destination.",
                    "Upload",
                ),
                Consent::Share => (
                    "Create restricted share link",
                    "This changes sharing on the selected remote file. Box uses existing collaborators; OneDrive uses organization-only view access. No anonymous/public link is requested. The provider may reject this permission.",
                    "Create share link",
                ),
            };
            let mut decision = None;
            let response = Dialog::new(egui::Id::unique("cloud-consent"), t(title))
                .icon(icons::CLOUD)
                .labels(&i18n::dialog_labels())
                .modal(true)
                .default_width(500.0)
                .show(ctx, |ui| {
                    ui.label(t(provider_label(self.settings.cloud.provider)));
                    ui.label(t(message));
                    if matches!(consent, Consent::Connect) && self.settings.cloud.provider == CloudProvider::OneDrive {
                        ui.label(t("OneDrive connection locates the app folder and may create it. No document is transferred, and access is not broadened if discovery fails."));
                    }
                    if matches!(consent, Consent::Upload) {
                        text_field(
                            ui,
                            "Cloud file name",
                            &mut self.cloud_program.upload_name,
                            false,
                        );
                        if let Some(folder) = self.cloud_program.folders.last() {
                            ui.label(format!("{}: {}", t("Folder"), folder.name));
                            widgets::caption(ui, format!("{}: {}", t("File ID"), folder.id.0));
                        }
                        if let Some(warning) = self
                            .cloud_program
                            .connection
                            .as_ref()
                            .and_then(|connection| connection.upload_warning())
                        {
                            ui.label(egui::RichText::new(t(warning)).strong());
                        }
                    }
                    let before = self.cloud_program.consented;
                    widgets::toggle(
                        ui,
                        &mut self.cloud_program.consented,
                        t("I agree to this action"),
                    );
                    if !allowed {
                        self.cloud_program.consented = before
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                self.cloud_program.consented && allowed,
                                egui::Button::new(t(action)),
                            )
                            .clicked()
                            && allowed
                        {
                            decision = Some(true)
                        }
                        if ui.button(t("Cancel")).clicked() && allowed {
                            decision = Some(false)
                        }
                    });
                });
            if response.close_requested && allowed {
                decision = Some(false)
            }
            if let Some(accept) = decision {
                self.cloud_program.consent = None;
                self.cloud_program.consented = false;
                if accept {
                    self.save_settings();
                    self.cloud_accept_consent(consent)
                } else {
                    self.cloud_program.password.clear();
                }
            }
            return;
        }
        if self.cloud_program.import.is_some() {
            let mut accept = None;
            let response = Dialog::new(
                egui::Id::unique("cloud-import"),
                t("Import requires confirmation"),
            )
            .icon(icons::CLOUD)
            .labels(&i18n::dialog_labels())
            .modal(true)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.label(t(
                    "Unsupported source content may be omitted. The cloud original is not changed.",
                ));
                if let Some(opened) = &self.cloud_program.import {
                    for warning in &opened.report.warnings {
                        ui.label(t(i18n::import_warning_label(*warning)));
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button(t("Open")).clicked() && allowed {
                        accept = Some(true)
                    }
                    if ui.button(t("Cancel")).clicked() && allowed {
                        accept = Some(false)
                    }
                });
            });
            if response.close_requested && allowed {
                accept = Some(false)
            }
            if let Some(accept) = accept
                && let Some(opened) = self.cloud_program.import.take()
                && accept
            {
                self.cloud_install(opened);
            }
            return;
        }
        if self.cloud_program.browser_open {
            let mut close = false;
            let response = Dialog::new(egui::Id::unique("cloud-browser"), t("Cloud files"))
                .icon(icons::CLOUD).labels(&i18n::dialog_labels()).modal(true)
                .default_width(680.0).default_height(500.0).show(ctx, |ui| {
                    let idle = !self.cloud_program.busy();
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(idle, egui::Button::new(t("List folder"))).clicked() && allowed
                            && let Some(folder) = self.cloud_program.folders.last().cloned() { self.cloud_list(folder) }
                        if ui.add_enabled(idle && self.cloud_program.folders.len() > 1, egui::Button::new(t("Parent folder"))).clicked() && allowed {
                            let parent = self.cloud_program.folders.len() - 2;
                            if let Some(folder) = self.cloud_program.folders.get(parent).cloned() { self.cloud_list(folder) }
                        }
                        if let Some(folder) = self.cloud_program.folders.last() { ui.label(&folder.name); }
                    });
                    egui::ScrollArea::both().id_salt("cloud-entries").max_height(240.0).show(ui, |ui| {
                        for entry in self.cloud_program.entries.clone() {
                            let selected = self.cloud_program.selected.as_ref().is_some_and(|old| old.id == entry.id);
                            let label = format!("{} {}", if entry.is_dir { icons::OPEN } else { icons::NEW_FILE }, entry.name);
                            if ui.add_enabled(idle, egui::Button::new(label).selected(selected)).clicked() && allowed {
                                self.cloud_program.selected = Some(entry);
                                self.cloud_program.versions.clear();
                                self.cloud_program.share_link = None;
                            }
                        }
                    });
                    if let Some(entry) = self.cloud_program.selected.clone() {
                        widgets::caption(ui, format!("{}: {}", t("File ID"), entry.id.0));
                        widgets::caption(ui, format!("{}: {}", t("Revision"), entry.revision.as_ref().map_or(t("Unavailable"), |r| r.0.as_str())));
                        let caps = self.cloud_program.connection.as_ref().map(|p| p.storage().capabilities()).unwrap_or_default();
                        ui.horizontal_wrapped(|ui| {
                            if ui.add_enabled(idle, egui::Button::new(t("Open"))).clicked() && allowed {
                                if entry.is_dir {
                                    let path = self.cloud_program.folders.last().and_then(|f| f.dropbox_path.as_ref())
                                        .filter(|_| leaf_name(&entry.name)).map(|p| format!("{p}/{}", entry.name));
                                    self.cloud_list(Folder { id: entry.id.clone(), name: entry.name.clone(), dropbox_path: path });
                                } else { self.cloud_open_selected() }
                            }
                            if ui.add_enabled(idle && !entry.is_dir && caps.versions, egui::Button::new(t("Versions"))).clicked() && allowed
                                && let Some(connection) = self.cloud_program.connection.clone() {
                                let id = entry.id.clone();
                                let (cancel, _) = self.cloud_program.new_cancellation();
                                self.cloud_program.queue("Loading version metadata…", async move {
                                    Ok(Completion::Versions(connection.versions(&id, cancel).await.map_err(storage_failure)?))
                                });
                            }
                            if ui.add_enabled(idle && !entry.is_dir && caps.share_links, egui::Button::new(t("Create share link"))).clicked() && allowed {
                                self.cloud_program.ask(Consent::Share);
                            }
                        });
                        if !caps.versions || !caps.share_links {
                            widgets::caption(ui, t("Versions and sharing are unavailable where the adapter does not implement them."));
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(self.cloud_upload_available(), egui::Button::new(t("Upload current document…"))).clicked() && allowed {
                            self.cloud_program.upload_name = format!("{}.thinoda", crate::app::sanitize(&self.doc().map(OpenDoc::title).unwrap_or_default()));
                            self.cloud_program.ask(Consent::Upload);
                        }
                        if ui.button(t("Close")).clicked() && allowed { close = true }
                    });
                    if matches!(self.cloud_program.connection.as_deref(), Some(Connection::Drive(_))) {
                        widgets::caption(ui, t("Google Drive allows duplicate file names. This upload creates a distinct new file in the selected folder, even if the same name already exists. Existing files are not replaced. Later Save targets only the returned file ID."));
                    }
                    egui::ScrollArea::both().id_salt("cloud-version-results").max_height(100.0).show(ui, |ui| {
                        for version in &self.cloud_program.versions {
                            ui.label(format!("{}: {}  {}", t("Version"), version.id, version.modified.as_deref().unwrap_or("")));
                        }
                    });
                    if let Some(link) = &self.cloud_program.share_link {
                        ui.label(t("Restricted share link"));
                        // Do not create an automatic hyperlink/open-url action.
                        let mut text = link.0.as_str();
                        ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY))
                            .accessible_name(t("Restricted share link"));
                        if ui.button(t("Copy link")).clicked() && allowed { ctx.copy_text(link.0.clone()) }
                    }
                    if let Some(status) = self.cloud_program.status { widgets::caption(ui, t(status)); }
                });
            if (response.close_requested || close) && allowed {
                self.cloud_program.browser_open = false
            }
        }
        // Nonmodal progress permits newer edits while a snapshot uploads.
        if self.cloud_program.busy() {
            egui::Window::new(t("Cloud operation")).id(egui::Id::unique("cloud-progress"))
                .collapsible(false).resizable(false).show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t(self.cloud_program.busy_label.unwrap_or("Cloud operation")));
                    });
                    let cancellable = !matches!(self.cloud_program.busy_label,
                        Some("Checking browser storage…" | "Requesting persistent storage…" | "Disconnecting cloud account…"));
                    if ui.add_enabled(cancellable, egui::Button::new(t("Cancel"))).clicked() && allowed {
                        self.cloud_program.cancel.cancel();
                        self.cloud_program.auth_cancel.cancel();
                    }
                    widgets::caption(ui, t("Progress is indeterminate. Cancel cannot undo an upload already accepted by the provider."));
                });
        }
    }
    fn cloud_resolve_conflict(&mut self, ctx: &Context, open_remote: bool) {
        let Some(conflict) = self.cloud_program.conflict.take() else {
            return;
        };
        let key = conflict.save.key.clone();
        // Clear modal ownership before resolving so the batch/close attempt
        // stops rather than waiting forever. Neither action reports save success.
        if !self.resolve_save_all(ctx, &key, false) {
            self.resolve_save_close_tab(&key, false);
        }
        if open_remote
            && conflict.can_open_remote
            && conflict.save.source.session == self.cloud_program.session
            && self.cloud_program.connection.is_some()
        {
            self.cloud_program.selected = Some(Entry {
                id: conflict.save.source.id,
                name: conflict.save.source.name,
                size: 0,
                revision: None,
                availability: thinoda_platform::storage::Availability::OnlineOnly,
                is_dir: false,
            });
            // Explicitly chosen read opens a separate document; never reload or
            // modify the conflicted original or its old expected revision.
            self.cloud_open_selected();
        }
    }
}

fn mark_same_state_saved(doc: &mut OpenDoc, state: u64) -> bool {
    if doc.doc.state_id() == state
        && !doc.has_inspector_changes()
        && !doc.view.has_title_changes()
        && doc.fim_edit.is_none()
    {
        doc.doc.mark_saved();
        true
    } else {
        false
    }
}

/// Revalidate persisted preferences before passing the directory to the local
/// storage adapter, too. Canonical paths reject symlink/junction aliases into a
/// sync root; unavailable roots fail closed rather than accepting an unsafe path.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn backup_folder_allowed(
    candidate: &Path,
    roots: &[thinoda_platform::cloud::CloudFolder],
) -> bool {
    let Ok(candidate) = candidate.canonicalize() else {
        return false;
    };
    if !candidate.is_dir() {
        return false;
    }
    for root in roots {
        let Ok(root) = root.path.canonicalize() else {
            return false;
        };
        #[cfg(windows)]
        let beneath = {
            let candidate: Vec<_> = candidate
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
                .collect();
            let root: Vec<_> = root
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
                .collect();
            candidate.starts_with(&root)
        };
        #[cfg(not(windows))]
        let beneath = candidate.starts_with(&root);
        if beneath {
            return false;
        }
    }
    true
}

/// Native backup directories have no meaning in browser storage. This keeps
/// shared settings validation fail-closed without touching the browser VFS.
#[cfg(target_arch = "wasm32")]
pub(crate) fn backup_folder_allowed(
    _candidate: &Path,
    _roots: &[thinoda_platform::cloud::CloudFolder],
) -> bool {
    false
}

fn text_field(ui: &mut Ui, key: &'static str, value: &mut String, password: bool) {
    widgets::property_field(ui, t(key), |ui| {
        ui.add(
            egui::TextEdit::singleline(value)
                .password(password)
                .desired_width(260.0),
        )
        .accessible_name(t(key))
    });
}

#[cfg(all(test, not(feature = "integrated-cloud-services")))]
mod disabled_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn default_build_hides_accounts_but_preserves_local_storage_controls() {
        let mut app = ThinodaApp::headless();
        app.settings.cloud.provider = CloudProvider::OneDrive;
        let mut harness = Harness::builder().build_ui_state(
            |ui, app: &mut ThinodaApp| {
                app.cloud_settings_section(ui, true);
            },
            app,
        );
        harness.run_steps(2);
        assert!(harness.query_by_label(t("Cloud accounts")).is_none());
        assert!(harness.query_by_label(t("Connect")).is_none());
        #[cfg(not(target_arch = "wasm32"))]
        assert!(harness.query_by_label(t("Choose backup folder…")).is_some());
    }

    #[test]
    fn disabled_integration_cannot_start_jobs_from_saved_provider_or_consent() {
        let mut app = ThinodaApp::headless();
        for provider in [
            CloudProvider::WebDav,
            CloudProvider::OneDrive,
            CloudProvider::Dropbox,
            CloudProvider::Box,
            CloudProvider::GoogleDrive,
        ] {
            app.settings.cloud.provider = provider;
            app.settings.cloud.webdav_server = "https://example.invalid/".into();
            app.cloud_program.use_stored = true;
            for consent in [
                Consent::Connect,
                Consent::Disconnect,
                Consent::Upload,
                Consent::Share,
            ] {
                app.cloud_accept_consent(consent);
                assert!(app.cloud_program.pending.is_none());
                assert!(app.cloud_program.running.is_none());
                assert!(app.cloud_program.connection.is_none());
            }
            app.cloud_consume_redirect();
            app.cloud_list(Folder {
                id: StorageId("root".into()),
                name: "root".into(),
                dropbox_path: None,
            });
            app.cloud_open_selected();
            app.cloud_upload_new();
            assert!(!app.cloud_upload_available());
            assert!(app.cloud_save_active().is_none());
            assert!(app.cloud_program.pending.is_none());
            assert!(app.cloud_program.status.is_none());
        }
    }
}

#[cfg(all(test, feature = "integrated-cloud-services"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use thinoda_core::{Document, Sheet, Workbook};
    use thinoda_platform::storage::{Capabilities, StorageFuture};

    struct Mock(Arc<AtomicUsize>);
    impl StorageProvider for Mock {
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }
        fn list<'a>(&'a self, _: &'a StorageId, _: CancelToken) -> StorageFuture<'a, Vec<Entry>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(Vec::new()) })
        }
        fn read<'a>(
            &'a self,
            _: &'a StorageId,
            _: CancelToken,
        ) -> StorageFuture<'a, thinoda_platform::storage::ReadResult> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Err(StorageError::Unsupported) })
        }
        fn write<'a>(
            &'a self,
            _: &'a StorageId,
            _: &'a [u8],
            expected: ExpectedRevision,
            _: CancelToken,
        ) -> StorageFuture<'a, Revision> {
            self.0.fetch_add(1, Ordering::SeqCst);
            assert!(matches!(
                expected,
                ExpectedRevision::Match(_) | ExpectedRevision::Missing
            ));
            Box::pin(async { Ok(Revision("new".into())) })
        }
    }

    fn doc() -> OpenDoc {
        OpenDoc::new(Workbook::new(Sheet::new("Sheet", "Original")))
    }

    fn configured_connection(provider: CloudProvider) -> (Connection, Folder) {
        let settings = CloudSettings {
            provider,
            client_id: "public-test-registration".into(),
            webdav_server: "https://cloud.example.test/".into(),
            onedrive_drive_id: "test-drive".into(),
            root_folder_id: match provider {
                CloudProvider::Dropbox => "/chosen-folder".into(),
                CloudProvider::OneDrive => "app-folder".into(),
                _ => String::new(),
            },
            ..Default::default()
        };
        let credential = if provider == CloudProvider::WebDav {
            CloudCredential::WebDav(AppPassword {
                server: settings.webdav_server.clone(),
                username: "synthetic-test-account".into(),
                password: "synthetic-test-password".into(),
            })
        } else {
            CloudCredential::OAuth(cloud_auth::TokenSet {
                provider: account_type(provider).unwrap(),
                client_id: settings.client_id.clone(),
                access_token: "synthetic-test-token".into(),
                refresh_token: None,
                expires_at: None,
                scope: None,
            })
        };
        connect_from_credential(&settings, credential).unwrap()
    }

    #[test]
    fn new_upload_command_availability_matches_each_providers_safe_creation_semantics() {
        for (provider, available) in [
            (CloudProvider::WebDav, true),
            (CloudProvider::OneDrive, true),
            (CloudProvider::Dropbox, true),
            (CloudProvider::Box, true),
            (CloudProvider::GoogleDrive, true),
        ] {
            let mut app = ThinodaApp::headless();
            app.set_file_work_mode(crate::file_ops::FileWorkMode::Background);
            let (connection, folder) = configured_connection(provider);
            app.cloud_program.connection = Some(Arc::new(connection));
            app.cloud_program.folders = vec![folder];
            assert_eq!(app.cloud_upload_available(), available, "{provider:?}");
            assert!(
                !app.cloud_program.busy(),
                "checking availability cannot start a request"
            );
        }
    }

    #[test]
    fn new_upload_cannot_guess_dropbox_parent_paths_or_allow_empty_graph_box_folders() {
        for provider in [
            CloudProvider::Dropbox,
            CloudProvider::OneDrive,
            CloudProvider::Box,
            CloudProvider::GoogleDrive,
        ] {
            let (connection, mut folder) = configured_connection(provider);
            if provider == CloudProvider::Dropbox {
                folder.dropbox_path = None;
            } else {
                folder.id.0.clear();
            }
            assert!(!connection.can_create_in(&folder));
        }
    }

    #[test]
    fn dropbox_new_upload_prefers_listed_folder_id_over_a_reconstructed_path() {
        let (connection, mut folder) = configured_connection(CloudProvider::Dropbox);
        folder.id = StorageId("id:listed-folder".into());
        folder.dropbox_path = Some("/not-the-listed-folder".into());
        assert_eq!(
            connection.creation_parent(&folder),
            Some(StorageId("id:listed-folder".into()))
        );
        folder.dropbox_path = None;
        assert!(connection.can_create_in(&folder));
    }

    #[test]
    fn cloud_drawio_detachment_requires_cancelable_confirmation() {
        use egui_kittest::{Harness, kittest::Queryable};

        let xml = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="shape" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
            r#"<mxCell id="edge" edge="1" parent="1" source="missing" target="shape" style="endArrow=none;"><mxGeometry relative="1" as="geometry"><mxPoint x="7" y="8" as="sourcePoint"/></mxGeometry></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        let report = thinoda_io::import_bytes_with_report(Format::DrawIo, xml.as_bytes()).unwrap();
        assert!(
            report
                .warnings
                .contains(&thinoda_io::ImportWarning::DetachedConnectors)
        );
        let mut app = ThinodaApp::headless();
        app.set_file_work_mode(crate::file_ops::FileWorkMode::Background);
        let before = app.doc().unwrap().doc.workbook().clone();
        let (sender, receiver) = mpsc::channel();
        assert!(
            sender
                .send(Ok(Completion::Opened(Opened {
                    report,
                    source: Source {
                        session: 1,
                        id: StorageId("synthetic-drawio-id".into()),
                        revision: Revision("synthetic-revision".into()),
                        name: "diagram.drawio".into(),
                        format: Format::DrawIo,
                    },
                    viewport: ViewportId::ROOT,
                })))
                .is_ok()
        );
        app.cloud_program.running = Some(receiver);
        app.poll_cloud_program(&Context::default());
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(app.cloud_program.import.is_some());

        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 1000.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| app.show_cloud_dialogs(ui.ctx()),
                app,
            );
        h.run_steps(3);
        h.get_by_label(t("Connector attachments were detached"));
        h.get_by_label(t("Cancel")).click();
        h.run_steps(3);
        assert!(h.state().cloud_program.import.is_none());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        assert!(h.state().cloud_program.sources.is_empty());
    }

    #[test]
    fn google_upload_confirmation_exposes_duplicate_warning_and_destination_before_transmission() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let mut app = ThinodaApp::headless();
        app.set_file_work_mode(crate::file_ops::FileWorkMode::Background);
        let (connection, folder) = configured_connection(CloudProvider::GoogleDrive);
        let warning = connection
            .upload_warning()
            .expect("Drive creation needs a duplicate warning");
        let folder_name = folder.name.clone();
        let folder_id = folder.id.0.clone();
        app.cloud_program.connection = Some(Arc::new(connection));
        app.cloud_program.folders = vec![folder];
        app.cloud_program.upload_name = "Potential duplicate.thinoda".into();
        let key = app.docs[0].key.clone();
        app.docs[0].doc.mark_dirty();
        app.cloud_program.ask(Consent::Upload);
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 1000.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| app.show_cloud_dialogs(ui.ctx()),
                app,
            );
        h.run_steps(3);
        h.get_by_label(t(warning));
        assert_eq!(
            h.get_by_label(t("Cloud file name")).value().as_deref(),
            Some("Potential duplicate.thinoda")
        );
        h.get_by_label(&format!("{}: {folder_name}", t("Folder")));
        h.get_by_label(&format!("{}: {folder_id}", t("File ID")));
        assert!(h.get_by_label(t("Upload")).accesskit_node().is_disabled());
        assert!(
            !h.state().cloud_program.busy(),
            "showing consent cannot queue HTTP"
        );
        h.get_by_label(t("I agree to this action")).click();
        h.run_steps(3);
        assert!(!h.get_by_label(t("Upload")).accesskit_node().is_disabled());
        assert!(
            !h.state().cloud_program.busy(),
            "agreement alone cannot queue HTTP"
        );
        h.get_by_label(t("Cancel")).click();
        h.run_steps(3);
        assert!(h.state().cloud_program.consent.is_none());
        assert!(!h.state().cloud_program.busy());
        assert!(!h.state().cloud_program.sources.contains_key(&key));
        assert!(h.state().docs[0].is_dirty());
    }

    #[test]
    fn duplicate_name_warning_is_specific_to_drive_and_uses_existing_upload_consent() {
        for provider in [
            CloudProvider::WebDav,
            CloudProvider::OneDrive,
            CloudProvider::Dropbox,
            CloudProvider::Box,
            CloudProvider::GoogleDrive,
        ] {
            let (connection, _) = configured_connection(provider);
            assert_eq!(
                connection.upload_warning().is_some(),
                provider == CloudProvider::GoogleDrive
            );
            if let Some(warning) = connection.upload_warning() {
                assert!(CLOUD_UI_KEYS.contains(&warning));
            }
        }
    }

    #[derive(Clone)]
    struct WriteCall {
        id: StorageId,
        expected: ExpectedRevision,
        bytes: Vec<u8>,
    }

    struct CreateSpy {
        calls: Arc<std::sync::Mutex<Vec<WriteCall>>>,
        conflict: bool,
    }
    impl StorageProvider for CreateSpy {
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }
        fn list<'a>(&'a self, _: &'a StorageId, _: CancelToken) -> StorageFuture<'a, Vec<Entry>> {
            Box::pin(async { panic!("new upload must not perform a preflight listing") })
        }
        fn read<'a>(
            &'a self,
            _: &'a StorageId,
            _: CancelToken,
        ) -> StorageFuture<'a, thinoda_platform::storage::ReadResult> {
            Box::pin(async { panic!("new upload must not download content automatically") })
        }
        fn write<'a>(
            &'a self,
            id: &'a StorageId,
            bytes: &'a [u8],
            expected: ExpectedRevision,
            cancel: CancelToken,
        ) -> StorageFuture<'a, Revision> {
            self.calls.lock().unwrap().push(WriteCall {
                id: id.clone(),
                expected,
                bytes: bytes.to_vec(),
            });
            let conflict = self.conflict;
            Box::pin(async move {
                cancel.check()?;
                if conflict {
                    Err(StorageError::Conflict {
                        actual: Some(Revision("other".into())),
                    })
                } else {
                    Ok(Revision("written-revision".into()))
                }
            })
        }
    }

    fn mock_creation_app(conflict: bool) -> (ThinodaApp, Arc<std::sync::Mutex<Vec<WriteCall>>>) {
        let mut app = ThinodaApp::headless();
        app.set_file_work_mode(crate::file_ops::FileWorkMode::Background);
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        app.cloud_program.session = 7;
        app.cloud_program.connection = Some(Arc::new(Connection::MockCreator(
            Box::new(CreateSpy {
                calls: calls.clone(),
                conflict,
            }),
            StorageId("server-assigned-file-id".into()),
        )));
        app.cloud_program.folders = vec![Folder {
            id: StorageId("chosen-parent-id".into()),
            name: "Chosen folder".into(),
            dropbox_path: None,
        }];
        app.cloud_program.upload_name = "New map.thinoda".into();
        app.docs[0].doc.mark_dirty();
        (app, calls)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn complete_queued_cloud_job(app: &mut ThinodaApp) {
        let job = app
            .cloud_program
            .pending
            .take()
            .expect("an explicit action queued work");
        let outcome = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(job);
        let (tx, rx) = mpsc::channel();
        tx.send(outcome).ok().unwrap();
        app.cloud_program.running = Some(rx);
        app.poll_cloud_program(&Context::default());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn new_upload_uses_missing_and_retains_server_id_for_later_conditional_save() {
        let (mut app, calls) = mock_creation_app(false);
        let key = app.docs[0].key.clone();
        app.cloud_upload_new();
        let root = app.docs[0].doc.sheet().root;
        app.docs[0]
            .doc
            .edit_sheet("Edit during creation", |sheet| {
                sheet.set_title(root, "Newer content")
            })
            .unwrap();
        complete_queued_cloud_job(&mut app);
        assert!(app.docs[0].is_dirty());
        assert!(app.docs[0].path.is_none());
        assert_eq!(
            app.cloud_program.sources[&key].id,
            StorageId("server-assigned-file-id".into())
        );
        assert_eq!(
            app.cloud_program.sources[&key].revision,
            Revision("written-revision".into())
        );
        assert_eq!(app.cloud_save_active(), Some(false));
        complete_queued_cloud_job(&mut app);
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, StorageId("chosen-parent-id".into()));
        assert_eq!(calls[0].expected, ExpectedRevision::Missing);
        assert_eq!(calls[1].id, StorageId("server-assigned-file-id".into()));
        assert_eq!(
            calls[1].expected,
            ExpectedRevision::Match(Revision("written-revision".into()))
        );
        let original =
            thinoda_io::import_named_bytes_with_report(Path::new("map.thinoda"), &calls[0].bytes)
                .unwrap();
        let saved =
            thinoda_io::import_named_bytes_with_report(Path::new("map.thinoda"), &calls[1].bytes)
                .unwrap();
        assert_ne!(original.workbook, saved.workbook);
        assert_eq!(saved.workbook, *app.docs[0].doc.workbook());
        assert!(!app.docs[0].is_dirty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn duplicate_name_create_conflict_never_overwrites_or_replaces_existing_binding() {
        let (mut app, calls) = mock_creation_app(true);
        let key = app.docs[0].key.clone();
        app.cloud_program.sources.insert(
            key.clone(),
            Source {
                session: 7,
                id: StorageId("original-remote-id".into()),
                revision: Revision("original-revision".into()),
                name: "Original.thinoda".into(),
                format: Format::Thinoda,
            },
        );
        app.cloud_upload_new();
        complete_queued_cloud_job(&mut app);
        assert!(app.cloud_program.conflict_for(&key));
        assert!(!app.cloud_program.conflict.as_ref().unwrap().can_open_remote);
        assert_eq!(
            app.cloud_program.sources[&key].id,
            StorageId("original-remote-id".into())
        );
        assert_eq!(
            app.cloud_program.sources[&key].revision,
            Revision("original-revision".into())
        );
        assert!(app.docs[0].is_dirty());
        assert!(!app.cloud_upload_available());
        app.cloud_resolve_conflict(&Context::default(), false);
        assert_eq!(
            calls.lock().unwrap().len(),
            1,
            "no conflict retry or overwrite"
        );
        assert!(!app.cloud_program.busy());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn upload_of_local_document_is_a_copy_and_never_marks_local_source_saved() {
        let (mut app, _) = mock_creation_app(false);
        let key = app.docs[0].key.clone();
        app.docs[0].path = Some("local.thinoda".into());
        app.cloud_upload_new();
        complete_queued_cloud_job(&mut app);
        assert!(app.docs[0].is_dirty());
        assert_eq!(
            app.docs[0].path.as_deref(),
            Some(Path::new("local.thinoda"))
        );
        assert!(!app.cloud_program.sources.contains_key(&key));
    }

    #[test]
    fn new_upload_rejects_unsafe_names_before_any_adapter_request() {
        let (mut app, calls) = mock_creation_app(false);
        for name in [
            "",
            "../escape.thinoda",
            "folder\\escape.thinoda",
            "map.xmind",
            "map\n.thinoda",
        ] {
            app.cloud_program.upload_name = name.into();
            app.cloud_upload_new();
            assert!(!app.cloud_program.busy(), "{name:?}");
        }
        assert!(calls.lock().unwrap().is_empty());
        assert!(app.docs[0].is_dirty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cancelled_new_upload_never_calls_adapter_or_installs_cloud_source() {
        let (mut app, calls) = mock_creation_app(false);
        let key = app.docs[0].key.clone();
        app.cloud_upload_new();
        app.cloud_program.cancel.cancel();
        complete_queued_cloud_job(&mut app);
        assert!(calls.lock().unwrap().is_empty());
        assert!(!app.cloud_program.sources.contains_key(&key));
        assert!(app.docs[0].is_dirty());
        assert!(!app.cloud_program.busy());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ambiguous_creation_identity_never_marks_document_saved_or_guesses_a_file_id() {
        let (mut app, calls) = mock_creation_app(false);
        app.cloud_program.connection = Some(Arc::new(Connection::MockCreator(
            Box::new(CreateSpy {
                calls: calls.clone(),
                conflict: false,
            }),
            StorageId(String::new()),
        )));
        let key = app.docs[0].key.clone();
        app.cloud_upload_new();
        complete_queued_cloud_job(&mut app);
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert!(app.docs[0].is_dirty());
        assert!(!app.cloud_program.sources.contains_key(&key));
        assert_eq!(
            app.cloud_program.status,
            Some(
                "Cloud creation returned no stable identity or revision. The upload may have reached the server; reopen before saving again."
            )
        );
        assert!(!app.cloud_program.busy());
    }

    #[test]
    fn upload_availability_is_network_free_and_honors_execution_and_busy_guards() {
        let mut app = ThinodaApp::headless();
        let (connection, folder) = configured_connection(CloudProvider::WebDav);
        app.cloud_program.connection = Some(Arc::new(connection));
        app.cloud_program.folders = vec![folder];
        assert!(
            !app.cloud_upload_available(),
            "headless execution must never upload"
        );
        app.set_file_work_mode(crate::file_ops::FileWorkMode::Background);
        assert!(app.cloud_upload_available());
        app.cloud_program.saving_key = Some(app.docs[0].key.clone());
        app.cloud_program.pending = Some(Box::pin(async { Err("Cloud sign-in cancelled.") }));
        assert!(!app.cloud_upload_available());
        app.cloud_program.pending = None;
        app.cloud_program.saving_key = None;
        app.cloud_program.connection = None;
        assert!(
            !app.cloud_upload_available(),
            "disconnected accounts cannot upload"
        );
    }

    #[test]
    fn translation_handoff_has_every_literal_ui_key_once() {
        let unique: std::collections::HashSet<_> = CLOUD_UI_KEYS.iter().copied().collect();
        assert_eq!(unique.len(), CLOUD_UI_KEYS.len());
        let source = include_str!("cloud_program.rs")
            .split("#[cfg(all(test, feature = \"integrated-cloud-services\"))]\nmod tests")
            .next()
            .unwrap();
        for literal in source.split('"').skip(1).step_by(2) {
            if literal.starts_with(char::is_uppercase) && !literal.contains('{') {
                assert!(unique.contains(literal), "missing UI key: {literal}");
            }
        }
    }

    #[test]
    fn unconfigured_settings_and_poll_do_not_start_work() {
        let mut app = ThinodaApp::headless();
        let ctx = Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.cloud_settings_section(ui, true);
            app.poll_cloud_program(ui.ctx());
        });
        output.textures_delta.clear();
        assert_eq!(app.settings.cloud.provider, CloudProvider::None);
        assert!(!app.cloud_program.busy());
        assert!(app.cloud_program.connection.is_none());
    }

    #[test]
    fn consumer_oauth_settings_hide_registration_and_account_ids_for_every_provider() {
        use egui_kittest::{Harness, kittest::Queryable};
        for provider in [
            CloudProvider::OneDrive,
            CloudProvider::Dropbox,
            CloudProvider::Box,
            CloudProvider::GoogleDrive,
        ] {
            let mut app = ThinodaApp::headless();
            app.settings.cloud.provider = provider;
            let mut h = Harness::builder()
                .with_size(egui::vec2(1000.0, 1600.0))
                .build_ui_state(
                    |ui, app: &mut ThinodaApp| {
                        app.cloud_settings_section(ui, true);
                    },
                    app,
                );
            h.run_steps(3);
            for label in [
                "Public OAuth client ID",
                "Starting folder ID or path",
                "OneDrive drive ID",
                "Registered web redirect URI",
            ] {
                assert!(
                    h.query_by_label(label).is_none(),
                    "{provider:?}: ordinary settings exposed {label}"
                );
            }
            h.get_by_label(t("Connect"));
            if consumer_oauth_config(provider).is_err() {
                h.get_by_label(t("Provider sign-in is not enabled in this build. The publisher must configure a registered OAuth application before you can connect."));
            }
            assert!(!h.state().cloud_program.busy());
            assert!(h.state().cloud_program.connection.is_none());
        }
    }

    #[test]
    fn missing_publisher_registration_does_not_fall_back_to_legacy_settings_or_launch() {
        for provider in [
            CloudProvider::OneDrive,
            CloudProvider::Dropbox,
            CloudProvider::Box,
            CloudProvider::GoogleDrive,
        ] {
            let account = account_type(provider).unwrap();
            assert_eq!(
                cloud_auth::PublisherOAuthRegistrations::default()
                    .effective(account, OAuthTarget::Native)
                    .err(),
                Some(cloud_auth::AuthError::BuildConfiguration)
            );
            // Exercise the actual queue gate in builds lacking this registration.
            // Box remains unavailable regardless of build IDs with the current harness.
            if consumer_oauth_config(provider).is_ok() {
                continue;
            }
            let mut app = ThinodaApp::headless();
            app.set_file_work_mode(crate::file_ops::FileWorkMode::Background);
            app.settings.cloud.provider = provider;
            app.settings.cloud.client_id = "legacy-saved-client".into();
            app.settings.cloud.root_folder_id = "legacy-folder".into();
            app.settings.cloud.onedrive_drive_id = "legacy-drive".into();
            app.settings.cloud.redirect_uri = "https://legacy.example/".into();
            app.cloud_program.use_stored = true;
            app.docs[0].doc.mark_dirty();
            app.cloud_accept_consent(Consent::Connect);
            assert!(
                !app.cloud_program.busy(),
                "unavailable registration queued a worker"
            );
            assert!(app.cloud_program.connection.is_none());
            assert!(app.docs[0].is_dirty());
            assert_eq!(
                app.cloud_program.status,
                Some(
                    "Provider sign-in is not enabled in this build. The publisher must configure a registered OAuth application before you can connect."
                )
            );
        }
    }

    #[test]
    fn publisher_registration_controls_transient_inputs_not_saved_ids() {
        let registrations = cloud_auth::PublisherOAuthRegistrations {
            onedrive: cloud_auth::PublicOAuthRegistration {
                native_client_id: Some("publisher-test-id"),
                native_redirect_uri: Some("http://127.0.0.1:49179/callback"),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = registrations
            .effective(CloudAccountType::OneDrive, OAuthTarget::Native)
            .unwrap();
        let saved = CloudSettings {
            provider: CloudProvider::OneDrive,
            client_id: "legacy-id".into(),
            onedrive_drive_id: "legacy-drive".into(),
            root_folder_id: "legacy-root".into(),
            redirect_uri: "https://legacy.example/".into(),
            ..Default::default()
        };
        let transient = consumer_settings(&saved, Some(&config));
        assert_eq!(transient.client_id, "publisher-test-id");
        assert!(transient.onedrive_drive_id.is_empty());
        assert!(transient.root_folder_id.is_empty());
        assert!(transient.redirect_uri.is_empty());
        assert_eq!(
            saved.client_id, "legacy-id",
            "preparation modified persisted preferences"
        );
    }

    #[test]
    fn webdav_defaults_to_server_and_browser_login_without_password_fields() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut app = ThinodaApp::headless();
        app.settings.cloud.provider = CloudProvider::WebDav;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 1600.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    app.cloud_settings_section(ui, true);
                },
                app,
            );
        h.run_steps(3);
        h.get_by_label(t("WebDAV server URL"));
        assert!(h.query_by_label(t("WebDAV username")).is_none());
        assert!(h.query_by_label(t("WebDAV app password")).is_none());
        assert!(!h.state().cloud_program.app_password_login);
        assert!(!h.state().cloud_program.busy());
    }

    #[test]
    fn configured_settings_never_call_mock_until_explicit_list() {
        let mut app = ThinodaApp::headless();
        let calls = Arc::new(AtomicUsize::new(0));
        app.cloud_program.connection =
            Some(Arc::new(Connection::Mock(Box::new(Mock(calls.clone())))));
        app.settings.cloud.provider = CloudProvider::WebDav;
        let ctx = Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.cloud_settings_section(ui, true);
        });
        output.textures_delta.clear();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(!app.cloud_program.busy());
    }

    #[test]
    fn upload_of_old_state_never_marks_newer_state_saved() {
        let mut doc = doc();
        let state = doc.doc.state_id();
        doc.doc
            .edit_sheet("Rename", |sheet| sheet.set_title(sheet.root, "Newer"))
            .unwrap();
        assert!(!mark_same_state_saved(&mut doc, state));
        assert!(doc.is_dirty());
        assert_eq!(doc.doc.sheet().root_topic().title, "Newer");
    }

    #[test]
    fn same_state_upload_marks_saved() {
        let mut doc = doc();
        doc.doc.mark_dirty();
        let state = doc.doc.state_id();
        assert!(mark_same_state_saved(&mut doc, state));
        assert!(!doc.is_dirty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn conditional_mock_upload_completion_preserves_newer_app_edits() {
        let mut app = ThinodaApp::headless();
        app.set_file_work_mode(crate::file_ops::FileWorkMode::Background);
        let calls = Arc::new(AtomicUsize::new(0));
        app.cloud_program.connection =
            Some(Arc::new(Connection::Mock(Box::new(Mock(calls.clone())))));
        app.cloud_program.session = 1;
        app.docs[0].doc.mark_dirty();
        let key = app.docs[0].key.clone();
        app.cloud_program.sources.insert(
            key.clone(),
            Source {
                session: 1,
                id: StorageId("opaque".into()),
                revision: Revision("old".into()),
                name: "map.thinoda".into(),
                format: Format::Thinoda,
            },
        );
        assert_eq!(app.cloud_save_active(), Some(false));
        let root = app.docs[0].doc.sheet().root;
        app.docs[0]
            .doc
            .edit_sheet("Newer edit", |sheet| sheet.set_title(root, "Keep newer"))
            .unwrap();
        let job = app.cloud_program.pending.take().unwrap();
        let outcome = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(job)
            .unwrap();
        let Completion::Saved(save, revision) = outcome else {
            panic!("expected a completed upload")
        };
        app.cloud_program.saving_key = None;
        app.cloud_finish_save(&Context::default(), save, revision);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(app.docs[0].is_dirty());
        assert_eq!(app.docs[0].doc.sheet().root_topic().title, "Keep newer");
        assert_eq!(
            app.cloud_program.sources[&key].revision,
            Revision("new".into())
        );
        assert!(app.docs[0].path.is_none());
    }

    #[test]
    fn reconnecting_never_retargets_an_old_remote_document() {
        let mut app = ThinodaApp::headless();
        app.set_file_work_mode(crate::file_ops::FileWorkMode::Background);
        let calls = Arc::new(AtomicUsize::new(0));
        app.cloud_program.connection =
            Some(Arc::new(Connection::Mock(Box::new(Mock(calls.clone())))));
        app.cloud_program.session = 2;
        app.docs[0].doc.mark_dirty();
        app.cloud_program.sources.insert(
            app.docs[0].key.clone(),
            Source {
                session: 1,
                id: StorageId("opaque".into()),
                revision: Revision("old".into()),
                name: "map.thinoda".into(),
                format: Format::Thinoda,
            },
        );
        assert_eq!(app.cloud_save_active(), Some(false));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(!app.cloud_program.busy());
        assert!(app.docs[0].is_dirty());
    }

    #[test]
    fn local_save_as_path_invalidates_remote_source_before_next_save() {
        let mut app = ThinodaApp::headless();
        app.set_file_work_mode(crate::file_ops::FileWorkMode::Background);
        let calls = Arc::new(AtomicUsize::new(0));
        app.cloud_program.connection =
            Some(Arc::new(Connection::Mock(Box::new(Mock(calls.clone())))));
        let key = app.docs[0].key.clone();
        app.cloud_program.sources.insert(
            key.clone(),
            Source {
                session: 0,
                id: StorageId("remote".into()),
                revision: Revision("old".into()),
                name: "map.thinoda".into(),
                format: Format::Thinoda,
            },
        );
        // The local writer's finish_save sets this path before its completion
        // callback. Even without forget_document/housekeeping, Save is local.
        app.docs[0].path = Some("local-copy.thinoda".into());
        app.docs[0].doc.mark_dirty();
        assert_eq!(app.cloud_save_active(), None);
        assert!(!app.cloud_program.sources.contains_key(&key));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(app.docs[0].is_dirty());
    }

    #[test]
    fn forgetting_remote_binding_preserves_edits() {
        let mut app = ThinodaApp::headless();
        let key = app.docs[0].key.clone();
        app.docs[0].doc.mark_dirty();
        app.cloud_program.sources.insert(
            key.clone(),
            Source {
                session: 0,
                id: StorageId("remote".into()),
                revision: Revision("old".into()),
                name: "map.thinoda".into(),
                format: Format::Thinoda,
            },
        );
        let before = app.docs[0].doc.workbook().clone();
        app.cloud_program.forget_document(&key);
        assert!(app.docs[0].is_dirty());
        assert_eq!(app.docs[0].doc.workbook(), &before);
        assert!(!app.cloud_program.sources.contains_key(&key));
    }

    #[test]
    fn cancelling_remote_conflict_ends_close_continuation_without_saving() {
        let mut app = ThinodaApp::headless();
        let key = app.docs[0].key.clone();
        let source = Source {
            session: 0,
            id: StorageId("remote".into()),
            revision: Revision("old".into()),
            name: "map.thinoda".into(),
            format: Format::Thinoda,
        };
        app.docs[0].doc.mark_dirty();
        app.dialogs.save_close_tab = Some(key.clone());
        app.cloud_program.conflict = Some(Conflict {
            save: Save {
                key: key.clone(),
                state: app.docs[0].doc.state_id(),
                source,
                attach: true,
            },
            can_open_remote: true,
        });
        assert!(app.cloud_program.conflict_for(&key));
        assert!(app.cloud_program.modal_open());
        assert!(!app.cloud_program.saving());
        assert!(app.dialogs.conflict.is_none());
        app.cloud_resolve_conflict(&Context::default(), false);
        assert!(!app.cloud_program.conflict_for(&key));
        assert!(app.dialogs.save_close_tab.is_none());
        assert!(app.docs.iter().any(|doc| doc.key == key && doc.is_dirty()));
        assert!(!app.cloud_program.busy());
    }

    #[test]
    fn disconnect_keeps_edits_and_source_identity() {
        let mut doc = doc();
        doc.doc
            .edit_sheet("Rename", |sheet| sheet.set_title(sheet.root, "Keep me"))
            .unwrap();
        let workbook = doc.doc.workbook().clone();
        let mut program = CloudProgram::default();
        program.sources.insert(
            doc.key.clone(),
            Source {
                session: 1,
                id: StorageId("opaque".into()),
                revision: Revision("old".into()),
                name: "map.thinoda".into(),
                format: Format::Thinoda,
            },
        );
        program.detach();
        assert!(doc.is_dirty());
        assert_eq!(doc.doc.workbook(), &workbook);
        assert!(program.sources.contains_key(&doc.key));
    }

    #[test]
    fn public_settings_default_and_roundtrip_contain_no_credentials() {
        let settings: CloudSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.provider, CloudProvider::None);
        let encoded = serde_json::to_string(&settings).unwrap();
        for key in [
            "password",
            "access_token",
            "refresh_token",
            "username",
            "client_secret",
        ] {
            assert!(!encoded.contains(key));
        }
        let restored: CloudSettings = serde_json::from_str(&encoded).unwrap();
        assert_eq!(settings, restored);
    }

    #[test]
    fn public_urls_and_leaf_names_reject_secrets_and_path_escape() {
        assert!(public_url("https://cloud.example/nextcloud"));
        assert!(public_url("http://127.0.0.1:8123/dav"));
        for url in [
            "https://user:secret@cloud.example/",
            "https://cloud.example/?token=secret",
            "https://cloud.example/#secret",
            "http://cloud.example",
            "https://cloud.example\\@other",
        ] {
            assert!(!public_url(url));
        }
        assert!(leaf_name("Übersicht.thinoda"));
        assert!(!leaf_name("../elsewhere.thinoda"));
        assert!(!leaf_name("dir\\elsewhere.thinoda"));
        assert_eq!(segment("a b/ü"), "a%20b%2F%C3%BC");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn backup_folder_rejects_sync_root_and_children_but_not_prefix_siblings() {
        let dir = tempfile::tempdir().unwrap();
        let sync = dir.path().join("sync");
        let child = sync.join("backups");
        let outside = dir.path().join("sync-backups");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let roots = vec![thinoda_platform::cloud::CloudFolder {
            provider: thinoda_platform::cloud::CloudProvider::OneDrive,
            name: "OneDrive".into(),
            path: sync.clone(),
        }];
        assert!(!backup_folder_allowed(&sync, &roots));
        assert!(!backup_folder_allowed(&child, &roots));
        assert!(backup_folder_allowed(&outside, &roots));
        assert!(!backup_folder_allowed(&dir.path().join("missing"), &roots));
    }

    #[test]
    fn document_source_never_becomes_a_local_path() {
        let doc = doc();
        let _: &Document = &doc.doc;
        assert!(doc.path.is_none());
        assert!(doc.import_source.is_none());
        assert!(doc.storage_revision.is_none());
    }
}
