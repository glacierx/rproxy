use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::TcpListener;
use log::*;
use tokio::time;

use crate::dns::DNSResolve;
use futures::future::try_join;

struct TCPPeerPair {
    client: TcpStream,
    remote: String,
}

impl TCPPeerPair {
    async fn run(mut self) -> Result<(), std::io::Error>{
        // let mut outbound = TcpStream::connect(self.remote.clone()).await?;

        // let (mut ri, mut wi) = self.client.split();
        // let (mut ro, mut wo) = outbound.split();
    
        // let client_to_server = async {
        //     tokio::io::copy(&mut ri, &mut wo).await?;
        //     wo.shutdown().await
        // };
    
        // let server_to_client = async {
        //     tokio::io::copy(&mut ro, &mut wi).await?;
        //     wi.shutdown().await
        // };
        // try_join(client_to_server, server_to_client).await?;
        let mut outbound = TcpStream::connect(self.remote.clone()).await?;
        tokio::io::copy_bidirectional(&mut self.client, &mut outbound).await?;    
        outbound.shutdown().await?;
        self.client.shutdown().await?;
        Ok(())
    }
}
struct TCPProxy<'a> {
    addr: &'a String,
    remote: &'a String,
    dns: Vec<String>
}

impl<'a> DNSResolve<'a> for TCPProxy<'a> {
    fn remote(&self) -> &String{
        self.remote
    }
    fn dns(&self) -> &Vec<String>{
        &self.dns
    }
    fn reset_dns(&mut self,d: &Vec<String>) -> usize {
        self.dns = d.to_vec();
        self.dns.len()
    }

}
impl<'a> TCPProxy<'a> {

    async fn run(& mut self) -> Result<(), std::io::Error> {
        self.resolve().await.unwrap();
        let mut time_out1 = time::interval(tokio::time::Duration::from_secs(30));
        let mut host = self.dns[0].clone();
        match TcpListener::bind(self.addr).await {
            Ok(listener) => {
                loop{
                    tokio::select!{
                        x = listener.accept() => {
                            match x {
                                Ok((inbound, _)) => {
                                    // let transfer = Self::transfer(inbound, self.remote.clone());
                                    // tokio::spawn(transfer);
                                    let client = TCPPeerPair{
                                        client: inbound,
                                        remote: host.clone()
                                    };
                                    tokio::spawn(client.run());
                                },
                                Err(e1) => {
                                    error!("Failed to accept new connection from {}, err={:?}", self.addr, e1);
                                }
                            }                            
                        },
                        _ = time_out1.tick() => {
                            self.resolve().await.unwrap();
                            host = self.dns[0].clone();
                        }
                    }
                }
            },
            Err(e) => {
                error!("Failed to bind interface {}, err={:?}", self.addr, e);
            }
        }

        Ok(())
    }
}

pub async fn tcp_proxy(local: &String, 
    remote:&String) -> Result<(), std::io::Error>
{
    let mut server = TCPProxy {
        addr: &local,
        remote: &remote,
        dns: vec![]
    };
    info!("Start service in TCP mode {}->{}", server.addr, server.remote);
    return server.run().await;
}