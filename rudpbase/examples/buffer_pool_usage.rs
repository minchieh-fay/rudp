use std::net::SocketAddr;
use tokio::time::{sleep, Duration};
use rudpbase::{Rudpbase, RudpError};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Rudpbase 内存池使用示例 ===");

    // 创建两个Rudpbase实例
    let addr1: SocketAddr = "127.0.0.1:8080".parse()?;
    let addr2: SocketAddr = "127.0.0.1:8081".parse()?;

    let mut rudp1 = Rudpbase::new(addr1).await?;
    let mut rudp2 = Rudpbase::new(addr2).await?;

    println!("Rudpbase实例创建成功!");
    println!("实例1: {}", addr1);
    println!("实例2: {}", addr2);

    // 内存池已在创建时自动预热
    println!("\n=== 内存池已自动预热 ===");

    let stats = rudp1.get_buffer_pool_stats()?;
    println!("内存池统计 - 预热后:");
    println!("  空闲buffer数量: {}", stats.free_count);
    println!("  总分配次数: {}", stats.total_allocations);

    println!("\n=== 使用内存池发送数据 ===");

    // 使用内存池发送多条消息
    for i in 1..=5 {
        // 从内存池获取buffer
        let mut buffer = rudp1.get_buffer()?;
        
        // 准备数据
        let message = format!("使用内存池发送的消息 #{}", i);
        let data = message.as_bytes();
        
        // 写入数据到buffer
        buffer.data_mut()[..data.len()].copy_from_slice(data);
        buffer.set_data_len(data.len())?;
        
        // 发送数据（零拷贝）
        rudp1.write(buffer, addr2).await?;
        
        println!("发送: {}", message);
        
        // 让接收方处理数据
        sleep(Duration::from_millis(10)).await;
    }

    println!("\n=== 接收和处理数据 ===");

    // 接收数据
    for _ in 0..10 {
        rudp2.tick().await;
        
        if let Some(rbuffer) = rudp2.poll_read().await {
            match rbuffer.result {
                Ok(buffer) => {
                    let message = String::from_utf8_lossy(buffer.as_slice());
                    println!("接收自 {}: {}", rbuffer.from, message);
                }
                Err(e) => {
                    println!("接收错误: {}", e);
                }
            }
        }
        
        sleep(Duration::from_millis(10)).await;
    }

    println!("\n=== 内存池统计信息 ===");
    
    // 显示内存池统计
    let stats1 = rudp1.get_buffer_pool_stats()?;
    let stats2 = rudp2.get_buffer_pool_stats()?;
    
    println!("实例1内存池统计:");
    println!("  总分配次数: {}", stats1.total_allocations);
    println!("  池命中次数: {}", stats1.pool_hits);
    println!("  池未命中次数: {}", stats1.pool_misses);
    println!("  当前空闲buffer数量: {}", stats1.free_count);
    println!("  池命中率: {:.2}%", 
        if stats1.total_allocations > 0 { 
            stats1.pool_hits as f64 / stats1.total_allocations as f64 * 100.0 
        } else { 
            0.0 
        });

    println!("\n实例2内存池统计:");
    println!("  总分配次数: {}", stats2.total_allocations);
    println!("  池命中次数: {}", stats2.pool_hits);
    println!("  池未命中次数: {}", stats2.pool_misses);
    println!("  当前空闲buffer数量: {}", stats2.free_count);
    println!("  池命中率: {:.2}%", 
        if stats2.total_allocations > 0 { 
            stats2.pool_hits as f64 / stats2.total_allocations as f64 * 100.0 
        } else { 
            0.0 
        });

    println!("\n=== 性能对比测试 ===");
    
    // 测试传统方式 vs 内存池方式的性能
    let test_data = "Performance test data - This is a longer message for testing".as_bytes();
    let test_count = 1000;
    
    // 传统方式
    let start = std::time::Instant::now();
    for _ in 0..test_count {
        rudp1.write_bytes(test_data, addr2).await?;
    }
    let traditional_time = start.elapsed();
    
    // 内存池方式
    let start = std::time::Instant::now();
    for _ in 0..test_count {
        let mut buffer = rudp1.get_buffer()?;
        buffer.data_mut()[..test_data.len()].copy_from_slice(test_data);
        buffer.set_data_len(test_data.len())?;
        rudp1.write(buffer, addr2).await?;
    }
    let pooled_time = start.elapsed();
    
    println!("传统方式 ({} 次发送): {:?}", test_count, traditional_time);
    println!("内存池方式 ({} 次发送): {:?}", test_count, pooled_time);
    
    if traditional_time > pooled_time {
        let improvement = (traditional_time.as_nanos() as f64 / pooled_time.as_nanos() as f64 - 1.0) * 100.0;
        println!("内存池方式性能提升: {:.2}%", improvement);
    } else {
        let degradation = (pooled_time.as_nanos() as f64 / traditional_time.as_nanos() as f64 - 1.0) * 100.0;
        println!("内存池方式性能下降: {:.2}%", degradation);
    }

    // 最终统计
    let final_stats1 = rudp1.get_buffer_pool_stats()?;
    println!("\n=== 最终内存池统计 ===");
    println!("最终池命中率: {:.2}%", 
        if final_stats1.total_allocations > 0 { 
            final_stats1.pool_hits as f64 / final_stats1.total_allocations as f64 * 100.0 
        } else { 
            0.0 
        });

    // 清理资源
    rudp1.close().await;
    rudp2.close().await;

    println!("\n内存池使用示例完成!");
    Ok(())
} 