#![recursion_limit="512"]
#![warn(rust_2018_idioms)]
#[cfg(tcp)]
#[cfg(udp)]

use std::sync::Arc;
use chrono::Local;
use std::path::PathBuf;
use std::path::Path;
use log::*;
use argh::FromArgs;
use log4rs;
mod tcp;
mod udp;
mod dns;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(FromArgs)]
#[argh(description = "rproxy is a platform independent UDP TCP high performance async proxy")]
struct Options {
    /// remote service endpoint (UDP & TCP)
    #[argh(option, short='r', default="\"\".to_string()")]
    remote: String,
    /// local endpoint to be binded by rproxy
    #[argh(option, short='b', default="\"\".to_string()")]
    bind: String,
    /// protocol of the remote service(UDP|TCP)
    #[argh(option, short='p', default="\"UDP\".to_string()")]
    protocol: String,
    /// set logger level to debug or not
    #[argh(switch, short='d')]
    debug: bool,

    /// logger settings rotating etc...
    #[argh(option, short = 'l', default = "\"fixtures/logger.yaml\".to_string()")]
    logger_settings: String,

    /// configuration for multiple proxy instances
    #[argh(option, short = 'c')]
    config: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Proxy {
    remote: String,
    bind: String,
    protocol: String
}

static MY_LOGGER: MyLogger = MyLogger;

struct MyLogger;

impl log::Log for MyLogger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            println!("[{}][{}] - {}", record.level(), Local::now(), record.args());
        }
    }
    fn flush(&self) {}
}

#[tokio::main]
async fn main(){
    let options: Options = argh::from_env();
    
    if Path::new(&options.logger_settings).exists(){
        log4rs::init_file(&options.logger_settings, 
            Default::default()).unwrap();
        debug!("NICE");
    } else {
        log::set_logger(&MY_LOGGER).unwrap();
        if options.debug {
            log::set_max_level(LevelFilter::Debug);
        } else {
            log::set_max_level(LevelFilter::Info);
        }
    }

    match options.config {
        None => {
            if options.protocol == "UDP"{
                udp::udp_proxy(&options.bind, &options.remote).await.unwrap();
            } else if options.protocol == "TCP" {
                tcp::tcp_proxy(&options.bind, &options.remote).await.unwrap();
            }
        },
        Some(_config) => {
            if _config.as_path().exists() {
                if let Ok(_path) = _config.to_owned().into_os_string().into_string(){
                    let content = std::fs::read_to_string(_path).unwrap();
                    match serde_json::from_str::<Vec<Proxy>>(content.as_str()){
                        Ok(proxies) => {
                            let mut procs = vec![];
                            for i in 0..proxies.len() {
                                let bind = proxies[i].bind.to_owned();
                                let remote = proxies[i].remote.to_owned();
                                if proxies[i].protocol == "UDP"{
                                    procs.push(tokio::spawn(async move {udp::udp_proxy(&bind, &remote).await}))
                                } else if proxies[i].protocol == "TCP" {
                                    procs.push(tokio::spawn(async move {tcp::tcp_proxy(&bind, &remote).await}))
                                }
                            }
                            futures::future::join_all(procs).await;
                        },
                        Err(e) => {
                            error!("Failed to parse configuration json {:?}", e);
                        }
                    }
                }

            } else {
                error!("Invalid configuration file path {}", _config.as_path().display());
            }
        }
    }
}