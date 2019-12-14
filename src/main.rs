#![warn(rust_2018_idioms)]
extern crate getopts;
extern crate rand;
extern crate corona;
extern crate tokio;

use getopts::Options;
use std::collections::HashMap;
use std::{env, io};
// use std::net::UdpSocket;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

#[cfg(tcp)]
#[cfg(udp)]

use tokio::net::TcpListener;

use tokio::net::UdpSocket;

// use tokio::net::

fn usage(program: &str, opts: Options){
    let program_path = std::path::PathBuf::from(program);
    let program_name = program_path.file_stem().unwrap().to_str().unwrap();
    print!("Usage: {} [-b BIND_ADDR] -l LOCAL_PORT -h REMOTE_ADDR -r REMOTE_PORT\n", program_name);
}
#[tokio::main]
async fn main(){
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.reqopt("l",
                "local-port",
                "The local port",
                "LOCAL_PORT");
    opts.reqopt("r",
                "remote-port",
                "The remote port",
                "REMOTE_PORT");
    opts.reqopt("h",
                "host",
                "The remote address",
                "REMOTE_ADDR");
    opts.optopt("b",
                "bind",
                "The address to be listened",
                "BIND_ADDR");
    opts.optflag("d", "debug", "Enable debug mode");

    let matches = opts.parse(&args[1..])
        .unwrap_or_else(|_| {
            usage(&program, opts);
            std::process::exit(-1);
        });
}