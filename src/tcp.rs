use futures::future::try_join;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::TcpListener;
use log::*;

struct TCPPeerPair {
    client: TcpStream,
    remote: String,
}

impl TCPPeerPair {
    async fn run(mut self) -> Result<(), std::io::Error>{
        let mut outbound = TcpStream::connect(self.remote.clone()).await?;

        let (mut ri, mut wi) = self.client.split();
        let (mut ro, mut wo) = outbound.split();
    
        let client_to_server = async {
            tokio::io::copy(&mut ri, &mut wo).await?;
            wo.shutdown().await
        };
    
        let server_to_client = async {
            tokio::io::copy(&mut ro, &mut wi).await?;
            wi.shutdown().await
        };
    
        try_join(client_to_server, server_to_client).await?;
        Ok(())
    }
}

struct TCPProxy<'a> {
    addr: &'a String,
    remote: &'a String,
}


impl<'a> TCPProxy<'a> {
    async fn run(self) -> Result<(), std::io::Error> {
        match TcpListener::bind(self.addr).await {
            Ok(listener) => {
                loop{

                    match listener.accept().await {
                        Ok((inbound, _)) => {
                            // let transfer = Self::transfer(inbound, self.remote.clone());
                            // tokio::spawn(transfer);
                            let client = TCPPeerPair{
                                client: inbound,
                                remote: self.remote.clone()
                            };
                            tokio::spawn(client.run());
                        },
                        Err(e1) => {
                            error!("Failed to accept new connection from {}, err={:?}", self.addr, e1);
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
    let server = TCPProxy {
        addr: &local,
        remote: &remote
    };
    info!("Start service in TCP mode {}->{}", server.addr, server.remote);
    return server.run().await;
}