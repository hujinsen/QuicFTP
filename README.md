# QuicFTP

基于 QUIC 协议的 FTP 服务器与客户端，使用 Rust 编写。

## 为什么选择 QuicFTP

传统 FTP 协议设计于 1971 年，存在诸多问题：

| 问题 | 传统 FTP | QuicFTP |
|------|----------|---------|
| 数据传输 | 双通道模式，需要 PASV/PORT 协商 | 单连接多流，无需协商 |
| 防火墙穿透 | 被动模式经常被拦截 | QUIC 基于 UDP，天然友好 |
| 加密 | 需要额外配置 FTPS | 内置 TLS 1.3，开箱即用 |
| 性能 | 多连接建立开销大 | 单连接多路复用，0-RTT 建连 |
| 可靠性 | 明文传输易被篡改 | 端到端加密，数据完整性保障 |

**适用场景：**
- 需要在防火墙/NAT 环境下进行文件传输
- 需要安全的加密传输但不想配置复杂的 FTPS
- 需要高性能的多文件并发传输
- 需要多用户隔离的文件共享服务

## 快速开始

### 环境要求

- Rust 1.70+（推荐使用 [rustup](https://rustup.rs/) 安装）

### 安装

```bash
git clone https://github.com/hujinsen/QuicFTP.git
cd QuicFTP
cargo build --release
```



### 添加用户

```bash
cargo run --bin quicftp-server -- user add alice --password mypassword
```

### 启动服务器

```bash
cargo run --bin quicftp-server
```

首次启动会自动生成 TLS 证书，无需手动配置。

### 连接服务器

```bash
cargo run --bin quicftp-client -- -H 127.0.0.1 -p 5000
```

进入交互界面后：

```
ftp> 用户 alice
请输入密码: alice
ftp> 密码 mypassword
欢迎, alice
ftp> 列表
📄         44 字节  readme.txt
📁 <目录> documents
ftp> 上传 local_file.txt
上传完成: local_file.txt (1024 字节)
ftp> 下载 readme.txt
下载完成: readme.txt (44 字节)
ftp> 退出
已断开连接
```

## 命令参考

### 文件操作

| 命令 | 说明 | 示例 |
|------|------|------|
| `列表` | 列出当前目录内容 | `列表` |
| `上传 <文件>` | 上传本地文件到服务器 | `上传 report.pdf` |
| `下载 <文件>` | 从服务器下载文件 | `下载 data.csv` |
| `大小 <文件>` | 查看文件大小 | `大小 video.mp4` |
| `删除 <文件>` | 删除服务器上的文件 | `删除 temp.txt` |
| `重命名 <旧名> <新名>` | 重命名文件 | `重命名 old.txt new.txt` |

### 目录操作

| 命令 | 说明 | 示例 |
|------|------|------|
| `当前目录` | 显示当前工作目录 | `当前目录` |
| `切换 <目录>` | 切换目录 | `切换 documents` |
| `创建目录 <目录名>` | 创建新目录 | `创建目录 backup` |
| `删除目录 <目录名>` | 删除空目录 | `删除目录 old` |

### 连接管理

| 命令 | 说明 | 示例 |
|------|------|------|
| `用户 <用户名>` | 输入登录用户名 | `用户 alice` |
| `密码 <密码>` | 输入登录密码 | `密码 mypassword` |
| `退出` | 断开连接并退出 | `退出` |
| `帮助` | 显示帮助信息 | `帮助` |

所有命令同时支持英文：`USER`、`PASS`、`LIST`、`GET`、`PUT`、`CD`、`PWD`、`MKDIR`、`DEL`、`QUIT`。

## 用户管理

```bash
# 添加用户
cargo run --bin quicftp-server -- user add <用户名> --password <密码>

# 列出所有用户
cargo run --bin quicftp-server -- user list

# 修改密码
cargo run --bin quicftp-server -- user password <用户名> --password <新密码>

# 删除用户
cargo run --bin quicftp-server -- user remove <用户名>
```

用户数据存储在 `config/users.toml`，每个用户拥有独立的主目录。

## 配置

服务器配置文件位于 `config/server.toml`：

```toml
host = "0.0.0.0"
port = 5000
cert_path = "config/cert.pem"
key_path = "config/key.pem"
users_path = "config/users.toml"
root_dir = "./ftp_root"
max_connections = 100
auto_generate_cert = true
```

## 技术架构

```
┌─────────────┐     QUIC/TLS     ┌─────────────┐
│   Client    │◄────────────────►│   Server    │
│             │   单连接多流      │             │
│  ┌───────┐  │                  │  ┌───────┐  │
│  │ Shell │  │   控制流(命令)    │  │Session│  │
│  └───┬───┘  │◄────────────────►│  └───┬───┘  │
│      │      │   数据流(文件)    │      │      │
│  ┌───┴───┐  │◄────────────────►│  ┌───┴───┐  │
│  │Client │  │                  │  │Handler│  │
│  └───────┘  │                  │  └───────┘  │
└─────────────┘                  └─────────────┘
```

- **控制流**：每个命令使用独立的双向流传输
- **数据流**：文件传输使用新的双向流，协议为 8 字节文件大小 + 文件内容
- **用户认证**：Argon2id 密码哈希，TOML 文件存储
- **TLS**：首次运行自动生成自签证书，客户端接受自签名证书

## 项目结构

```
QuicFTP/
├── Cargo.toml                  # Workspace 配置
├── config/
│   ├── server.toml             # 服务器配置
│   └── users.toml              # 用户数据
└── crates/
    ├── quicftp-common/         # 共享库（协议、配置、用户、TLS）
    ├── quicftp-server/         # 服务器端
    └── quicftp-client/         # 客户端
```

## 依赖

| 组件 | Crate | 用途 |
|------|-------|------|
| QUIC 协议 | `quinn` | 纯 Rust 实现，Tokio 原生 |
| 异步运行时 | `tokio` | 异步 IO 和并发 |
| CLI 解析 | `clap` | 命令行参数解析 |
| 密码哈希 | `argon2` | OWASP 推荐算法 |
| 序列化 | `serde` + `toml` | 配置文件处理 |
| 日志 | `tracing` | 结构化异步日志 |

## 许可证

MIT
