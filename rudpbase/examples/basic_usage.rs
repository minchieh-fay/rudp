use rudpbase::new_rudpbase;
use std::time::Duration;
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create two Rudpbase instances for demonstration
    let mut rudp1 = new_rudpbase("127.0.0.1:8080".parse()?).await?;
    let mut rudp2 = new_rudpbase("127.0.0.1:8081".parse()?).await?;

    println!("Rudpbase instances created successfully!");
    println!("Instance 1: 127.0.0.1:8080");
    println!("Instance 2: 127.0.0.1:8081");

    // Spawn a task to handle instance 2 (receiver)
    let handle = tokio::spawn(async move {
        loop {
            // Maintenance
            rudp2.tick().await;
            
            // Check for incoming data
            if let Some(rbuffer) = rudp2.poll_read().await {
                match rbuffer.result {
                    Ok(buffer) => {
                        let message = String::from_utf8_lossy(buffer.as_slice());
                        println!("Received from {}: {}", rbuffer.from, message);
                        
                        // Send response using buffer pool (recommended)
                        let response = format!("Echo: {}", message);
                        match rudp2.get_buffer() {
                            Ok(mut buffer) => {
                                let response_bytes = response.as_bytes();
                                buffer.data_mut()[..response_bytes.len()].copy_from_slice(response_bytes);
                                if let Err(e) = buffer.set_data_len(response_bytes.len()) {
                                    eprintln!("Failed to set buffer length: {}", e);
                                } else if let Err(e) = rudp2.write(buffer, rbuffer.from).await {
                                    eprintln!("Failed to send response: {}", e);
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to get buffer: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Receive error: {}", e);
                    }
                }
            }
            
            time::sleep(Duration::from_millis(1)).await;
        }
    });

    // Send some test messages from instance 1
    let target = "127.0.0.1:8081".parse()?;
    
    for i in 1..=5 {
        let message = format!("Hello from instance 1, message {}", i);
        println!("Sending: {}", message);
        
        // Use buffer pool for sending (recommended)
        match rudp1.get_buffer() {
            Ok(mut buffer) => {
                let message_bytes = message.as_bytes();
                buffer.data_mut()[..message_bytes.len()].copy_from_slice(message_bytes);
                if let Err(e) = buffer.set_data_len(message_bytes.len()) {
                    eprintln!("Failed to set buffer length: {}", e);
                } else if let Err(e) = rudp1.write(buffer, target).await {
                    eprintln!("Failed to send message: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Failed to get buffer: {}", e);
            }
        }
        
        time::sleep(Duration::from_millis(100)).await;
    }

    // Give some time for message exchange
    for _ in 0..50 {
        rudp1.tick().await;
        
        // Check for responses
        if let Some(rbuffer) = rudp1.poll_read().await {
            match rbuffer.result {
                Ok(buffer) => {
                    let message = String::from_utf8_lossy(buffer.as_slice());
                    println!("Response from {}: {}", rbuffer.from, message);
                }
                Err(e) => {
                    eprintln!("Receive error: {}", e);
                }
            }
        }
        
        time::sleep(Duration::from_millis(10)).await;
    }

    // Show connection statistics
    if let Some(stats) = rudp1.get_stats(target) {
        println!("\nConnection Statistics:");
        println!("  Packets sent: {}", stats.packets_sent);
        println!("  Packets received: {}", stats.packets_received);
        println!("  Packets lost: {}", stats.packets_lost);
        println!("  Retransmissions: {}", stats.retransmissions);
        println!("  Average RTT: {:?}", stats.avg_rtt);
        println!("  Packet loss rate: {:.2}%", stats.packet_loss_rate() * 100.0);
    }

    println!("\nConnection status: {:?}", rudp1.connection_status(target));

    // Clean shutdown
    rudp1.close().await;
    handle.abort();

    println!("Example completed successfully!");
    Ok(())
} 