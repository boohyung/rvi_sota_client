#[macro_use] extern crate log;
extern crate env_logger;
extern crate getopts;
extern crate hyper;
extern crate ws;
extern crate rustc_serialize;
#[macro_use] extern crate libotaplus;

use getopts::Options;
use hyper::Url;
use std::env;

use libotaplus::auth_plus::authenticate;
use libotaplus::datatype::{config, Config, PackageManager as PackageManagerType, Event, Command};
use libotaplus::ui::spawn_websocket_server;
use libotaplus::http_client::HttpClient;
use libotaplus::package_manager::Dpkg;
use libotaplus::read_interpret::ReplEnv;
use libotaplus::read_interpret;
use libotaplus::pubsub;
use libotaplus::interpreter::Interpreter;

use rustc_serialize::json;
use std::sync::mpsc::{Sender, Receiver, channel};

use std::thread;
use std::time::Duration;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ws::{Sender as WsSender};

macro_rules! spawn_thread {
    ($name:expr, $body:block) => {
        {
            match thread::Builder::new().name($name.to_string()).spawn(move || {
                info!("Spawning {}", $name.to_string());
                $body
            }) {
                Err(e) => panic!("Couldn't spawn {}: {}", $name, e),
                Ok(handle) => handle
            }
        }
    }
}

fn main() {

    env_logger::init().unwrap();

    let config = build_config();
    let config2 = config.clone();
    let config3 = config.clone();

    info!("Authenticating against AuthPlus...");
    let _ = authenticate::<hyper::Client>(&config.auth).map(|token| {
        let (etx, erx): (Sender<Event>, Receiver<Event>) = channel();
        let (ctx, crx): (Sender<Command>, Receiver<Command>) = channel();

        let mut registry = pubsub::Registry::new(erx);

        {
            let events_for_autoacceptor = registry.subscribe();
            let ctx_ = ctx.clone();
            spawn_thread!("Autoacceptor of software updates", {
                fn dispatch(ev: &Event, outlet: Sender<Command>) {
                    match ev {
                        &Event::NewUpdateAvailable(ref id) => {
                            let _ = outlet.send(Command::AcceptUpdate(id.clone()));
                        },
                        &Event::Batch(ref evs) => {
                            for ev in evs {
                                dispatch(ev, outlet.clone())
                            }
                        },
                        _ => {}
                    }
                };
                loop {
                    dispatch(&events_for_autoacceptor.recv().unwrap(), ctx_.clone())
                }
            });
        }

        spawn_thread!("Interpreter", {
            Interpreter::<hyper::Client>::new(&config2, token.clone(), crx, etx).start();
        });

        let events_for_ws = registry.subscribe();
        {
            let all_clients = Arc::new(Mutex::new(HashMap::new()));
            let all_clients_ = all_clients.clone();
            spawn_thread!("Websocket Event Broadcast", {
                loop {
                    let event = events_for_ws.recv().unwrap();
                    let clients = all_clients_.lock().unwrap().clone();
                    for (_, client) in clients {
                        let x: WsSender = client;
                        let _ = x.send(json::encode(&event).unwrap());
                    }
                }
            });

            let ctx_ = ctx.clone();
            spawn_thread!("Websocket Server", {
                let _ = spawn_websocket_server("0.0.0.0:9999", ctx_, all_clients);
            });
        }


        {
            let ctx_ = ctx.clone();
            spawn_thread!("Update poller", {
                loop {
                    let _ = ctx_.send(Command::GetPendingUpdates);
                    thread::sleep(Duration::from_secs(config3.ota.polling_interval));
                }
            });
        }

        spawn_thread!("PubSub Registry", { registry.start(); });

        // Perform initial sync
        let _ = ctx.clone().send(Command::PostInstalledPackages);

        thread::sleep(Duration::from_secs(60000000));
    });

    if config.test.looping {
        read_interpret::read_interpret_loop(ReplEnv::new(Dpkg));
    }

}

fn build_config() -> Config {

    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optflag("h", "help",
                 "print this help menu");
    opts.optopt("", "config",
                "change config path", "PATH");
    opts.optopt("", "auth-server",
                "change the auth server URL", "URL");
    opts.optopt("", "auth-client-id",
                "change auth client id", "ID");
    opts.optopt("", "auth-secret",
                "change auth secret", "SECRET");
    opts.optopt("", "ota-server",
                "change ota server URL", "URL");
    opts.optopt("", "ota-vin",
                "change ota vin", "VIN");
    opts.optopt("", "ota-packages-dir",
                "change downloaded directory for packages", "PATH");
    opts.optopt("", "ota-package-manager",
                "change package manager", "MANAGER");
    opts.optflag("", "test-looping",
                 "enable read-interpret test loop");

    let matches = opts.parse(&args[1..])
        .unwrap_or_else(|err| panic!(err.to_string()));

    if matches.opt_present("h") {
        let brief = format!("Usage: {} [options]", program);
        exit!("{}", opts.usage(&brief));
    }

    let mut config_file = env::var("OTA_PLUS_CLIENT_CFG")
        .unwrap_or("/opt/ats/ota/etc/ota.toml".to_string());

    if let Some(path) = matches.opt_str("config") {
        config_file = path;
    }

    let mut config = config::load_config(&config_file)
        .unwrap_or_else(|err| exit!("{}", err));

    if let Some(s) = matches.opt_str("auth-server") {
        match Url::parse(&s) {
            Ok(url)  => config.auth.server = url,
            Err(err) => exit!("Invalid auth-server URL: {}", err)
        }
    }

    if let Some(client_id) = matches.opt_str("auth-client-id") {
        config.auth.client_id = client_id;
    }

    if let Some(secret) = matches.opt_str("auth-secret") {
        config.auth.secret = secret;
    }

    if let Some(s) = matches.opt_str("ota-server") {
        match Url::parse(&s) {
            Ok(url)  => config.ota.server = url,
            Err(err) => exit!("Invalid ota-server URL: {}", err)
        }
    }

    if let Some(vin) = matches.opt_str("ota-vin") {
        config.ota.vin = vin;
    }

    if let Some(path) = matches.opt_str("ota-packages-dir") {
        config.ota.packages_dir = path;
    }

    if let Some(s) = matches.opt_str("ota-package-manager") {
        config.ota.package_manager = match s.to_lowercase().as_str() {
            "dpkg" => PackageManagerType::Dpkg,
            "rpm"  => PackageManagerType::Rpm,
            path   => PackageManagerType::File(path.to_string()),
        }
    }

    if matches.opt_present("test-looping") {
        config.test.looping = true;
    }

    return config
}

// Hack to build a binary with a predictable path for use in tests/. We
// can remove this when https://github.com/rust-lang/cargo/issues/1924
// is resolved.
#[test]
fn build_binary() {
    let output = std::process::Command::new("cargo")
        .arg("build")
        .output()
        .unwrap_or_else(|e| panic!("failed to execute child: {}", e));

    assert!(output.status.success())
}
