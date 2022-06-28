#![recursion_limit="512"]
#![warn(rust_2018_idioms)]
#[cfg(tcp)]
#[cfg(udp)]

use std::sync::Arc;
use std::path::PathBuf;
use std::path::Path;
use log::*;
use argh::FromArgs;
use log4rs;
mod tcp;
mod udp;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(FromArgs)]
#[argh(description = "rproxy is a platform independent UDP TCP high performance async proxy")]
struct Options {
    /// remote service endpoint (UDP & TCP)
    #[argh(option, short='r')]
    remote: String,
    /// local endpoint to be binded by rproxy
    #[argh(option, short='b')]
    bind: String,
    /// protocol of the remote service(UDP|TCP)
    #[argh(option, short='p', default="\"UDP\".to_string()")]
    protocol: String,
    /// set logger level to debug or not
    #[argh(option, short='d', default="true")]
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



#[tokio::main]
async fn main(){
    let options: Options = argh::from_env();
    
    if Path::new(&options.logger_settings).exists(){
        log4rs::init_file(&options.logger_settings, 
            Default::default()).unwrap();
        debug!("NICE");
    }

    match options.config {
        None => {
            if options.protocol == "UDP"{
                info!("Start service in UDP mode.");
                udp::udp_proxy(&options.bind, &options.remote).await.unwrap();
            } else if options.protocol == "TCP" {
                info!("Start service in TCP mode.");
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