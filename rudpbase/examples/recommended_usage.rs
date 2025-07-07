use rudpbase::new_rudpbase;
use std::time::Duration;
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Rudpbase 推荐使用方式示例 ===");

    // 创建两个Rudpbase实例
    let mut rudp1 = new_rudpbase("127.0.0.1:8080".parse()?).await?;
    let mut rudp2 = new_rudpbase("127.0.0.1:8081".parse()?).await?;

    println!("Rudpbase实例创建成功!");
    println!("实例1: 127.0.0.1:8080");
    println!("实例2: 127.0.0.1:8081");

    // 内存池已在创建时自动预热，无需手动操作
    println!("\n=== 内存池已自动预热 ===");

    // 启动接收任务
    let handle = tokio::spawn(async move {
        loop {
            // 维护连接
            rudp2.tick().await;
            
            // 检查接收数据
            if let Some(rbuffer) = rudp2.poll_read().await {
                match rbuffer.result {
                    Ok(buffer) => {
                        let message = String::from_utf8_lossy(buffer.as_slice());
                        println!("接收自 {}: {}", rbuffer.from, message);
                        
                        // 推荐方式：使用内存池发送响应
                        match rudp2.get_buffer() {
                            Ok(mut response_buffer) => {
                                let response = format!("Echo: {}", message);
                                let response_bytes = response.as_bytes();
                                
                                // 写入响应数据
                                response_buffer.data_mut()[..response_bytes.len()].copy_from_slice(response_bytes);
                                if let Err(e) = response_buffer.set_data_len(response_bytes.len()) {
                                    eprintln!("设置buffer长度失败: {}", e);
                                    continue;
                                }
                                
                                // 零拷贝发送
                                if let Err(e) = rudp2.write(response_buffer, rbuffer.from).await {
                                    eprintln!("发送响应失败: {}", e);
                                }
                            }
                            Err(e) => {
                                eprintln!("获取buffer失败: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("接收错误: {}", e);
                    }
                }
            }
            
            time::sleep(Duration::from_millis(1)).await;
        }
    });

    // 发送测试消息
    let target = "127.0.0.1:8081".parse()?;
    
    println!("\n=== 发送测试消息 ===");
    for i in 1..=5 {
        let message = format!("使用内存池的消息 #{}", i);
        println!("发送: {}", message);
        
        // 推荐方式：使用内存池
        match rudp1.get_buffer() {
            Ok(mut buffer) => {
                let message_bytes = message.as_bytes();
                
                // 写入数据到buffer
                buffer.data_mut()[..message_bytes.len()].copy_from_slice(message_bytes);
                if let Err(e) = buffer.set_data_len(message_bytes.len()) {
                    eprintln!("设置buffer长度失败: {}", e);
                    continue;
                }
                
                // 零拷贝发送
                if let Err(e) = rudp1.write(buffer, target).await {
                    eprintln!("发送消息失败: {}", e);
                }
            }
            Err(e) => {
                eprintln!("获取buffer失败: {}", e);
            }
        }
        
        time::sleep(Duration::from_millis(100)).await;
    }

    // 等待消息交换完成
    println!("\n=== 等待消息交换完成 ===");
    for _ in 0..50 {
        rudp1.tick().await;
        
        // 检查响应
        if let Some(rbuffer) = rudp1.poll_read().await {
            match rbuffer.result {
                Ok(buffer) => {
                    let message = String::from_utf8_lossy(buffer.as_slice());
                    println!("响应自 {}: {}", rbuffer.from, message);
                }
                Err(e) => {
                    eprintln!("接收错误: {}", e);
                }
            }
        }
        
        time::sleep(Duration::from_millis(10)).await;
    }

    // 显示内存池统计
    println!("\n=== 内存池统计 ===");
    if let Ok(stats) = rudp1.get_buffer_pool_stats() {
        println!("实例1内存池统计:");
        println!("  总分配次数: {}", stats.total_allocations);
        println!("  池命中次数: {}", stats.pool_hits);
        println!("  池未命中次数: {}", stats.pool_misses);
        println!("  当前空闲buffer数量: {}", stats.free_count);
        println!("  池命中率: {:.2}%", 
            if stats.total_allocations > 0 { 
                stats.pool_hits as f64 / stats.total_allocations as f64 * 100.0 
            } else { 
                0.0 
            });
    }

    // 显示连接统计
    println!("\n=== 连接统计 ===");
    if let Some(stats) = rudp1.get_stats(target) {
        println!("连接统计:");
        println!("  发送包数: {}", stats.packets_sent);
        println!("  接收包数: {}", stats.packets_received);
        println!("  丢包数: {}", stats.packets_lost);
        println!("  重传次数: {}", stats.retransmissions);
        println!("  平均RTT: {:?}", stats.avg_rtt);
        println!("  丢包率: {:.2}%", stats.packet_loss_rate() * 100.0);
    }

    println!("\n连接状态: {:?}", rudp1.connection_status(target));

    // 清理资源
    rudp1.close().await;
    handle.abort();

    println!("\n推荐使用方式示例完成!");
    Ok(())
} 