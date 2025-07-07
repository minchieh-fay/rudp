use rudpbase::Rudpbase;
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Rudpbase basic usage example...");

    let addr1: SocketAddr = "127.0.0.1:8080".parse()?;
    let addr2: SocketAddr = "127.0.0.1:8081".parse()?;

    let mut rudp1 = Rudpbase::new(addr1).await?;
    let mut rudp2 = Rudpbase::new(addr2).await?;

    // Server loop (rudp2)
    tokio::spawn(async move {
        loop {
            rudp2.tick().await;
            
            if let Some(received) = rudp2.recv().await {
                match received.result {
                    Ok(buffer) => {
                        println!("Server received from {}: {:?}", 
                            received.from, 
                            String::from_utf8_lossy(buffer.data()));
                        
                        // Echo back
                        let mut response_buffer = rudp2.get_buffer().unwrap();
                        let response = b"Echo: ";
                        response_buffer.data_mut()[..response.len()].copy_from_slice(response);
                        response_buffer.data_mut()[response.len()..response.len() + buffer.data().len()]
                            .copy_from_slice(buffer.data());
                        response_buffer.set_data_len(response.len() + buffer.data().len()).unwrap();
                        
                        if let Err(e) = rudp2.send(response_buffer, received.from).await {
                            println!("Server send error: {}", e);
                        }
                    }
                    Err(e) => {
                        println!("Server receive error: {}", e);
                    }
                }
            }
            
            sleep(Duration::from_millis(1)).await;
        }
    });

    // Client (rudp1)
    sleep(Duration::from_millis(100)).await; // Give server time to start

    for i in 0..5 {
        // Send message
        let mut buffer = rudp1.get_buffer()?;
        let message = format!("Hello from client {}", i);
        let message_bytes = message.as_bytes();
        buffer.data_mut()[..message_bytes.len()].copy_from_slice(message_bytes);
        buffer.set_data_len(message_bytes.len())?;
        
        if let Err(e) = rudp1.send(buffer, addr2).await {
            println!("Client send error: {}", e);
        } else {
            println!("Client sent: {}", message);
        }

        // Wait for response
        let mut response_received = false;
        for _ in 0..100 { // Wait up to 100ms
            rudp1.tick().await;
            
            if let Some(received) = rudp1.recv().await {
                match received.result {
                    Ok(buffer) => {
                        println!("Client received from {}: {:?}", 
                            received.from, 
                            String::from_utf8_lossy(buffer.data()));
                        response_received = true;
                        break;
                    }
                    Err(e) => {
                        println!("Client receive error: {}", e);
                    }
                }
            }
            
            sleep(Duration::from_millis(1)).await;
        }
        
        if !response_received {
            println!("Client: No response received for message {}", i);
        }
        
        sleep(Duration::from_millis(500)).await;
    }

         println!("Basic usage example completed!");
     Ok(())
} 