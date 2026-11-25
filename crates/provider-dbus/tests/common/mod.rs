//! Private-bus test harness.
//!
//! Every test gets its own `dbus-daemon` with no service directories, no
//! activation, and no system services. The daemon's address is exported as
//! `DBUS_SYSTEM_BUS_ADDRESS`, which is exactly the path the foundation uses.
//! Tests are serialized because the address is process-wide.
//!
//! The harness provides:
//!
//! * [`Harness`] — the daemon plus the serialization guard.
//! * [`WireMonitor`] — a `BecomeMonitor` connection that records every message
//!   on the private bus, including flags, signatures, and selected bodies.
//! * [`MockService`] — a raw zbus mock that owns one service name and answers
//!   `Properties.Get` according to a per-test table.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use futures::StreamExt;
use zbus::message::Type;
use zbus::names::OwnedUniqueName;
use zbus::{Connection, Message, MessageStream, OwnedObjectPath, Variant};

use araiguma_provider_dbus::policy::{
    NETWORK_MANAGER_ACTIVE_CONNECTION_PATH_PREFIX, NETWORK_MANAGER_DEVICE_PATH_PREFIX,
    NETWORK_MANAGER_MANAGER_INTERFACE, NETWORK_MANAGER_MANAGER_PATH, NETWORK_MANAGER_SERVICE,
    SYSTEMD_RESOLVED_MANAGER_PATH, SYSTEMD_RESOLVED_SERVICE,
};
use araiguma_provider_dbus::Service;

/// Messages whose mere presence would prove a forbidden operation.
pub const PROHIBITED_MEMBERS: &[&str] = &[
    "GetAll",
    "Set",
    "GetManagedObjects",
    "Introspect",
    "GetSettings",
    "GetSecrets",
    "StartServiceByName",
    "ListNames",
    "ListActivatableNames",
    "GetDevices",
    "GetAllDevices",
    "ActivateConnection",
    "AddAndActivateConnection",
    "DeactivateConnection",
    "CheckConnectivity",
    "Reload",
    "Sleep",
];

static TEST_LOCK: Mutex<()> = Mutex::new(());

/// One private bus and the exclusive right to use it.
pub struct Harness {
    bus: PrivateBus,
    _guard: MutexGuard<'static, ()>,
}

impl Harness {
    /// Start a fresh private bus and make it the system-bus address.
    pub fn start() -> Self {
        let guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let bus = PrivateBus::start();
        Self { bus, _guard: guard }
    }

    /// The address of the private bus.
    pub fn address(&self) -> &str {
        &self.bus.address
    }

    /// Start a wire monitor before any other connection is made.
    pub async fn monitor(&self) -> WireMonitor {
        WireMonitor::start().await
    }
}

struct PrivateBus {
    dir: PathBuf,
    child: Child,
    address: String,
}

impl PrivateBus {
    fn start() -> Self {
        let dir = unique_dir();
        std::fs::create_dir_all(&dir).expect("create private bus directory");
        let socket = dir.join("bus.sock");
        let config = dir.join("bus.conf");
        let config_text = format!(
            r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>system</type>
  <listen>unix:path={socket}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow user="*"/>
    <allow own="*"/>
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow eavesdrop="true"/>
  </policy>
</busconfig>
"#,
            socket = socket.display()
        );
        std::fs::write(&config, config_text).expect("write private bus config");

        let mut child = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .arg("--nofork")
            .arg("--print-address=1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("dbus-daemon must be installed for the private-bus tests");

        let stdout = child.stdout.take().expect("dbus-daemon stdout");
        let mut reader = BufReader::new(stdout);
        let mut address = String::new();
        reader
            .read_line(&mut address)
            .expect("read dbus-daemon address");
        let address = address.trim().to_owned();
        assert!(
            address.starts_with("unix:path="),
            "unexpected private bus address"
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        while !socket.exists() {
            assert!(
                Instant::now() < deadline,
                "private bus socket did not appear"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        std::env::set_var("DBUS_SYSTEM_BUS_ADDRESS", &address);

        Self {
            dir,
            child,
            address,
        }
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn unique_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir()
        .join("opencode")
        .join("araiguma-dbus-tests");
    base.join(format!("{}-{sequence}", std::process::id()))
}

/// A message observed on the wire.
#[derive(Clone, Debug)]
pub struct WireMessage {
    pub kind: String,
    pub sender: Option<String>,
    pub destination: Option<String>,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub signature: String,
    pub flags: u8,
    pub body: Option<Vec<String>>,
}

/// Where a message was addressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    BusDriver,
    Application,
    Other,
}

impl WireMessage {
    fn capture(message: &Message) -> Self {
        let header = message.header();
        Self {
            kind: format!("{:?}", message.message_type()),
            sender: header.sender().map(|value| value.as_str().to_owned()),
            destination: header.destination().map(|value| value.as_str().to_owned()),
            path: header.path().map(|value| value.as_str().to_owned()),
            interface: header.interface().map(|value| value.as_str().to_owned()),
            member: header.member().map(|value| value.as_str().to_owned()),
            signature: header.signature().to_string(),
            flags: header.primary().flags().bits(),
            body: capture_body(message),
        }
    }

    pub fn origin(&self) -> Origin {
        match self.destination.as_deref() {
            Some("org.freedesktop.DBus") => Origin::BusDriver,
            Some(NETWORK_MANAGER_SERVICE) | Some(SYSTEMD_RESOLVED_SERVICE) => Origin::Application,
            _ => Origin::Other,
        }
    }

    pub fn is_method_call(&self) -> bool {
        self.kind == "MethodCall"
    }

    pub fn is_application_call(&self) -> bool {
        self.is_method_call() && self.origin() == Origin::Application
    }

    pub fn is_bus_driver_call(&self) -> bool {
        self.is_method_call() && self.origin() == Origin::BusDriver
    }

    pub fn member_is(&self, member: &str) -> bool {
        self.member.as_deref() == Some(member)
    }

    /// Compare the parsed signature against `expected`.
    ///
    /// zbus renders a top-level two-field signature as `(ss)`; the wire
    /// signature is `ss`. Comparing parsed values avoids depending on the
    /// rendering while still asserting the exact types.
    pub fn signature_is(&self, expected: &str) -> bool {
        use std::str::FromStr;
        zbus::Signature::from_str(expected)
            .map(|signature| signature.to_string() == self.signature)
            .unwrap_or(false)
    }
}

fn capture_body(message: &Message) -> Option<Vec<String>> {
    let member = message.header().member()?.as_str().to_owned();
    match member.as_str() {
        "Get" => message
            .body()
            .deserialize::<(String, String)>()
            .ok()
            .map(|(interface, property)| vec![interface, property]),
        "AddMatch" | "NameHasOwner" | "GetNameOwner" => message
            .body()
            .deserialize::<(String,)>()
            .ok()
            .map(|(value,)| vec![value]),
        "RequestName" => message
            .body()
            .deserialize::<(String, u32)>()
            .ok()
            .map(|(name, flags)| vec![name, flags.to_string()]),
        "NameOwnerChanged" => message
            .body()
            .deserialize::<(String, String, String)>()
            .ok()
            .map(|(name, old_owner, new_owner)| vec![name, old_owner, new_owner]),
        _ => None,
    }
}

/// A monitor connection that records all private-bus traffic.
pub struct WireMonitor {
    messages: Arc<Mutex<Vec<WireMessage>>>,
    task: tokio::task::JoinHandle<()>,
}

impl WireMonitor {
    pub async fn start() -> Self {
        let connection = Connection::system()
            .await
            .expect("monitor connection to private bus");
        let monitoring = zbus::fdo::MonitoringProxy::new(&connection)
            .await
            .expect("monitor proxy");
        monitoring
            .become_monitor(&[], 0)
            .await
            .expect("become monitor");
        let mut stream = MessageStream::from(connection);
        let messages = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&messages);
        let task = tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(message) => sink
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .push(WireMessage::capture(&message)),
                    Err(_) => break,
                }
            }
        });
        Self { messages, task }
    }

    pub fn snapshot(&self) -> Vec<WireMessage> {
        self.messages
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn clear(&self) {
        self.messages
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    pub fn application_calls(&self) -> Vec<WireMessage> {
        self.snapshot()
            .into_iter()
            .filter(WireMessage::is_application_call)
            .collect()
    }

    pub fn bus_driver_calls(&self) -> Vec<WireMessage> {
        self.snapshot()
            .into_iter()
            .filter(WireMessage::is_bus_driver_call)
            .collect()
    }

    pub async fn wait_for<F>(&self, timeout: Duration, predicate: F) -> bool
    where
        F: Fn(&WireMessage) -> bool,
    {
        let deadline = Instant::now() + timeout;
        loop {
            if self.snapshot().iter().any(&predicate) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn wait_for_count<F>(&self, timeout: Duration, count: usize, predicate: F) -> bool
    where
        F: Fn(&WireMessage) -> bool,
    {
        let deadline = Instant::now() + timeout;
        loop {
            if self
                .snapshot()
                .iter()
                .filter(|message| predicate(message))
                .count()
                >= count
            {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Fail if any prohibited member was ever sent, or if any application
    /// call other than an allow-listed `Properties.Get` appeared.
    pub fn assert_clean(&self, allowed_application_gets: usize) {
        let messages = self.snapshot();
        for message in &messages {
            if let Some(member) = &message.member {
                assert!(
                    !PROHIBITED_MEMBERS.contains(&member.as_str()),
                    "prohibited member {member} appeared on the wire"
                );
            }
            if message.member_is("AddMatch") {
                if let Some(rule) = message.body.as_ref().and_then(|body| body.first()) {
                    assert!(
                        !rule.contains("NameOwnerChanged"),
                        "forbidden NameOwnerChanged subscription: {rule}"
                    );
                    assert!(
                        !rule.contains("PropertiesChanged"),
                        "forbidden PropertiesChanged subscription: {rule}"
                    );
                }
            }
        }

        let application_calls: Vec<_> = messages
            .iter()
            .filter(|message| message.is_application_call())
            .collect();
        for message in &application_calls {
            assert_eq!(
                message.interface.as_deref(),
                Some("org.freedesktop.DBus.Properties"),
                "unexpected application interface"
            );
            assert_eq!(
                message.member.as_deref(),
                Some("Get"),
                "unexpected application method"
            );
            assert_eq!(
                message.flags & 0x2,
                0x2,
                "application call did not carry NO_AUTO_START"
            );
            assert!(
                message.signature_is("ss"),
                "unexpected Get signature: {}",
                message.signature
            );
        }
        assert_eq!(
            application_calls.len(),
            allowed_application_gets,
            "unexpected number of application calls"
        );
    }
}

impl Drop for WireMonitor {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// One programmable `Properties.Get` reply.
pub enum MockReply {
    Value(Variant<'static>),
    Error(&'static str),
    Delay(Duration, Variant<'static>),
    Silent,
}

/// The reply table, keyed by `(interface, property)`.
pub type MockReplies = HashMap<(&'static str, &'static str), MockReply>;

#[derive(Clone, Debug)]
pub struct MockCall {
    pub path: String,
    pub interface: String,
    pub member: String,
    pub signature: String,
    pub flags: u8,
    pub body: Option<Vec<String>>,
}

impl MockCall {
    pub fn signature_is(&self, expected: &str) -> bool {
        use std::str::FromStr;
        zbus::Signature::from_str(expected)
            .map(|signature| signature.to_string() == self.signature)
            .unwrap_or(false)
    }
}

/// A raw mock service that owns one of the two approved names.
pub struct MockService {
    conn: Connection,
    calls: Arc<Mutex<Vec<MockCall>>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockService {
    pub async fn start(service: Service, replies: MockReplies) -> Self {
        let conn = Connection::system()
            .await
            .expect("mock service connection to private bus");
        conn.request_name(service.bus_name())
            .await
            .expect("mock service must own its name");

        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&calls);
        let task_conn = conn.clone();
        let mut stream = MessageStream::from(task_conn.clone());
        let task = tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                let Ok(message) = item else { break };
                if message.message_type() != Type::MethodCall {
                    continue;
                }

                let header = message.header();
                let interface = header
                    .interface()
                    .map(|value| value.as_str().to_owned())
                    .unwrap_or_default();
                let member = header
                    .member()
                    .map(|value| value.as_str().to_owned())
                    .unwrap_or_default();
                let path = header
                    .path()
                    .map(|value| value.as_str().to_owned())
                    .unwrap_or_default();
                let signature = header.signature().to_string();
                let flags = header.primary().flags().bits();
                let body = capture_body(&message);
                drop(header);

                recorded
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(MockCall {
                        path,
                        interface: interface.clone(),
                        member: member.clone(),
                        signature,
                        flags,
                        body: body.clone(),
                    });

                if interface != "org.freedesktop.DBus.Properties" || member != "Get" {
                    let reply = Message::error(
                        &message.header(),
                        "org.freedesktop.DBus.Error.UnknownMethod",
                    )
                    .build(&("unsupported mock method",))
                    .expect("build mock error");
                    let _ = task_conn.send(&reply).await;
                    continue;
                }

                let Some(parts) = body else {
                    let reply =
                        Message::error(&message.header(), "org.freedesktop.DBus.Error.InvalidArgs")
                            .build(&("bad Get body",))
                            .expect("build mock error");
                    let _ = task_conn.send(&reply).await;
                    continue;
                };
                let Some((requested_interface, rest)) = parts.split_first() else {
                    continue;
                };
                let Some(property) = rest.first() else {
                    continue;
                };
                let reply = replies
                    .get(&(requested_interface.as_str(), property.as_str()))
                    .unwrap_or(&MockReply::Error(
                        "org.freedesktop.DBus.Error.UnknownProperty",
                    ));

                match reply {
                    MockReply::Value(value) => {
                        let reply = Message::method_return(&message.header())
                            .build(value)
                            .expect("build property reply");
                        let _ = task_conn.send(&reply).await;
                    }
                    MockReply::Delay(delay, value) => {
                        let reply = Message::method_return(&message.header())
                            .build(value)
                            .expect("build property reply");
                        tokio::time::sleep(*delay).await;
                        let _ = task_conn.send(&reply).await;
                    }
                    MockReply::Error(name) => {
                        let reply = Message::error(&message.header(), *name)
                            .build(&("mock service error detail",))
                            .expect("build error reply");
                        let _ = task_conn.send(&reply).await;
                    }
                    MockReply::Silent => {}
                }
            }
        });

        Self { conn, calls, task }
    }

    pub fn calls(&self) -> Vec<MockCall> {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn clear_calls(&self) {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    pub async fn emit_state_changed(&self, state: u32) {
        self.conn
            .emit_signal(
                None::<&str>,
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "StateChanged",
                &(state,),
            )
            .await
            .expect("emit StateChanged");
    }

    pub async fn emit_device_added(&self, path: &str) {
        self.conn
            .emit_signal(
                None::<&str>,
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "DeviceAdded",
                &(OwnedObjectPath::try_from(path).expect("device path"),),
            )
            .await
            .expect("emit DeviceAdded");
    }

    pub async fn emit_device_removed(&self, path: &str) {
        self.conn
            .emit_signal(
                None::<&str>,
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "DeviceRemoved",
                &(OwnedObjectPath::try_from(path).expect("device path"),),
            )
            .await
            .expect("emit DeviceRemoved");
    }

    pub async fn emit_properties_changed(&self) {
        self.conn
            .emit_signal(
                None::<&str>,
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "PropertiesChanged",
                &(
                    "org.freedesktop.NetworkManager",
                    HashMap::<String, Variant<'_>>::new(),
                    Vec::<String>::new(),
                ),
            )
            .await
            .expect("emit PropertiesChanged");
    }

    pub async fn shutdown(self) {
        self.task.abort();
        let _ = self.conn.clone().close().await;
    }
}

impl Drop for MockService {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub fn nm_device_path(index: u32) -> String {
    format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}{index}")
}

pub fn nm_active_connection_path(index: u32) -> String {
    format!("{NETWORK_MANAGER_ACTIVE_CONNECTION_PATH_PREFIX}{index}")
}

pub fn resolved_link_path(index: u32) -> String {
    format!("{SYSTEMD_RESOLVED_MANAGER_PATH}/link/{index}")
}

pub fn object_paths(paths: &[String]) -> Variant<'static> {
    let paths: Vec<OwnedObjectPath> = paths
        .iter()
        .map(|path| OwnedObjectPath::try_from(path.as_str()).expect("object path"))
        .collect();
    Variant::from(paths)
}

pub fn no_auto_start() -> u8 {
    0x2
}

pub fn unique_owner(name: &str) -> OwnedUniqueName {
    OwnedUniqueName::try_from(name).expect("unique name")
}

/// Wait until the daemon reports the service name as unowned.
///
/// This is a test-side bus-driver query, used to make service-disappearance
/// tests deterministic without sleeping on an arbitrary delay.
pub async fn wait_for_service_absence(service: Service) {
    let connection = Connection::system()
        .await
        .expect("absence-check connection to private bus");
    let bus = zbus::proxy::Builder::<zbus::Proxy<'_>>::new(&connection)
        .destination("org.freedesktop.DBus")
        .path("/org/freedesktop/DBus")
        .interface("org.freedesktop.DBus")
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .expect("absence-check proxy");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let owned: bool = bus
            .call("NameHasOwner", &(service.bus_name(),))
            .await
            .expect("NameHasOwner");
        if !owned {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "service name was not released in time"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

pub fn find_get<'a>(calls: &'a [MockCall], property: &str) -> &'a MockCall {
    calls
        .iter()
        .find(|call| {
            call.member == "Get"
                && call
                    .body
                    .as_ref()
                    .and_then(|body| body.get(1))
                    .map(String::as_str)
                    == Some(property)
        })
        .unwrap_or_else(|| panic!("no Properties.Get for {property}"))
}

pub fn find_add_match_rule<'a>(messages: &'a [WireMessage], member: &str) -> Option<&'a str> {
    messages
        .iter()
        .filter(|message| message.is_bus_driver_call() && message.member_is("AddMatch"))
        .filter_map(|message| message.body.as_ref())
        .filter_map(|body| body.first())
        .find(|rule| rule.contains(&format!("member='{member}'")))
        .map(String::as_str)
}

pub fn is_executable(path: &Path) -> bool {
    path.exists()
}
