use rudpbase::{new_rudpbase, Buffer, RudpError};
use std::time::Duration;
use tokio::time;

#[tokio::test]
async fn test_basic_send_receive() {
    let mut sender = new_rudpbase("127.0.0.1:9001".parse().unwrap()).await.unwrap();
    let mut receiver = new_rudpbase("127.0.0.1:9002".parse().unwrap()).await.unwrap();

    let target = "127.0.0.1:9002".parse().unwrap();
    let test_data = b"Hello, Rudpbase!";

    // Send data using buffer pool
    let mut buffer = sender.get_buffer().unwrap();
    buffer.data_mut()[..test_data.len()].copy_from_slice(test_data);
    buffer.set_data_len(test_data.len()).unwrap();
    sender.write(buffer, target).await.unwrap();

    // Give some time for packet transmission
    for _ in 0..10 {
        sender.tick().await;
        receiver.tick().await;
        time::sleep(Duration::from_millis(10)).await;
    }

    // Receive data
    let mut received = false;
    for _ in 0..10 {
        receiver.tick().await;
        if let Some(rbuffer) = receiver.poll_read().await {
            match rbuffer.result {
                Ok(buffer) => {
                    assert_eq!(buffer.as_slice(), test_data);
                    received = true;
                    break;
                }
                Err(e) => panic!("Receive error: {}", e),
            }
        }
        time::sleep(Duration::from_millis(10)).await;
    }

    assert!(received, "Data was not received");

    sender.close().await;
    receiver.close().await;
}

#[tokio::test]
async fn test_buffer_size_limit() {
    let mut rudp = new_rudpbase("127.0.0.1:9003".parse().unwrap()).await.unwrap();
    let target = "127.0.0.1:9004".parse().unwrap();

    // Test with data that exceeds maximum buffer size
    let large_data = vec![0u8; 1500]; // Larger than MAX_BUFFER_SIZE (1200)
    
    let result = rudp.write_bytes(&large_data, target).await;
    assert!(result.is_err());
    
    if let Err(RudpError::BufferTooLarge { size, max }) = result {
        assert_eq!(size, 1500);
        assert_eq!(max, 1200);
    } else {
        panic!("Expected BufferTooLarge error");
    }

    rudp.close().await;
}

#[tokio::test]
async fn test_connection_statistics() {
    let mut sender = new_rudpbase("127.0.0.1:9005".parse().unwrap()).await.unwrap();
    let mut receiver = new_rudpbase("127.0.0.1:9006".parse().unwrap()).await.unwrap();

    let target = "127.0.0.1:9006".parse().unwrap();
    let _test_data = b"Test statistics"; 

    // Send multiple packets
    for i in 0..5 {
        let data = format!("Message {}", i);
        let mut buffer = sender.get_buffer().unwrap();
        let data_bytes = data.as_bytes();
        buffer.data_mut()[..data_bytes.len()].copy_from_slice(data_bytes);
        buffer.set_data_len(data_bytes.len()).unwrap();
        sender.write(buffer, target).await.unwrap();
        time::sleep(Duration::from_millis(10)).await;
    }

    // Process packets
    for _ in 0..50 {
        sender.tick().await;
        receiver.tick().await;
        
        while let Some(_rbuffer) = receiver.poll_read().await {
            // Process received data
        }
        
        time::sleep(Duration::from_millis(10)).await;
    }

    // Check statistics
    let stats = sender.get_stats(target);
    assert!(stats.is_some());
    
    let stats = stats.unwrap();
    assert!(stats.packets_sent > 0);
    println!("Packets sent: {}", stats.packets_sent);
    println!("Average RTT: {:?}", stats.avg_rtt);

    sender.close().await;
    receiver.close().await;
}

#[tokio::test]
async fn test_bidirectional_communication() {
    let mut node1 = new_rudpbase("127.0.0.1:9007".parse().unwrap()).await.unwrap();
    let mut node2 = new_rudpbase("127.0.0.1:9008".parse().unwrap()).await.unwrap();

    let addr1 = "127.0.0.1:9007".parse().unwrap();
    let addr2 = "127.0.0.1:9008".parse().unwrap();

    // Node1 sends to Node2
    let mut buffer1 = node1.get_buffer().unwrap();
    let data1 = b"Hello from Node1";
    buffer1.data_mut()[..data1.len()].copy_from_slice(data1);
    buffer1.set_data_len(data1.len()).unwrap();
    node1.write(buffer1, addr2).await.unwrap();
    
    // Node2 sends to Node1
    let mut buffer2 = node2.get_buffer().unwrap();
    let data2 = b"Hello from Node2";
    buffer2.data_mut()[..data2.len()].copy_from_slice(data2);
    buffer2.set_data_len(data2.len()).unwrap();
    node2.write(buffer2, addr1).await.unwrap();

    let mut node1_received = false;
    let mut node2_received = false;

    // Process communication
    for _ in 0..100 {
        node1.tick().await;
        node2.tick().await;

        // Check Node1 received data
        if let Some(rbuffer) = node1.poll_read().await {
            match rbuffer.result {
                Ok(buffer) => {
                    assert_eq!(buffer.as_slice(), b"Hello from Node2");
                    node1_received = true;
                }
                Err(e) => panic!("Node1 receive error: {}", e),
            }
        }

        // Check Node2 received data
        if let Some(rbuffer) = node2.poll_read().await {
            match rbuffer.result {
                Ok(buffer) => {
                    assert_eq!(buffer.as_slice(), b"Hello from Node1");
                    node2_received = true;
                }
                Err(e) => panic!("Node2 receive error: {}", e),
            }
        }

        if node1_received && node2_received {
            break;
        }

        time::sleep(Duration::from_millis(10)).await;
    }

    assert!(node1_received, "Node1 did not receive data");
    assert!(node2_received, "Node2 did not receive data");

    node1.close().await;
    node2.close().await;
}

#[test]
fn test_buffer_creation() {
    let data = b"Test data";
    let buffer = Buffer::new(data).unwrap();
    assert_eq!(buffer.as_slice(), data);
    assert_eq!(buffer.len, data.len());
}

#[test]
fn test_buffer_too_large() {
    let large_data = vec![0u8; 1500];
    let result = Buffer::new(&large_data);
    assert!(result.is_err());
} 