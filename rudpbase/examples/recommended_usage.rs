use rudpbase::Rudpbase;
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Rudpbase recommended usage example...");

    let addr1: SocketAddr = "127.0.0.1:8082".parse()?;
    let addr2: SocketAddr = "127.0.0.1:8083".parse()?;

    let mut rudp1 = Rudpbase::new(addr1).await?;
    let mut rudp2 = Rudpbase::new(addr2).await?;

    // Server task (rudp2)
    tokio::spawn(async move {
        let mut message_count = 0;
        
        loop {
            rudp2.tick().await;
            
            if let Some(received) = rudp2.recv().await {
                match received.result {
                    Ok(buffer) => {
                        message_count += 1;
                        println!("Server received message #{} from {}: {:?}", 
                            message_count,
                            received.from, 
                            String::from_utf8_lossy(buffer.data()));
                        
                        // Send acknowledgment
                        let mut response_buffer = rudp2.get_buffer().unwrap();
                        let response = format!("ACK #{}", message_count);
                        let response_bytes = response.as_bytes();
                        response_buffer.data_mut()[..response_bytes.len()].copy_from_slice(response_bytes);
                        response_buffer.set_data_len(response_bytes.len()).unwrap();
                        
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

    // Client task (rudp1)
    sleep(Duration::from_millis(100)).await; // Give server time to start

    println!("Client starting to send messages...");

    for i in 1..=10 {
        // Prepare message
        let mut buffer = rudp1.get_buffer()?;
        let message = format!("Message #{} from client", i);
        let message_bytes = message.as_bytes();
        buffer.data_mut()[..message_bytes.len()].copy_from_slice(message_bytes);
        buffer.set_data_len(message_bytes.len())?;
        
        // Send message
        if let Err(e) = rudp1.send(buffer, addr2).await {
            println!("Client send error: {}", e);
            continue;
        }
        
        println!("Client sent: {}", message);

        // Wait for acknowledgment
        let mut ack_received = false;
        for _ in 0..200 { // Wait up to 200ms
            rudp1.tick().await;
            
            if let Some(received) = rudp1.recv().await {
                match received.result {
                    Ok(buffer) => {
                        println!("Client received ACK from {}: {:?}", 
                            received.from, 
                            String::from_utf8_lossy(buffer.data()));
                        ack_received = true;
                        break;
                    }
                    Err(e) => {
                        println!("Client receive error: {}", e);
                    }
                }
            }
            
            sleep(Duration::from_millis(1)).await;
        }
        
        if !ack_received {
            println!("Client: No ACK received for message {}", i);
        }
        
        // Print connection statistics
        if let Some(stats) = rudp1.get_stats(addr2) {
            println!("Connection stats: sent={}, received={}, lost={}, retransmissions={}", 
                stats.packets_sent, stats.packets_received, stats.packets_lost, stats.retransmissions);
        }
        
        sleep(Duration::from_millis(300)).await;
    }

    println!("Recommended usage example completed!");
    Ok(())
} 