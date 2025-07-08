use rudpbase::{Rudpbase, RudpError};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Rudpbase 拥塞控制演示 ===\n");

    // 创建两个Rudpbase实例
    let mut rudp1 = Rudpbase::new("127.0.0.1:8080".parse().unwrap()).await?;
    let mut rudp2 = Rudpbase::new("127.0.0.1:8081".parse().unwrap()).await?;

    let addr1: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:8081".parse().unwrap();

    println!("🚀 开始拥塞控制测试...\n");

    // 启动接收任务
    let mut rudp2_clone = rudp2;
    let receiver_task = tokio::spawn(async move {
        loop {
            if let Some(received) = rudp2_clone.recv().await {
                match received.result {
                    Ok(buffer) => {
                        let data = &buffer.data()[..buffer.data_len()];
                        if let Ok(msg) = std::str::from_utf8(data) {
                            println!("📥 收到消息: {}", msg);
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ 接收错误: {}", e);
                    }
                }
            }
            rudp2_clone.tick().await;
            time::sleep(Duration::from_millis(1)).await;
        }
    });

    // 测试快速发送（会触发拥塞控制）
    println!("📤 开始快速发送测试（会触发拥塞控制）...");
    
    let mut sent_count = 0;
    let mut congestion_blocks = 0;
    let start_time = Instant::now();

    for i in 0..100 {
        let mut buffer = rudp1.get_buffer()?;
        let message = format!("快速消息 #{}", i);
        let data = message.as_bytes();
        
        buffer.data_mut()[..data.len()].copy_from_slice(data);
        buffer.set_data_len(data.len())?;

        match rudp1.send(buffer, addr2).await {
            Ok(()) => {
                sent_count += 1;
                println!("✅ 发送成功 #{}: {}", i, message);
            }
            Err(RudpError::CongestionWindowFull) => {
                congestion_blocks += 1;
                println!("🚫 拥塞窗口满，消息 #{} 被阻塞", i);
                
                // 等待一段时间让ACK处理
                for _ in 0..10 {
                    rudp1.tick().await;
                    time::sleep(Duration::from_millis(10)).await;
                }
                
                // 重试发送
                let mut buffer = rudp1.get_buffer()?;
                buffer.data_mut()[..data.len()].copy_from_slice(data);
                buffer.set_data_len(data.len())?;
                
                match rudp1.send(buffer, addr2).await {
                    Ok(()) => {
                        sent_count += 1;
                        println!("✅ 重试成功 #{}: {}", i, message);
                    }
                    Err(e) => {
                        println!("❌ 重试失败 #{}: {}", i, e);
                    }
                }
            }
            Err(e) => {
                println!("❌ 发送失败 #{}: {}", i, e);
            }
        }

        // 处理tick（包括ACK处理）
        rudp1.tick().await;
        
        // 短暂延迟以观察拥塞控制效果
        time::sleep(Duration::from_millis(50)).await;
    }

    let elapsed = start_time.elapsed();
    
    println!("\n=== 发送统计 ===");
    println!("总发送时间: {:?}", elapsed);
    println!("成功发送: {} 条消息", sent_count);
    println!("拥塞阻塞: {} 次", congestion_blocks);
    println!("平均发送速率: {:.2} 消息/秒", sent_count as f64 / elapsed.as_secs_f64());

    // 显示连接统计
    if let Some(stats) = rudp1.get_stats(addr2) {
        println!("\n=== 连接统计 ===");
        println!("发送包数: {}", stats.packets_sent);
        println!("接收包数: {}", stats.packets_received);
        println!("丢包数: {}", stats.packets_lost);
        println!("重传数: {}", stats.retransmissions);
        println!("平均RTT: {:?}", stats.avg_rtt);
        println!("丢包率: {:.2}%", stats.packet_loss_rate() * 100.0);
    }

    // 等待一段时间确保所有消息都被处理
    println!("\n⏳ 等待消息处理完成...");
    for _ in 0..50 {
        rudp1.tick().await;
        time::sleep(Duration::from_millis(100)).await;
    }

    println!("\n🎉 拥塞控制演示完成！");
    
    // 取消接收任务
    receiver_task.abort();
    
    Ok(())
} 