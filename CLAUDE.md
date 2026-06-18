# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

QuicFTP 是一个基于 QUIC 协议的 FTP 服务器和客户端，使用 Rust 编写。它利用 QUIC 的多路复用流替代传统 FTP 的双通道模型，无需 PASV/PORT 协商。

## 构建和运行

```bash
# 构建项目
cargo build

# 运行服务器
cargo run --bin quicftp-server -- serve

# 运行客户端
cargo run --bin quicftp-client -- --host localhost --port 5000

# 运行测试
cargo test
```

## 用户管理

```bash
# 添加用户
cargo run --bin quicftp-server -- user add <用户名> --password <密码>

# 列出用户
cargo run --bin quicftp-server -- user list

# 修改密码
cargo run --bin quicftp-server -- user password <用户名> --password <新密码>

# 删除用户
cargo run --bin quicftp-server -- user remove <用户名>
```

## 中文命令

| 命令 | 说明 | 示例 |
|------|------|------|
| 用户 | 登录用户名 | `用户 hujinsen` |
| 密码 | 登录密码 | `密码 123456` |
| 列表 | 列出目录内容 | `列表` |
| 上传 | 上传文件 | `上传 local.txt` |
| 下载 | 下载文件 | `下载 remote.txt` |
| 切换 | 切换目录 | `切换 /subdir` |
| 当前目录 | 显示当前目录 | `当前目录` |
| 创建目录 | 创建目录 | `创建目录 newdir` |
| 删除目录 | 删除目录 | `删除目录 olddir` |
| 删除 | 删除文件 | `删除 file.txt` |
| 重命名 | 重命名文件 | `重命名 old.txt new.txt` |
| 大小 | 查看文件大小 | `大小 file.txt` |
| 退出 | 断开连接 | `退出` |
| 帮助 | 显示帮助 | `帮助` |

也支持英文命令: USER, PASS, LIST, GET, PUT, CD, PWD, MKDIR, DEL, QUIT

## 架构

### Workspace 结构

- `crates/quicftp-common/` - 共享协议定义、配置、用户管理、TLS
- `crates/quicftp-server/` - QUIC 服务器实现
- `crates/quicftp-client/` - 交互式 CLI 客户端

### 关键设计决策

1. **QUIC 流作为 FTP 通道**: 每个客户端一个 QUIC 连接。控制命令通过主流传输，文件传输使用独立的双向流（通过 `open_bi()`/`accept_bi()` 打开）。

2. **协议格式**: 基于文本的中文命令（如 `用户 alice`、`列表`、`下载 file.txt`）。响应使用 FTP 风格的状态码（`230 欢迎`）。

3. **用户存储**: TOML 文件（`config/users.toml`），使用 Argon2id 哈希密码。

4. **TLS**: 首次运行时自动生成自签证书。客户端接受任意证书（开发用途）。

5. **文件传输**: 使用 QUIC 双向流传输文件数据。上传时客户端先发送文件大小（8字节），再发送文件内容。下载时服务器先发送文件大小，再发送文件内容。

### 流程

1. 客户端连接 → 服务器发送 `220 欢迎使用 QuicFTP 服务器` 横幅
2. 客户端发送 `用户` 然后 `密码` 命令
3. 认证后客户端发送命令（`列表`、`下载`、`上传`、`切换` 等）
4. 文件传输时，打开新的 QUIC 流传输数据

## 关键依赖

- `quinn` - QUIC 协议实现
- `tokio` - 异步运行时
- `clap` - CLI 参数解析
- `argon2` - 密码哈希
- `serde` + `toml` - 配置序列化
