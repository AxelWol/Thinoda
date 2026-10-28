//! Native CLI adapters. Prompts travel over stdin, never through a shell.
//! No authentication files are inspected; only documented, non-inference status
//! commands are used. CLI responses are buffered (not token streaming).

use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
use crate::{AiConfig, AiError, Message, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    OpenAiApi,
    CopilotCli,
    ClaudeCli,
    CodexCli,
}

impl Provider {
    pub fn command(self) -> &'static str {
        match self {
            Self::OpenAiApi => "",
            Self::CopilotCli => "copilot",
            Self::ClaudeCli => "claude",
            Self::CodexCli => "codex",
        }
    }

    pub fn login_command(self) -> &'static str {
        match self {
            Self::OpenAiApi => "",
            Self::CopilotCli => "copilot login",
            Self::ClaudeCli => "claude auth login",
            Self::CodexCli => "codex login",
        }
    }

    pub fn documentation(self) -> &'static str {
        match self {
            Self::OpenAiApi => "https://platform.openai.com/docs",
            Self::CopilotCli => "https://docs.github.com/en/copilot/how-tos/copilot-cli",
            Self::ClaudeCli => "https://code.claude.com/docs/en/setup",
            Self::CodexCli => "https://developers.openai.com/codex/cli",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HarnessError {
    #[error("Native CLIs are unavailable in the browser. The selection is preserved.")]
    Browser,
    #[error("CLI not found on PATH. Install it, restart Thinoda, then check again.")]
    Missing,
    #[error(
        "This CLI installation cannot be launched safely. Use a supported native or npm installation."
    )]
    Unsupported,
    #[error(
        "Copilot CLI has no documented isolated mode that disables all user MCP servers and hooks. Generation is unavailable."
    )]
    CopilotSafety,
    #[error("CLI check failed or is unsupported. Authentication could not be verified.")]
    Probe,
    #[error("CLI request timed out.")]
    Timeout,
    #[error("CLI output exceeded the safety limit.")]
    OutputLimit,
    #[error(
        "CLI request failed. Check authentication and update the CLI using its official documentation."
    )]
    Failed,
    #[error("Could not start the isolated CLI process.")]
    Start,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliStatus {
    Missing,
    Authenticated,
    Unauthenticated,
    Unknown,
    Unsupported,
    Browser,
    Error(HarnessError),
}

/// Start discovery/authentication inspection on an AI worker, never on the UI thread.
pub fn check(provider: Provider) -> crate::AiJob<CliStatus> {
    #[cfg(target_arch = "wasm32")]
    let _ = provider;
    #[cfg(not(target_arch = "wasm32"))]
    return crate::AiJob::spawn_with(
        AiConfig {
            provider,
            ..Default::default()
        },
        move |_| Ok(native::probe(provider)),
    );
    #[cfg(target_arch = "wasm32")]
    crate::AiJob::spawn_local(async { Ok(CliStatus::Browser) })
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn chat_blocking(cfg: &AiConfig, model: &str, messages: &[Message]) -> Result<String> {
    chat(cfg, model, messages, || false)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn chat(
    cfg: &AiConfig,
    model: &str,
    messages: &[Message],
    cancelled: impl Fn() -> bool,
) -> Result<String> {
    native::generate(cfg, model, messages, cancelled)
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::{
        ffi::OsString,
        io::{Read, Write},
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };

    const MAX_OUTPUT: usize = 2 * 1024 * 1024;
    const MAX_PROMPT: usize = 256 * 1024;

    fn error(e: HarnessError) -> AiError {
        AiError::Harness(e)
    }

    fn find(name: &str) -> Option<PathBuf> {
        find_in(name, std::env::var_os("PATH").as_deref())
    }

    fn find_in(name: &str, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
        let path = path?;
        for dir in std::env::split_paths(path).filter(|dir| dir.is_absolute()) {
            #[cfg(windows)]
            let names = [
                format!("{name}.exe"),
                format!("{name}.cmd"),
                format!("{name}.bat"),
            ];
            #[cfg(not(windows))]
            let names = [name.to_owned()];
            for name in names {
                let file = dir.join(name);
                if file.is_file() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if file.metadata().ok()?.permissions().mode() & 0o111 == 0 {
                            continue;
                        }
                    }
                    return Some(file);
                }
            }
        }
        None
    }

    /// npm shims are never passed to cmd.exe. Resolve the known package entry point
    /// and invoke Node directly, so spaces, quotes, %, ! and & are never shell syntax.
    /// Arbitrary batch wrappers fail closed rather than attempting shell quoting.
    fn invocation(path: &Path, provider: Provider) -> Result<(PathBuf, Vec<OsString>)> {
        #[cfg(windows)]
        if path
            .extension()
            .is_some_and(|ext| ext == "cmd" || ext == "bat")
        {
            let package = match provider {
                Provider::ClaudeCli => "@anthropic-ai/claude-code/cli.js",
                Provider::CodexCli => "@openai/codex/bin/codex.js",
                _ => return Err(error(HarnessError::Unsupported)),
            };
            let parent = path
                .parent()
                .ok_or_else(|| error(HarnessError::Unsupported))?;
            let entry = parent.join("node_modules").join(package);
            if !entry.is_file() {
                return Err(error(HarnessError::Unsupported));
            }
            let adjacent = parent.join("node.exe");
            let node = if adjacent.is_file() {
                adjacent
            } else {
                find("node")
                    .filter(|p| p.extension().is_some_and(|e| e == "exe"))
                    .ok_or_else(|| error(HarnessError::Unsupported))?
            };
            return Ok((node, vec![entry.into_os_string()]));
        }
        let _ = provider;
        Ok((path.to_owned(), Vec::new()))
    }

    #[derive(Debug)]
    struct Output {
        code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    }

    /// Reap even when thread creation or an unexpected worker panic unwinds.
    struct ReapChild(std::process::Child);

    impl std::ops::Deref for ReapChild {
        type Target = std::process::Child;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl std::ops::DerefMut for ReapChild {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl Drop for ReapChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn read_bounded(mut stream: impl Read, overflow: &AtomicBool) -> Vec<u8> {
        let mut result = Vec::new();
        let mut buffer = [0; 8192];
        while let Ok(count) = stream.read(&mut buffer) {
            if count == 0 {
                break;
            }
            if result.len().saturating_add(count) > MAX_OUTPUT {
                overflow.store(true, Ordering::Release);
                break;
            }
            result.extend_from_slice(&buffer[..count]);
        }
        result
    }

    fn run(
        path: &Path,
        provider: Provider,
        args: &[String],
        input: Vec<u8>,
        timeout: Duration,
        cancelled: impl Fn() -> bool,
    ) -> Result<Output> {
        let (exe, prefix) = invocation(path, provider)?;
        let dir = tempfile::Builder::new()
            .prefix("thinoda-ai-")
            .tempdir()
            .map_err(|_| error(HarnessError::Start))?;
        let mut command = Command::new(exe);
        command
            .args(prefix)
            .args(args)
            .current_dir(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Prevent process injection through inherited Node options or loader hooks.
        for key in [
            "NODE_OPTIONS",
            "NODE_PATH",
            "LD_PRELOAD",
            "DYLD_INSERT_LIBRARIES",
        ] {
            command.env_remove(key);
        }
        command.env("NO_COLOR", "1");
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Suspend until the kill-on-close Job Object is attached, so even a
            // fast CLI cannot spawn an unowned descendant before containment.
            command.creation_flags(0x08000004); // CREATE_NO_WINDOW | CREATE_SUSPENDED
        }
        let mut child = ReapChild(command.spawn().map_err(|_| error(HarnessError::Start))?);
        let tree = match ProcessTree::new(&child) {
            Ok(tree) => tree,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let overflow = Arc::new(AtomicBool::new(false));
        // All three pipes are serviced concurrently; large inputs/outputs cannot
        // deadlock the worker. The tree is terminated before joining readers.
        let out_limit = overflow.clone();
        let err_limit = overflow.clone();
        let out = std::thread::spawn(move || read_bounded(stdout, &out_limit));
        let err = std::thread::spawn(move || read_bounded(stderr, &err_limit));
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
        let start = Instant::now();
        let result = loop {
            if cancelled() {
                break Err(AiError::Cancelled);
            }
            if overflow.load(Ordering::Acquire) {
                break Err(error(HarnessError::OutputLimit));
            }
            if start.elapsed() >= timeout {
                break Err(error(HarnessError::Timeout));
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status.code()),
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(_) => break Err(error(HarnessError::Failed)),
            }
        };
        // Includes any descendant still holding pipes after the leader exited.
        drop(tree);
        let _ = child.kill();
        let _ = child.wait();
        let _ = writer.join();
        let stdout = out.join().unwrap_or_default();
        let stderr = err.join().unwrap_or_default();
        let code = result?;
        if overflow.load(Ordering::Acquire) {
            return Err(error(HarnessError::OutputLimit));
        }
        Ok(Output {
            code,
            stdout,
            stderr,
        })
    }

    pub(super) fn probe(provider: Provider) -> CliStatus {
        let Some(path) = find(provider.command()) else {
            return CliStatus::Missing;
        };
        if provider == Provider::CopilotCli {
            // There is no documented non-inference auth-status command.
            return CliStatus::Unknown;
        }
        let args = match provider {
            Provider::ClaudeCli => vec!["auth".into(), "status".into()],
            Provider::CodexCli => vec!["login".into(), "status".into()],
            _ => return CliStatus::Unsupported,
        };
        match run(
            &path,
            provider,
            &args,
            Vec::new(),
            Duration::from_secs(10),
            || false,
        ) {
            Ok(output) => parse_status(provider, &output),
            Err(AiError::Harness(HarnessError::Unsupported)) => CliStatus::Unsupported,
            Err(AiError::Harness(e)) => CliStatus::Error(e),
            Err(_) => CliStatus::Error(HarnessError::Probe),
        }
    }

    fn parse_status(provider: Provider, output: &Output) -> CliStatus {
        match provider {
            Provider::ClaudeCli => {
                let logged_in = serde_json::from_slice::<serde_json::Value>(&output.stdout)
                    .ok()
                    .and_then(|v| v.get("loggedIn").and_then(|v| v.as_bool()));
                match (output.code, logged_in) {
                    (Some(0), Some(true)) => CliStatus::Authenticated,
                    (Some(1), Some(false)) => CliStatus::Unauthenticated,
                    _ => CliStatus::Unknown,
                }
            }
            Provider::CodexCli => {
                // Never expose output: some CLI versions include key fragments.
                let out = String::from_utf8_lossy(&output.stdout);
                let err = String::from_utf8_lossy(&output.stderr);
                if output.code == Some(0)
                    && (out.starts_with("Logged in") || err.starts_with("Logged in"))
                {
                    CliStatus::Authenticated
                } else if output.code == Some(1)
                    && (out.trim() == "Not logged in" || err.trim() == "Not logged in")
                {
                    CliStatus::Unauthenticated
                } else {
                    CliStatus::Unknown
                }
            }
            _ => CliStatus::Unknown,
        }
    }

    fn generation_args(provider: Provider, model: &str) -> Result<Vec<String>> {
        let mut args: Vec<String> = match provider {
            Provider::ClaudeCli => [
                "-p",
                "--output-format",
                "json",
                "--tools=",
                "--strict-mcp-config",
                "--mcp-config",
                r#"{"mcpServers":{}}"#,
                "--setting-sources=",
                "--settings",
                r#"{"disableAllHooks":true}"#,
                "--safe-mode",
                "--no-session-persistence",
                "--no-chrome",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            Provider::CodexCli => [
                "exec",
                "--ignore-user-config",
                "--ignore-rules",
                "--ephemeral",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "--json",
                "-c",
                r#"approval_policy="never""#,
                "-c",
                "features.shell_tool=false",
                "-c",
                "features.hooks=false",
                "-c",
                "features.multi_agent=false",
                "-c",
                "agents.enabled=false",
                "-c",
                "features.apps=false",
                "-c",
                "features.remote_plugin=false",
                "-c",
                "mcp_servers={}",
                "-c",
                r#"web_search="disabled""#,
                "-c",
                "check_for_update_on_startup=false",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            Provider::CopilotCli => return Err(error(HarnessError::CopilotSafety)),
            Provider::OpenAiApi => return Err(error(HarnessError::Unsupported)),
        };
        if !model.trim().is_empty() {
            args.extend(["--model".into(), model.to_owned()]);
        }
        if provider == Provider::CodexCli {
            args.push("-".into());
        }
        Ok(args)
    }

    pub(super) fn generate(
        cfg: &AiConfig,
        model: &str,
        messages: &[Message],
        cancelled: impl Fn() -> bool,
    ) -> Result<String> {
        // Check policy before discovery, including the fail-closed Copilot adapter.
        generation_args(cfg.provider, model)?;
        let path = find(cfg.provider.command()).ok_or_else(|| error(HarnessError::Missing))?;
        generate_at(&path, cfg, model, messages, cancelled)
    }

    fn generate_at(
        path: &Path,
        cfg: &AiConfig,
        model: &str,
        messages: &[Message],
        cancelled: impl Fn() -> bool,
    ) -> Result<String> {
        let args = generation_args(cfg.provider, model)?;
        // A role-labelled JSON transcript preserves system/task/language instructions,
        // history and arbitrary user text without converting any text to CLI flags.
        let input = serde_json::to_vec(messages).map_err(|_| error(HarnessError::Failed))?;
        if input.len() > MAX_PROMPT {
            return Err(error(HarnessError::OutputLimit));
        }
        let output = run(
            path,
            cfg.provider,
            &args,
            input,
            Duration::from_secs(cfg.timeout_secs.clamp(1, 300)),
            cancelled,
        )?;
        if output.code != Some(0) {
            return Err(error(HarnessError::Failed));
        }
        parse_reply(cfg.provider, &output.stdout)
    }

    fn parse_reply(provider: Provider, bytes: &[u8]) -> Result<String> {
        let text = match provider {
            Provider::ClaudeCli => {
                let value: serde_json::Value =
                    serde_json::from_slice(bytes).map_err(|_| error(HarnessError::Failed))?;
                if value.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
                    return Err(error(HarnessError::Failed));
                }
                value
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned()
            }
            Provider::CodexCli => {
                let mut text = String::new();
                for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
                    let value: serde_json::Value =
                        serde_json::from_slice(line).map_err(|_| error(HarnessError::Failed))?;
                    if value["type"] == "turn.failed" || value["type"] == "error" {
                        return Err(error(HarnessError::Failed));
                    }
                    if value["type"] == "item.completed" && value["item"]["type"] == "agent_message"
                    {
                        text = value["item"]["text"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned();
                    }
                }
                text
            }
            _ => return Err(error(HarnessError::Unsupported)),
        };
        if text.trim().is_empty() {
            Err(AiError::Empty)
        } else {
            Ok(text)
        }
    }

    #[cfg(unix)]
    struct ProcessTree(i32);

    #[cfg(unix)]
    impl ProcessTree {
        fn new(child: &std::process::Child) -> Result<Self> {
            Ok(Self(child.id() as i32))
        }
    }

    #[cfg(unix)]
    impl Drop for ProcessTree {
        fn drop(&mut self) {
            // The child created its own process group; never signal our own group.
            unsafe {
                libc::kill(-self.0, libc::SIGKILL);
            }
        }
    }

    #[cfg(windows)]
    struct ProcessTree(windows_sys::Win32::Foundation::HANDLE);

    #[cfg(windows)]
    impl ProcessTree {
        fn new(child: &std::process::Child) -> Result<Self> {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::*;
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return Err(error(HarnessError::Start));
                }
                let tree = Self(handle);
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of_val(&info) as u32,
                ) == 0
                    || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0
                {
                    return Err(error(HarnessError::Start));
                }
                resume_contained(child.id())?;
                Ok(tree)
            }
        }
    }

    #[cfg(windows)]
    fn resume_contained(pid: u32) -> Result<()> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
            System::{Diagnostics::ToolHelp::*, Threading::*},
        };
        // std::process::Child exposes the process handle, not its initial thread.
        // Enumerate that still-suspended thread using documented Win32 APIs.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(error(HarnessError::Start));
            }
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of_val(&entry) as u32;
            let mut found = Thread32First(snapshot, &mut entry) != 0;
            let mut result = Err(error(HarnessError::Start));
            while found {
                if entry.th32OwnerProcessID == pid {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    if !thread.is_null() {
                        if ResumeThread(thread) != u32::MAX {
                            result = Ok(());
                        }
                        CloseHandle(thread);
                    }
                    break;
                }
                found = Thread32Next(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
            result
        }
    }

    #[cfg(windows)]
    impl Drop for ProcessTree {
        fn drop(&mut self) {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn authentication_requires_documented_status_not_arbitrary_exit_code() {
            let output = |code, stdout: &str, stderr: &str| Output {
                code: Some(code),
                stdout: stdout.as_bytes().to_vec(),
                stderr: stderr.as_bytes().to_vec(),
            };
            assert_eq!(
                parse_status(Provider::ClaudeCli, &output(0, r#"{"loggedIn":true}"#, "")),
                CliStatus::Authenticated
            );
            assert_eq!(
                parse_status(Provider::ClaudeCli, &output(1, r#"{"loggedIn":false}"#, "")),
                CliStatus::Unauthenticated
            );
            assert_eq!(
                parse_status(Provider::ClaudeCli, &output(1, "", "unknown command")),
                CliStatus::Unknown
            );
            assert_eq!(
                parse_status(
                    Provider::CodexCli,
                    &output(0, "", "Logged in using ChatGPT\n")
                ),
                CliStatus::Authenticated
            );
            assert_eq!(
                parse_status(Provider::CodexCli, &output(1, "", "Not logged in\n")),
                CliStatus::Unauthenticated
            );
            assert_eq!(
                parse_status(Provider::CodexCli, &output(0, "token exists", "")),
                CliStatus::Unknown
            );
        }

        #[test]
        fn final_responses_preserve_task_output_and_reject_errors() {
            assert_eq!(
                parse_reply(
                    Provider::ClaudeCli,
                    br##"{"result":"# Map\n- Idea","is_error":false}"##
                )
                .unwrap(),
                "# Map\n- Idea"
            );
            assert_eq!(parse_reply(Provider::CodexCli, b"{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Answer\"}}\n").unwrap(), "Answer");
            assert!(
                parse_reply(
                    Provider::ClaudeCli,
                    br#"{"result":"secret diagnostic","is_error":true}"#
                )
                .is_err()
            );
            assert!(parse_reply(Provider::CodexCli, b"{\"type\":\"turn.failed\"}\n").is_err());
        }

        #[test]
        fn arguments_never_enable_uncontrolled_execution() {
            for provider in [Provider::ClaudeCli, Provider::CodexCli] {
                let args = generation_args(provider, "model & arbitrary").unwrap();
                assert!(
                    !args
                        .iter()
                        .any(|a| a.contains("yolo") || a.contains("skip-permissions"))
                );
                assert!(args.contains(&"model & arbitrary".into()));
            }
            assert_eq!(
                generation_args(Provider::CopilotCli, "").unwrap_err(),
                error(HarnessError::CopilotSafety)
            );
        }

        fn fixture(script: &str) -> (PathBuf, Vec<String>) {
            #[cfg(windows)]
            {
                let path = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
                    .join("System32/WindowsPowerShell/v1.0/powershell.exe");
                (
                    path,
                    vec![
                        "-NoLogo".into(),
                        "-NoProfile".into(),
                        "-NonInteractive".into(),
                        "-Command".into(),
                        script.into(),
                    ],
                )
            }
            #[cfg(unix)]
            {
                (PathBuf::from("/bin/sh"), vec!["-c".into(), script.into()])
            }
        }

        #[test]
        fn subprocess_prompt_is_data_and_working_directory_is_isolated() {
            #[cfg(windows)]
            let script = "$r=[IO.StreamReader]::new([Console]::OpenStandardInput()); [Console]::Write($r.ReadToEnd()); [Console]::Error.Write((Get-Location).Path)";
            #[cfg(unix)]
            let script = "cat; pwd >&2";
            let (path, args) = fixture(script);
            let input = br#"[{"role":"user","content":"$(touch injected) & echo secret; %PATH% ! \"quoted\""}]"#.to_vec();
            let output = run(
                &path,
                Provider::ClaudeCli,
                &args,
                input.clone(),
                Duration::from_secs(30),
                || false,
            )
            .unwrap();
            assert_eq!(output.code, Some(0));
            assert_eq!(output.stdout, input);
            let cwd = String::from_utf8_lossy(&output.stderr);
            assert!(cwd.contains("thinoda-ai-"));
            assert!(
                !Path::new(cwd.trim()).exists(),
                "isolated directory must be removed"
            );
        }

        #[test]
        fn subprocess_timeout_and_cancel_reap_with_bounded_waits() {
            #[cfg(windows)]
            let script = "Start-Sleep -Seconds 60";
            #[cfg(unix)]
            let script = "sleep 60";
            let (path, args) = fixture(script);
            for cancel in [false, true] {
                let start = Instant::now();
                let result = run(
                    &path,
                    Provider::ClaudeCli,
                    &args,
                    Vec::new(),
                    Duration::from_millis(100),
                    || cancel && start.elapsed() >= Duration::from_millis(30),
                );
                assert_eq!(
                    result.unwrap_err(),
                    if cancel {
                        AiError::Cancelled
                    } else {
                        error(HarnessError::Timeout)
                    }
                );
                assert!(start.elapsed() < Duration::from_secs(5));
            }
        }

        #[test]
        fn oversized_stdout_and_stderr_fail_without_deadlocking() {
            #[cfg(windows)]
            let script = "$s='x'*8192; for($i=0;$i -lt 400;$i++){[Console]::Out.Write($s);[Console]::Error.Write($s)}";
            #[cfg(unix)]
            let script = "head -c 3000000 /dev/zero; head -c 3000000 /dev/zero >&2";
            let (path, args) = fixture(script);
            assert_eq!(
                run(
                    &path,
                    Provider::ClaudeCli,
                    &args,
                    Vec::new(),
                    Duration::from_secs(30),
                    || false
                )
                .unwrap_err(),
                error(HarnessError::OutputLimit)
            );
        }

        #[test]
        fn discovery_ignores_relative_and_empty_path_entries() {
            let dir = tempfile::tempdir().unwrap();
            #[cfg(windows)]
            let name = "claude.exe";
            #[cfg(unix)]
            let name = "claude";
            let executable = dir.path().join(name);
            std::fs::write(&executable, []).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
                    .unwrap();
            }
            let path = std::env::join_paths([Path::new(""), Path::new("."), dir.path()]).unwrap();
            assert_eq!(find_in("claude", Some(&path)), Some(executable));
            assert!(find_in("claude", Some(std::ffi::OsStr::new("."))).is_none());
        }

        #[test]
        #[cfg(windows)]
        fn batch_wrappers_are_not_executed_by_a_shell() {
            let dir = tempfile::Builder::new()
                .prefix("thinoda space & percent%-")
                .tempdir()
                .unwrap();
            let shim = dir.path().join("claude.cmd");
            std::fs::write(&shim, "@echo must-not-run").unwrap();
            assert_eq!(
                invocation(&shim, Provider::ClaudeCli).unwrap_err(),
                error(HarnessError::Unsupported)
            );
            let entry = dir
                .path()
                .join("node_modules/@anthropic-ai/claude-code/cli.js");
            std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
            std::fs::write(&entry, "// fixture").unwrap();
            let node = dir.path().join("node.exe");
            std::fs::write(&node, []).unwrap();
            let (exe, prefix) = invocation(&shim, Provider::ClaudeCli).unwrap();
            assert_eq!(exe, node);
            assert_eq!(prefix.len(), 1);
            assert_eq!(PathBuf::from(&prefix[0]), entry);
        }

        #[test]
        #[cfg(windows)]
        fn npm_fixture_generates_tasks_without_shell_prompt_interpolation() {
            // Node belongs to the fixture, not a live AI tool. Native-process
            // bounds/cancellation fixtures above do not require Node.
            let Some(_) = find("node") else {
                eprintln!("Node unavailable: skipping npm fixture");
                return;
            };
            for provider in [Provider::ClaudeCli, Provider::CodexCli] {
                let dir = tempfile::Builder::new()
                    .prefix("thinoda fixture & spaces%-")
                    .tempdir()
                    .unwrap();
                let package = if provider == Provider::ClaudeCli {
                    "@anthropic-ai/claude-code/cli.js"
                } else {
                    "@openai/codex/bin/codex.js"
                };
                let entry = dir.path().join("node_modules").join(package);
                std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
                std::fs::write(
                    &entry,
                    r##"
let data = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', d => data += d);
process.stdin.on('end', () => {
  const messages = JSON.parse(data);
  const combined = messages.map(m => m.content).join('\n');
  if (!combined.includes('Write your entire reply in German')
      || !combined.includes('& %PATH% ! "quoted"')) process.exit(3);
  const text = '# Fixture map\n- First\n- Second';
  if (process.argv.includes('exec')) {
    console.log(JSON.stringify({type:'item.completed',item:{type:'agent_message',text}}));
  } else console.log(JSON.stringify({result:text,is_error:false}));
});
"##,
                )
                .unwrap();
                let shim = dir.path().join(format!("{}.cmd", provider.command()));
                std::fs::write(&shim, "@echo this-wrapper-must-never-run").unwrap();
                let cfg = AiConfig {
                    provider,
                    base_url: "http://127.0.0.1:9/v1".into(),
                    reply_language: "German".into(),
                    ..Default::default()
                };
                let task = crate::AiTask::GenerateMap("& %PATH% ! \"quoted\"".into());
                let text =
                    generate_at(&shim, &cfg, "", &task.messages_in("German"), || false).unwrap();
                let parsed = crate::finish(&task, &text, cfg.model.clone()).unwrap();
                assert!(matches!(parsed.output, crate::AiOutput::Outline(_)));
            }
        }
    }
}
