use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Sandbox {
    root: PathBuf,
    socket: PathBuf,
    server: Option<Child>,
}

impl Sandbox {
    fn new() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("mirmir-cli-{}-{id}", std::process::id()));
        fs::create_dir_all(root.join("client/config")).expect("client fixture");
        fs::write(root.join("client/config/config.toml"), "schema_version = 99\n")
            .expect("invalid local runtime configuration");
        fs::write(root.join("client/.env"), "invalid dotenv syntax !\n")
            .expect("invalid local dotenv");
        Self {
            socket: root.join("run/runtime.sock"),
            root,
            server: None,
        }
    }

    fn client(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mirmir"))
            .arg("--socket")
            .arg(&self.socket)
            .args(args)
            .current_dir(self.root.join("client"))
            .env_remove("HOME")
            .env_remove("MIRMIR_MODEL")
            .env("MIRMIR_CONFIG_HOME", self.root.join("client/config"))
            .env("MIRMIR_STATE_HOME", self.root.join("client/state"))
            .env("XDG_RUNTIME_DIR", self.root.join("client/run"))
            .output()
            .expect("execute client")
    }

    fn start(&mut self) {
        fs::create_dir_all(self.root.join("server/config")).expect("server fixture");
        fs::write(
            self.root.join("server/config/config.toml"),
            "default_model = 'server-default'\n[server]\nmax_concurrency = 7\n",
        )
        .expect("server configuration");
        self.server = Some(
            Command::new(env!("CARGO_BIN_EXE_mirmir"))
                .arg("--socket")
                .arg(&self.socket)
                .args(["serve", "--no-http", "--shared-socket"])
                .env("MIRMIR_CONFIG_HOME", self.root.join("server/config"))
                .env("MIRMIR_STATE_HOME", self.root.join("server/state"))
                .current_dir(self.root.join("server"))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("start fixture service"),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if self.client(&["status"]).status.success() {
                break;
            }
            assert!(Instant::now() < deadline, "fixture service did not become reachable");
            assert!(
                self.server
                    .as_mut()
                    .expect("server")
                    .try_wait()
                    .expect("server state")
                    .is_none(),
                "fixture service exited"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn assert_local_storage_untouched(&self) {
        assert_eq!(
            fs::read_to_string(self.root.join("client/config/config.toml")).expect("local config"),
            "schema_version = 99\n"
        );
        assert!(!self.root.join("client/state").exists());
        assert!(!self.root.join("client/run").exists());
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if let Some(server) = self.server.as_mut() {
            let _ = server.kill();
            let _ = server.wait();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn client_uses_the_service_without_home_or_valid_local_configuration() {
    let mut sandbox = Sandbox::new();
    sandbox.start();
    let output = sandbox.client(&["status"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).expect("status JSON");
    assert_eq!(status["running"], true);
    assert_eq!(status["socket"], sandbox.socket.display().to_string());
    assert!(status["telemetry"].is_object());

    let output = sandbox.client(&["config", "show"]);
    assert!(output.status.success());
    let configuration: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("configuration JSON");
    assert_eq!(
        configuration["config_path"],
        sandbox.root.join("server/config/config.toml").display().to_string()
    );
    let output = sandbox.client(&["config", "set", "server.max_concurrency", "9"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        fs::read_to_string(sandbox.root.join("server/config/config.toml"))
            .expect("server config")
            .contains("max_concurrency = 9")
    );
    assert!(sandbox.client(&["config", "validate"]).status.success());
    assert!(sandbox.client(&["model", "list"]).status.success());

    let output = sandbox.client(&["prompt", "--prompt", "hello", "--no-stream"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("server-default"));
    assert!(
        sandbox.client(&["status"]).status.success(),
        "clients must leave the service running"
    );
    sandbox.assert_local_storage_untouched();
}

#[test]
fn unavailable_explicit_target_never_falls_back_or_reports_a_stopped_personal_server() {
    let sandbox = Sandbox::new();
    for args in [
        vec!["status"],
        vec!["config", "show"],
        vec!["config", "set", "server.max_concurrency", "9"],
        vec!["config", "remove", "server.api_key"],
        vec!["model", "list"],
        vec!["prompt", "--prompt", "hello"],
        vec![],
    ] {
        let output = sandbox.client(&args);
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("cannot attach"), "{error}");
        assert!(error.contains("no local runtime was started"), "{error}");
    }
    assert!(!sandbox.socket.parent().expect("socket directory").exists());
    sandbox.assert_local_storage_untouched();
}
