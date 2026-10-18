//! Native OS credential storage for secrets that must not be written to settings files.

/// Credential-store identifiers are stable so existing installations can migrate in place.
const SERVICE: &str = "Thinoda";
const ACCOUNT: &str = "ai-api-key";

/// Operations needed by settings persistence; implementations must not log credential values.
pub trait CredentialStore {
    fn get_password(&self) -> std::result::Result<Option<String>, String>;
    fn set_password(&self, password: &str) -> std::result::Result<(), String>;
    fn delete_password(&self) -> std::result::Result<(), String>;
}

/// Credential storage backed by the current operating system's native secret store.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeCredentialStore;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn entry() -> std::result::Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| error.to_string())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn entry() -> std::result::Result<(), String> {
    Err("native credential storage is unsupported on this platform".into())
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
impl CredentialStore for NativeCredentialStore {
    fn get_password(&self) -> std::result::Result<Option<String>, String> {
        match entry()?.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn set_password(&self, password: &str) -> std::result::Result<(), String> {
        entry()?
            .set_password(password)
            .map_err(|error| error.to_string())
    }

    fn delete_password(&self) -> std::result::Result<(), String> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
impl CredentialStore for NativeCredentialStore {
    fn get_password(&self) -> std::result::Result<Option<String>, String> {
        entry()?;
        Ok(None)
    }

    fn set_password(&self, _password: &str) -> std::result::Result<(), String> {
        entry()
    }

    fn delete_password(&self) -> std::result::Result<(), String> {
        entry()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemoryCredentials(Mutex<Option<String>>);

    impl CredentialStore for MemoryCredentials {
        fn get_password(&self) -> std::result::Result<Option<String>, String> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn set_password(&self, password: &str) -> std::result::Result<(), String> {
            *self.0.lock().unwrap() = Some(password.to_owned());
            Ok(())
        }

        fn delete_password(&self) -> std::result::Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    #[test]
    fn mock_store_roundtrips_and_deletes_without_native_credentials() {
        let store = MemoryCredentials::default();
        assert_eq!(store.get_password().unwrap(), None);
        store.set_password("test-secret").unwrap();
        assert_eq!(
            store.get_password().unwrap().as_deref(),
            Some("test-secret")
        );
        store.delete_password().unwrap();
        assert_eq!(store.get_password().unwrap(), None);
    }
}
