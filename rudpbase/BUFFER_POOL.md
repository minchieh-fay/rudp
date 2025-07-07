# Rudpbase 内存池设计

## 概述

Rudpbase 内存池是一个高效的内存管理系统，专门为可靠UDP传输优化设计。它通过预分配和重用固定大小的buffer来避免频繁的内存分配/释放操作，从而提高性能。

## 设计原则

### 1. 固定大小的Buffer块
- **所有buffer大小固定为 `DEFAULT_BUFFER_SIZE`（1409字节）**
- 包含：协议头预留空间（9字节）+ 用户数据区（1400字节）
- 固定大小确保内存管理的效率和一致性

### 2. 零拷贝架构
- Buffer预留协议头空间，用户直接写入数据区
- 发送时无需额外拷贝，直接使用预分配的buffer
- 自动内存管理，RAII模式确保资源正确释放

### 3. 自动预热机制
- **创建Rudpbase实例时自动预热内存池**
- 预分配 `DEFAULT_INITIAL_CAPACITY`（500个）buffer
- 用户无需手动调用预热接口，开箱即用

### 4. 线程安全
- 使用 `Arc<Mutex<>>` 实现多线程安全的共享池
- 支持多个Rudpbase实例共享同一个内存池

## 内存布局

```
Buffer结构 (1409字节):
┌─────────────────────┬─────────────────────────────────────────────────────────┐
│   协议头预留空间     │                    用户数据区                              │
│      (9字节)       │                   (1400字节)                            │
├─────────────────────┼─────────────────────────────────────────────────────────┤
│type│安全码│seq│len  │                用户可写入数据                           │
│ 1  │  4  │ 4 │     │                                                         │
└─────────────────────┴─────────────────────────────────────────────────────────┘
```

## 核心组件

### PooledBuffer
- **用途**: 内存池管理的buffer块
- **特点**: 
  - 固定大小（1409字节）
  - 自动归还到池中（RAII）
  - 用户只能访问数据区（1400字节）
  - 协议头由rudpbase内部管理

### BufferPool
- **用途**: 管理固定大小的buffer块
- **特点**:
  - 所有buffer大小固定为 `DEFAULT_BUFFER_SIZE`
  - 使用双端队列管理空闲buffer
  - 提供详细的统计信息

### SharedBufferPool
- **用途**: 线程安全的共享内存池
- **特点**:
  - 可在多个Rudpbase实例间共享
  - 线程安全的分配和归还
  - 支持预热功能

## 配置参数

```rust
// 固定的buffer大小
pub const DEFAULT_BUFFER_SIZE: usize = 1409;  // 9 + 1400字节

// 内存池最大容量
pub const MAX_POOL_CAPACITY: usize = 200000;  // 最多缓存20万个buffer

// 默认初始容量
pub const DEFAULT_INITIAL_CAPACITY: usize = 500;  // 初始分配500个buffer
```

## API 使用

### 推荐使用方式（零拷贝）

```rust
use rudpbase::Rudpbase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建Rudpbase实例，内存池自动预热
    let mut rudp = Rudpbase::new("127.0.0.1:8080".parse()?).await?;
    
    // 从内存池获取buffer
    let mut buffer = rudp.get_buffer()?;
    
    // 写入用户数据
    let data = b"Hello, world!";
    buffer.data_mut()[..data.len()].copy_from_slice(data);
    buffer.set_data_len(data.len())?;
    
    // 零拷贝发送
    rudp.write(buffer, "127.0.0.1:8081".parse()?).await?;
    
    Ok(())
}
```

### 传统使用方式（兼容性）

```rust
use rudpbase::Rudpbase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建Rudpbase实例，内存池自动预热
    let mut rudp = Rudpbase::new("127.0.0.1:8080".parse()?).await?;
    
    // 传统方式（需要数据拷贝）
    let data = b"Hello, world!";
    rudp.write_bytes(data, "127.0.0.1:8081".parse()?).await?;
    
    Ok(())
}
```

## 性能特点

### 内存效率
- **固定大小**: 所有buffer都是1409字节，避免内存碎片
- **预分配**: 初始分配500个buffer，减少运行时分配开销
- **容量限制**: 最多缓存20万个buffer，防止内存无限增长

### 时间复杂度
- **分配**: O(1) - 从队列头部取出
- **归还**: O(1) - 放入队列尾部
- **统计**: O(1) - 实时维护计数器

### 内存占用估算
- **单个buffer**: 1409字节
- **默认初始容量**: 500个 × 1409字节 ≈ 704KB
- **最大容量**: 200,000个 × 1409字节 ≈ 282MB

## 统计信息

内存池提供详细的运行时统计：

```rust
let stats = rudp.get_buffer_pool_stats()?;
println!("总分配次数: {}", stats.total_allocations);
println!("池命中次数: {}", stats.pool_hits);
println!("池未命中次数: {}", stats.pool_misses);
println!("当前空闲buffer数量: {}", stats.free_count);
println!("池命中率: {:.2}%", 
    stats.pool_hits as f64 / stats.total_allocations as f64 * 100.0);
```

## 最佳实践

1. **使用推荐API**: 优先使用 `get_buffer()` + `write()` 的零拷贝方式
2. **自动预热**: 内存池在创建Rudpbase实例时自动预热，无需手动操作
3. **监控统计**: 定期检查池命中率，了解内存池使用情况
4. **避免长时间持有**: PooledBuffer会在Drop时自动归还，避免长时间持有

## 设计优势

1. **零拷贝**: 预分配buffer，避免发送时的数据拷贝
2. **内存安全**: RAII模式确保buffer正确归还
3. **高性能**: 固定大小的buffer池，O(1)分配和释放
4. **开箱即用**: 自动预热内存池，无需手动配置
5. **线程安全**: 支持多线程环境下的安全使用
6. **统计完备**: 提供详细的性能统计信息

这个设计确保了Rudpbase在高并发、高吞吐量场景下的优异性能表现。 