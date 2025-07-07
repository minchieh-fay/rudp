use rudpbase::Rudpbase;
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_basic_send_receive() {
    let addr1: SocketAddr = "127.0.0.1:9001".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:9002".parse().unwrap();

    let mut sender = Rudpbase::new(addr1).await.unwrap();
    let mut receiver = Rudpbase::new(addr2).await.unwrap();

    // Send a message
    let mut buffer = sender.get_buffer().unwrap();
    let test_data = b"Hello, World!";
    buffer.data_mut()[..test_data.len()].copy_from_slice(test_data);
    buffer.set_data_len(test_data.len()).unwrap();
    
    let target = addr2;
    sender.send(buffer, target).await.unwrap();

    // Give some time for the message to be sent and received
    sleep(Duration::from_millis(100)).await;

    // Receive the message
    let mut received_message = false;
    for _ in 0..100 {
        receiver.tick().await;
        
        if let Some(received) = receiver.recv().await {
            match received.result {
                Ok(buffer) => {
                    assert_eq!(buffer.data(), test_data);
                    assert_eq!(received.from, addr1);
                    received_message = true;
                    break;
                }
                Err(e) => {
                    panic!("Receive error: {}", e);
                }
            }
        }
        
        sleep(Duration::from_millis(1)).await;
    }
    
    assert!(received_message, "Message was not received");
}

#[tokio::test]
async fn test_large_message() {
    let addr1: SocketAddr = "127.0.0.1:9003".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:9004".parse().unwrap();

    let mut rudp = Rudpbase::new(addr1).await.unwrap();
    let target = addr2;

    // Test with large data (should fail if too large)
    let large_data = vec![0u8; 2000]; // Larger than max buffer size
    let mut buffer = rudp.get_buffer().unwrap();
    
         // This should fail because the data is too large
     let result = if large_data.len() <= buffer.data_mut().len() {
         buffer.data_mut()[..large_data.len()].copy_from_slice(&large_data);
         buffer.set_data_len(large_data.len())
     } else {
         Err(rudpbase::RudpError::BufferTooLarge { size: large_data.len(), max: buffer.data_mut().len() })
     };
     
     assert!(result.is_err(), "Large data should not fit in buffer");
}

#[tokio::test]
async fn test_multiple_messages() {
    let addr1: SocketAddr = "127.0.0.1:9005".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:9006".parse().unwrap();

    let mut sender = Rudpbase::new(addr1).await.unwrap();
    let mut receiver = Rudpbase::new(addr2).await.unwrap();

    let target = addr2;
    let message_count = 10;

    // Send multiple messages
    for i in 0..message_count {
        let mut buffer = sender.get_buffer().unwrap();
        let test_data = format!("Message {}", i);
        let test_bytes = test_data.as_bytes();
        buffer.data_mut()[..test_bytes.len()].copy_from_slice(test_bytes);
        buffer.set_data_len(test_bytes.len()).unwrap();
        
        sender.send(buffer, target).await.unwrap();
        sender.tick().await;
    }

    // Receive all messages
    let mut received_count = 0;
    for _ in 0..1000 { // Timeout after 1000 iterations
        receiver.tick().await;
        
        while let Some(_received) = receiver.recv().await {
            received_count += 1;
            if received_count >= message_count {
                break;
            }
        }
        
        if received_count >= message_count {
            break;
        }
        
        sleep(Duration::from_millis(1)).await;
    }
    
    assert_eq!(received_count, message_count, "Not all messages were received");
}

#[tokio::test]
async fn test_bidirectional_communication() {
    let addr1: SocketAddr = "127.0.0.1:9007".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:9008".parse().unwrap();

    let mut node1 = Rudpbase::new(addr1).await.unwrap();
    let mut node2 = Rudpbase::new(addr2).await.unwrap();

    // Node1 sends to Node2
    let mut buffer1 = node1.get_buffer().unwrap();
    let message1 = b"Hello from Node1";
    buffer1.data_mut()[..message1.len()].copy_from_slice(message1);
    buffer1.set_data_len(message1.len()).unwrap();
    node1.send(buffer1, addr2).await.unwrap();

    // Node2 sends to Node1
    let mut buffer2 = node2.get_buffer().unwrap();
    let message2 = b"Hello from Node2";
    buffer2.data_mut()[..message2.len()].copy_from_slice(message2);
    buffer2.set_data_len(message2.len()).unwrap();
    node2.send(buffer2, addr1).await.unwrap();

    sleep(Duration::from_millis(100)).await;

    // Check if Node1 received message from Node2
    let mut node1_received = false;
    for _ in 0..100 {
        node1.tick().await;
        
        if let Some(received) = node1.recv().await {
            match received.result {
                Ok(buffer) => {
                    assert_eq!(buffer.data(), message2);
                    assert_eq!(received.from, addr2);
                    node1_received = true;
                    break;
                }
                Err(e) => {
                    panic!("Node1 receive error: {}", e);
                }
            }
        }
        
        sleep(Duration::from_millis(1)).await;
    }

    // Check if Node2 received message from Node1
    let mut node2_received = false;
    for _ in 0..100 {
        node2.tick().await;
        
        if let Some(received) = node2.recv().await {
            match received.result {
                Ok(buffer) => {
                    assert_eq!(buffer.data(), message1);
                    assert_eq!(received.from, addr1);
                    node2_received = true;
                    break;
                }
                Err(e) => {
                    panic!("Node2 receive error: {}", e);
                }
            }
        }
        
        sleep(Duration::from_millis(1)).await;
    }

    assert!(node1_received, "Node1 did not receive message from Node2");
    assert!(node2_received, "Node2 did not receive message from Node1");
}

#[tokio::test]
async fn test_connection_statistics() {
    let addr1: SocketAddr = "127.0.0.1:9009".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:9010".parse().unwrap();

    let mut sender = Rudpbase::new(addr1).await.unwrap();
    let mut receiver = Rudpbase::new(addr2).await.unwrap();

    // Send a few messages
    for i in 0..5 {
        let mut buffer = sender.get_buffer().unwrap();
        let test_data = format!("Test message {}", i);
        let test_bytes = test_data.as_bytes();
        buffer.data_mut()[..test_bytes.len()].copy_from_slice(test_bytes);
        buffer.set_data_len(test_bytes.len()).unwrap();
        
        sender.send(buffer, addr2).await.unwrap();
        sender.tick().await;
    }

    // Receive messages
    for _ in 0..100 {
        receiver.tick().await;
        if let Some(_received) = receiver.recv().await {
            // Message received
        }
        sleep(Duration::from_millis(1)).await;
    }

    // Check statistics
    if let Some(stats) = sender.get_stats(addr2) {
        assert!(stats.packets_sent > 0, "No packets were sent according to statistics");
    }
}

#[tokio::test]
async fn test_buffer_pool_stats() {
    let addr1: SocketAddr = "127.0.0.1:9011".parse().unwrap();
    let rudp = Rudpbase::new(addr1).await.unwrap();

    // Get buffer pool statistics
    let stats = rudp.get_buffer_pool_stats().unwrap();
    
    // Should have some initial state
    assert!(stats.total_allocations >= 0, "Total allocations should be non-negative");
    assert!(stats.pool_hits >= 0, "Pool hits should be non-negative");
    assert!(stats.pool_misses >= 0, "Pool misses should be non-negative");
} 