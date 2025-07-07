# RUDP Project

这是一个可靠UDP传输协议的Rust实现项目，采用分层设计：

## 项目结构

```
rudp/
├── rudpbase/           # 底层可靠UDP传输库
│   ├── src/           # 核心实现代码
│   ├── examples/      # 使用示例
│   ├── tests/         # 单元测试
│   ├── Cargo.toml     # 依赖配置
│   └── README.md      # 详细设计文档
│
└── rudp/              # 上层业务封装库（待开发）
    ├── src/           # 业务封装代码
    ├── examples/      # 业务示例
    ├── tests/         # 集成测试
    ├── Cargo.toml     # 依赖配置
    └── README.md      # 使用文档
```

## 分层设计

### Rudpbase（底层库）
- **职责**: 提供核心的可靠UDP传输功能
- **特性**: 
  - 不丢包的UDP传输
  - 不做包排序，专注可靠性
  - 高性能，9字节协议头
  - 4字节seq支持高带宽场景
  - 完整的重传和超时机制

### Rudp（上层库）
- **职责**: 基于rudpbase进行业务封装
- **特性**:
  - 多业务复用
  - 会话管理
  - 业务层协议
  - 更高级的API接口

## 快速开始

### 使用Rudpbase
```bash
cd rudpbase
cargo run --example basic_usage
```

### 开发Rudp（计划中）
```bash
cd rudp
cargo build
```

## 设计理念

1. **分层解耦**: 底层专注传输可靠性，上层处理业务逻辑
2. **高性能**: 避免不必要的复杂性，专注核心功能
3. **可扩展**: 为不同业务场景提供灵活的封装能力
4. **易用性**: 提供简洁清晰的API接口

## 许可证

MIT OR Apache-2.0 