use rudpbase::Rudpbase;
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Buffer 生命周期演示 ===\n");

    // 创建两个Rudpbase实例
    let mut sender = Rudpbase::new("127.0.0.1:9001".parse().unwrap()).await?;
    let mut receiver = Rudpbase::new("127.0.0.1:9002".parse().unwrap()).await?;

    let sender_addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();
    let receiver_addr: SocketAddr = "127.0.0.1:9002".parse().unwrap();

    // 显示初始内存池状态
    println!("📊 初始内存池状态:");
    if let Ok(stats) = sender.get_buffer_pool_stats() {
        println!("  - 总分配次数: {}", stats.total_allocations);
        println!("  - 池命中次数: {}", stats.pool_hits);
        println!("  - 池未命中次数: {}", stats.pool_misses);
        println!("  - 空闲buffer数量: {}", stats.free_count);
    }

    println!("\n🔄 开始生命周期演示...\n");

    // 第1步：发送数据
    println!("📤 步骤1: 发送数据");
    {
        let mut buffer = sender.get_buffer()?;
        println!("  ✅ 从内存池获取buffer");
        
        let message = "Hello, 这是生命周期测试!";
        let data = message.as_bytes();
        buffer.data_mut()[..data.len()].copy_from_slice(data);
        buffer.set_data_len(data.len())?;
        
        println!("  ✅ 填充数据: {}", message);
        sender.send(buffer, receiver_addr).await?;
        println!("  ✅ 发送完成，buffer仍在send_buffer中等待ACK");
    } // <- buffer变量离开作用域，但实际buffer在send_buffer中
    
    // 第2步：接收数据
    println!("\n📥 步骤2: 接收数据");
    
    // 启动接收任务
    let received_data = tokio::spawn(async move {
        loop {
            receiver.tick().await;
            if let Some(received) = receiver.recv().await {
                return received;
            }
            sleep(Duration::from_millis(10)).await;
        }
    });

    // 处理ACK
    for _ in 0..50 {
        sender.tick().await;
        sleep(Duration::from_millis(10)).await;
    }

    // 获取接收到的数据
    let received = received_data.await?;
    
    println!("  ✅ 接收到数据，开始处理...");
    
    // 第3步：用户处理数据
    println!("\n🔍 步骤3: 用户处理数据");
    match received.result {
        Ok(received_buffer) => {
            println!("  ✅ 获得PooledBuffer，内存池引用计数增加");
            
            // 用户读取数据
            let data = &received_buffer.data()[..received_buffer.data_len()];
            let message = std::str::from_utf8(data).unwrap();
            println!("  ✅ 读取数据: {}", message);
            
            // 显示当前内存池状态
            if let Ok(stats) = sender.get_buffer_pool_stats() {
                println!("  📊 当前内存池状态:");
                println!("    - 总分配次数: {}", stats.total_allocations);
                println!("    - 池命中次数: {}", stats.pool_hits);
                println!("    - 池未命中次数: {}", stats.pool_misses);
                println!("    - 空闲buffer数量: {}", stats.free_count);
            }
            
            println!("  ⏳ 模拟用户处理时间...");
            sleep(Duration::from_millis(1000)).await;
            
            println!("  ✅ 用户处理完成");
            
            // 这里received_buffer即将离开作用域
        } // <- 关键时刻：PooledBuffer离开作用域，自动调用Drop::drop()
        Err(e) => {
            println!("  ❌ 接收错误: {}", e);
        }
    }
    
    // 第4步：观察自动回收
    println!("\n♻️  步骤4: 自动回收完成");
    println!("  ✅ PooledBuffer已离开作用域");
    println!("  ✅ Rust自动调用了Drop::drop()方法");
    println!("  ✅ Buffer已自动归还到内存池");
    
    // 显示回收后的内存池状态
    if let Ok(stats) = sender.get_buffer_pool_stats() {
        println!("  📊 回收后内存池状态:");
        println!("    - 总分配次数: {}", stats.total_allocations);
        println!("    - 池命中次数: {}", stats.pool_hits);
        println!("    - 池未命中次数: {}", stats.pool_misses);
        println!("    - 空闲buffer数量: {}", stats.free_count);
    }

    // 第5步：验证复用
    println!("\n🔄 步骤5: 验证buffer复用");
    {
        let mut buffer = sender.get_buffer()?;
        println!("  ✅ 再次获取buffer（应该复用之前的）");
        
        if let Ok(stats) = sender.get_buffer_pool_stats() {
            println!("  📊 获取后内存池状态:");
            println!("    - 总分配次数: {}", stats.total_allocations);
            println!("    - 池命中次数: {}", stats.pool_hits);
            println!("    - 池未命中次数: {}", stats.pool_misses);
            println!("    - 空闲buffer数量: {}", stats.free_count);
        }
        
        // 使用buffer
        let message = "复用的buffer测试";
        let data = message.as_bytes();
        buffer.data_mut()[..data.len()].copy_from_slice(data);
        buffer.set_data_len(data.len())?;
        
        println!("  ✅ 复用buffer成功: {}", message);
    } // <- buffer再次自动回收

    println!("\n🎉 生命周期演示完成！");
    println!("\n📝 总结:");
    println!("  1. 用户无需手动管理内存");
    println!("  2. PooledBuffer离开作用域时自动回收");
    println!("  3. 内存池实现高效复用");
    println!("  4. 避免频繁的内存分配/释放");
    println!("  5. 零拷贝 + 自动管理 = 高性能");

    Ok(())
} 