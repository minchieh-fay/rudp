use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use crate::error::RudpError;
use crate::protocol::PROTOCOL_HEADER_SIZE;

/// 默认buffer大小：协议头(9字节) + 数据区(1400字节)
pub const DEFAULT_BUFFER_SIZE: usize = PROTOCOL_HEADER_SIZE + 1400;

/// 内存池最大容量（固定值）
pub const MAX_POOL_CAPACITY: usize = 200000;

/// 内存池默认初始容量
pub const DEFAULT_INITIAL_CAPACITY: usize = 500;

/// 内存池管理的buffer块
/// 
/// 内存布局：
/// [协议头预留空间(9字节)][用户数据区(1400字节)]
/// |<-- header_size -->|<-- user data area -->|
/// 
/// 用户只能访问数据区，协议头由rudpbase内部填充
pub struct PooledBuffer {
    /// 完整的buffer（包含协议头空间）
    raw_buffer: Vec<u8>,
    /// 用户数据的实际长度
    data_len: usize,
    /// 内存池的引用，用于归还buffer
    pool: Arc<Mutex<BufferPool>>,
}

impl PooledBuffer {
    /// 创建新的池化buffer
    /// 
    /// 大小固定为 DEFAULT_BUFFER_SIZE
    fn new(pool: Arc<Mutex<BufferPool>>) -> Self {
        Self {
            raw_buffer: vec![0u8; DEFAULT_BUFFER_SIZE],
            data_len: 0,
            pool,
        }
    }

    /// 获取用户数据区的可写切片
    /// 
    /// 返回从协议头之后开始的数据区域
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.raw_buffer[PROTOCOL_HEADER_SIZE..]
    }

    /// 获取用户数据区的只读切片
    pub fn data(&self) -> &[u8] {
        &self.raw_buffer[PROTOCOL_HEADER_SIZE..PROTOCOL_HEADER_SIZE + self.data_len]
    }

    /// 设置用户数据的实际长度
    /// 
    /// # 参数
    /// - `len`: 用户数据的长度，不能超过数据区大小
    pub fn set_data_len(&mut self, len: usize) -> Result<(), RudpError> {
        let max_data_len = self.raw_buffer.len() - PROTOCOL_HEADER_SIZE;
        if len > max_data_len {
            return Err(RudpError::BufferTooLarge { size: len, max: max_data_len });
        }
        self.data_len = len;
        Ok(())
    }

    /// 获取用户数据长度
    pub fn data_len(&self) -> usize {
        self.data_len
    }

    /// 获取完整buffer（包含协议头空间）
    /// 
    /// 仅供rudpbase内部使用，用于填充协议头
    pub(crate) fn raw_buffer(&self) -> &[u8] {
        &self.raw_buffer
    }

    /// 获取完整buffer的可写引用
    /// 
    /// 仅供rudpbase内部使用，用于填充协议头
    pub(crate) fn raw_buffer_mut(&mut self) -> &mut [u8] {
        &mut self.raw_buffer
    }

    /// 获取协议头区域的可写切片
    /// 
    /// 仅供rudpbase内部使用
    pub(crate) fn header_mut(&mut self) -> &mut [u8] {
        &mut self.raw_buffer[..PROTOCOL_HEADER_SIZE]
    }

    /// 获取包含协议头的完整数据切片
    /// 
    /// 仅供rudpbase内部使用，用于发送数据
    pub(crate) fn full_packet(&self) -> &[u8] {
        &self.raw_buffer[..PROTOCOL_HEADER_SIZE + self.data_len]
    }

    /// 获取包含协议头的完整数据切片
    /// 
    /// 仅供rudpbase内部使用，用于发送数据
    pub(crate) fn full_data(&self) -> &[u8] {
        &self.raw_buffer[..PROTOCOL_HEADER_SIZE + self.data_len]
    }

    /// 填充协议头
    /// 
    /// 仅供rudpbase内部使用
    pub(crate) fn fill_protocol_header(&mut self, packet_type: crate::protocol::PacketType, seq: u32) -> Result<(), RudpError> {
        use crate::security::SecurityCode;
        
        // 计算安全码
        let security_code = SecurityCode::calculate(packet_type, seq, self.data());
        
        // 填充协议头
        let header = self.header_mut();
        header[0] = packet_type as u8;
        header[1..5].copy_from_slice(&security_code.to_be_bytes());
        header[5..9].copy_from_slice(&seq.to_be_bytes());
        
        Ok(())
    }

    /// 重置buffer状态，准备复用
    fn reset(&mut self) {
        // 只重置数据长度，不清零内存（性能优化）
        // 下次使用时会重新填充协议头和数据，无需清零
        self.data_len = 0;
    }
}

impl Drop for PooledBuffer {
    /// 自动归还buffer到内存池
    fn drop(&mut self) {
        if let Ok(mut pool) = self.pool.lock() {
            // 只重置数据长度，不清零内存（性能优化）
            // 下次使用时会重新填充协议头和数据，无需清零
            self.data_len = 0;
            
            // 归还到池中
            if pool.free_buffers.len() < MAX_POOL_CAPACITY {
                // 移动buffer到池中（避免clone）
                let mut buffer = Vec::new();
                std::mem::swap(&mut buffer, &mut self.raw_buffer);
                pool.free_buffers.push_back(buffer);
            }
            // 如果池已满，则直接丢弃buffer（让操作系统回收内存）
        }
    }
}

/// 内存池
/// 
/// 管理固定大小的buffer块，支持高效的分配和回收
/// 所有buffer块大小固定为 DEFAULT_BUFFER_SIZE
pub struct BufferPool {
    /// 空闲buffer队列
    free_buffers: VecDeque<Vec<u8>>,
    /// 统计信息
    stats: PoolStats,
}

/// 内存池统计信息
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// 总分配次数
    pub total_allocations: u64,
    /// 从池中获取的次数
    pub pool_hits: u64,
    /// 新分配的次数
    pub pool_misses: u64,
    /// 当前池中空闲buffer数量
    pub free_count: usize,
}

impl BufferPool {
    /// 创建新的内存池
    /// 
    /// # 参数
    /// - `initial_capacity`: 初始预分配的buffer数量
    /// 
    /// 注意：所有buffer大小固定为 DEFAULT_BUFFER_SIZE
    pub fn new(initial_capacity: usize) -> Self {
        let mut pool = Self {
            free_buffers: VecDeque::with_capacity(MAX_POOL_CAPACITY),
            stats: PoolStats {
                total_allocations: 0,
                pool_hits: 0,
                pool_misses: 0,
                free_count: 0,
            },
        };

        // 预分配初始buffer，大小固定为 DEFAULT_BUFFER_SIZE
        for _ in 0..initial_capacity {
            pool.free_buffers.push_back(vec![0u8; DEFAULT_BUFFER_SIZE]);
        }

        pool
    }

    /// 创建默认配置的内存池
    pub fn default() -> Self {
        Self::new(DEFAULT_INITIAL_CAPACITY)
    }

    /// 从池中获取buffer
    fn get_buffer(&mut self) -> Vec<u8> {
        self.stats.total_allocations += 1;

        if let Some(buffer) = self.free_buffers.pop_front() {
            // 从池中获取
            self.stats.pool_hits += 1;
            buffer
        } else {
            // 池为空，分配新buffer，大小固定为 DEFAULT_BUFFER_SIZE
            self.stats.pool_misses += 1;
            vec![0u8; DEFAULT_BUFFER_SIZE]
        }
    }

    /// 获取统计信息
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            total_allocations: self.stats.total_allocations,
            pool_hits: self.stats.pool_hits,
            pool_misses: self.stats.pool_misses,
            free_count: self.free_buffers.len(),
        }
    }
}

/// 共享内存池
/// 
/// 线程安全的内存池，可以在多个rudpbase实例间共享
pub struct SharedBufferPool {
    pool: Arc<Mutex<BufferPool>>,
}

impl SharedBufferPool {
    /// 创建新的共享内存池
    /// 
    /// # 参数
    /// - `initial_capacity`: 初始预分配的buffer数量
    /// 
    /// 注意：所有buffer大小固定为 DEFAULT_BUFFER_SIZE
    pub fn new(initial_capacity: usize) -> Self {
        Self {
            pool: Arc::new(Mutex::new(BufferPool::new(initial_capacity))),
        }
    }

    /// 创建默认配置的共享内存池
    pub fn default() -> Self {
        Self {
            pool: Arc::new(Mutex::new(BufferPool::default())),
        }
    }

    /// 获取一个buffer用于写入数据
    /// 
    /// # 返回
    /// - `Ok(PooledBuffer)`: 可用的buffer，用户可以写入数据区
    /// - `Err(RudpError)`: 获取失败
    /// 
    /// # 使用示例
    /// ```rust
    /// use rudpbase::SharedBufferPool;
    /// 
    /// let pool = SharedBufferPool::default();
    /// let mut buffer = pool.get_write_buffer().unwrap();
    /// 
    /// // 写入用户数据
    /// let data = b"Hello, world!";
    /// buffer.data_mut()[..data.len()].copy_from_slice(data);
    /// buffer.set_data_len(data.len()).unwrap();
    /// 
    /// // buffer会在离开作用域时自动归还到池中
    /// ```
    pub fn get_write_buffer(&self) -> Result<PooledBuffer, RudpError> {
        let mut pool = self.pool.lock().map_err(|_| RudpError::InternalError)?;
        let raw_buffer = pool.get_buffer();
        
        Ok(PooledBuffer {
            raw_buffer,
            data_len: 0,
            pool: Arc::clone(&self.pool),
        })
    }

    /// 获取内存池统计信息
    pub fn stats(&self) -> Result<PoolStats, RudpError> {
        let pool = self.pool.lock().map_err(|_| RudpError::InternalError)?;
        Ok(PoolStats {
            total_allocations: pool.stats.total_allocations,
            pool_hits: pool.stats.pool_hits,
            pool_misses: pool.stats.pool_misses,
            free_count: pool.free_buffers.len(),
        })
    }

    /// 预热内存池
    /// 
    /// 预分配指定数量的buffer，提高后续分配性能
    /// 所有buffer大小固定为 DEFAULT_BUFFER_SIZE
    pub fn warmup(&self, count: usize) -> Result<(), RudpError> {
        let mut pool = self.pool.lock().map_err(|_| RudpError::InternalError)?;
        
        for _ in 0..count {
            if pool.free_buffers.len() >= MAX_POOL_CAPACITY {
                break;
            }
            pool.free_buffers.push_back(vec![0u8; DEFAULT_BUFFER_SIZE]);
        }
        
        Ok(())
    }
}

impl Clone for SharedBufferPool {
    fn clone(&self) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_basic() {
        let pool = SharedBufferPool::default();
        
        // 获取buffer
        let mut buffer = pool.get_write_buffer().unwrap();
        assert_eq!(buffer.data_len(), 0);
        
        // 写入数据
        let test_data = b"Hello, world!";
        buffer.data_mut()[..test_data.len()].copy_from_slice(test_data);
        buffer.set_data_len(test_data.len()).unwrap();
        
        // 验证数据
        assert_eq!(buffer.data(), test_data);
        assert_eq!(buffer.data_len(), test_data.len());
    }

    #[test]
    fn test_buffer_pool_reuse() {
        let pool = SharedBufferPool::default();
        
        // 获取初始统计
        let initial_stats = pool.stats().unwrap();
        
        // 分配和释放buffer
        {
            let _buffer = pool.get_write_buffer().unwrap();
        } // buffer在这里被释放并归还到池中
        
        // 再次分配，应该复用之前的buffer
        let _buffer2 = pool.get_write_buffer().unwrap();
        
        let final_stats = pool.stats().unwrap();
        assert_eq!(final_stats.total_allocations, initial_stats.total_allocations + 2);
        assert!(final_stats.pool_hits > initial_stats.pool_hits);
    }

    #[test]
    fn test_buffer_size_limit() {
        let pool = SharedBufferPool::default();
        let mut buffer = pool.get_write_buffer().unwrap();
        
        // 尝试设置过大的数据长度
        let max_data_len = DEFAULT_BUFFER_SIZE - PROTOCOL_HEADER_SIZE;
        assert!(buffer.set_data_len(max_data_len).is_ok());
        assert!(buffer.set_data_len(max_data_len + 1).is_err());
    }

    #[test]
    fn test_buffer_header_access() {
        let pool = SharedBufferPool::default();
        let mut buffer = pool.get_write_buffer().unwrap();
        
        // 测试协议头访问
        let header = buffer.header_mut();
        assert_eq!(header.len(), PROTOCOL_HEADER_SIZE);
        
        // 填充协议头
        header[0] = 1; // packet type
        header[1..5].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]); // security code
        
        // 填充用户数据
        let test_data = b"test";
        buffer.data_mut()[..test_data.len()].copy_from_slice(test_data);
        buffer.set_data_len(test_data.len()).unwrap();
        
        // 验证完整包
        let full_packet = buffer.full_packet();
        assert_eq!(full_packet.len(), PROTOCOL_HEADER_SIZE + test_data.len());
        assert_eq!(full_packet[0], 1);
        assert_eq!(&full_packet[PROTOCOL_HEADER_SIZE..], test_data);
    }
} 