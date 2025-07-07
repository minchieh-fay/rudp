use rudpbase::Rudpbase;
use std::net::SocketAddr;
use tokio::time::{sleep, Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Rudpbase buffer pool usage example...");

    let addr1: SocketAddr = "127.0.0.1:8084".parse()?;
    let addr2: SocketAddr = "127.0.0.1:8085".parse()?;

    let mut rudp1 = Rudpbase::new(addr1).await?;
    let mut rudp2 = Rudpbase::new(addr2).await?;

    // Server task (rudp2)
    tokio::spawn(async move {
        let mut received_count = 0;
        
        loop {
            rudp2.tick().await;
            
            if let Some(received) = rudp2.recv().await {
                match received.result {
                    Ok(buffer) => {
                        received_count += 1;
                        println!("Server received packet #{} from {}: {} bytes", 
                            received_count,
                            received.from, 
                            buffer.data().len());
                        
                        // Echo back the data
                        let mut response_buffer = rudp2.get_buffer().unwrap();
                        response_buffer.data_mut()[..buffer.data().len()].copy_from_slice(buffer.data());
                        response_buffer.set_data_len(buffer.data().len()).unwrap();
                        
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

    println!("Client starting buffer pool performance test...");

    // Test 1: Zero-copy sending with buffer pool
    println!("\n=== Test 1: Zero-copy buffer pool usage ===");
    let start_time = Instant::now();
    
    for i in 0..1000 {
        // Get buffer from pool
        let mut buffer = rudp1.get_buffer()?;
        
        // Fill with test data
        let test_data = format!("Test message #{:04} - This is a test message for buffer pool performance", i);
        let test_bytes = test_data.as_bytes();
        buffer.data_mut()[..test_bytes.len()].copy_from_slice(test_bytes);
        buffer.set_data_len(test_bytes.len())?;
        
        // Send (zero-copy)
        rudp1.send(buffer, addr2).await?;
        
        // Maintain connection
        rudp1.tick().await;
        
        if i % 100 == 0 {
            println!("Sent {} messages", i);
        }
    }
    
    let zero_copy_time = start_time.elapsed();
    println!("Zero-copy method: {:?} for 1000 messages", zero_copy_time);

    // Wait for all responses
    let mut response_count = 0;
    let response_start = Instant::now();
    
    while response_count < 1000 && response_start.elapsed() < Duration::from_secs(10) {
        rudp1.tick().await;
        
        if let Some(received) = rudp1.recv().await {
            match received.result {
                Ok(_buffer) => {
                    response_count += 1;
                    if response_count % 100 == 0 {
                        println!("Received {} responses", response_count);
                    }
                }
                Err(e) => {
                    println!("Client receive error: {}", e);
                }
            }
        }
        
        sleep(Duration::from_millis(1)).await;
    }
    
    println!("Received {} out of 1000 responses", response_count);

    // Print buffer pool statistics
    if let Ok(stats) = rudp1.get_buffer_pool_stats() {
        println!("\nBuffer Pool Statistics:");
        println!("  Total allocations: {}", stats.total_allocations);
        println!("  Pool hits: {}", stats.pool_hits);
        println!("  Pool misses: {}", stats.pool_misses);
        println!("  Current free buffers: {}", stats.free_count);
        println!("  Hit rate: {:.2}%", 
            (stats.pool_hits as f64 / stats.total_allocations as f64) * 100.0);
    }

    // Print connection statistics
    if let Some(stats) = rudp1.get_stats(addr2) {
        println!("\nConnection Statistics:");
        println!("  Packets sent: {}", stats.packets_sent);
        println!("  Packets received: {}", stats.packets_received);
        println!("  Packets lost: {}", stats.packets_lost);
        println!("  Retransmissions: {}", stats.retransmissions);
        println!("  Average RTT: {:?}", stats.avg_rtt);
        println!("  Packet loss rate: {:.2}%", stats.packet_loss_rate() * 100.0);
    }

         println!("\nBuffer pool usage example completed!");
     Ok(())
} 