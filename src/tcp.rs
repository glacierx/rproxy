use futures::future::try_join;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::TcpListener;

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
        let listener = TcpListener::bind(self.addr).await?;

        while let Ok((inbound, _)) = listener.accept().await {
            // let transfer = Self::transfer(inbound, self.remote.clone());
            // tokio::spawn(transfer);
            let client = TCPPeerPair{
                client: inbound,
                remote: self.remote.clone()
            };
            tokio::spawn(client.run());
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
    return server.run().await;
}