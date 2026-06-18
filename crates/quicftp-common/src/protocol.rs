use serde::{Deserialize, Serialize};
use std::fmt;

/// FTP commands supported by QuicFTP (Chinese)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    /// 用户名
    User(String),
    /// 密码
    Pass(String),
    /// 列出目录
    List,
    /// 下载文件
    Get(String),
    /// 上传文件
    Put(String, u64), // (远程文件名, 文件大小)
    /// 切换目录
    Cd(String),
    /// 当前目录
    Pwd,
    /// 创建目录
    Mkdir(String),
    /// 删除目录
    Rmdir(String),
    /// 删除文件
    Del(String),
    /// 重命名
    Ren { from: String, to: String },
    /// 文件大小
    Size(String),
    /// 退出
    Quit,
}

impl Command {
    /// Format command for sending over the network (includes actual password)
    pub fn to_send_string(&self) -> String {
        match self {
            Command::User(name) => format!("用户 {}", name),
            Command::Pass(pass) => format!("密码 {}", pass),
            Command::List => "列表".to_string(),
            Command::Get(path) => format!("下载 {}", path),
            Command::Put(path, size) => format!("上传 {} {}", path, size),
            Command::Cd(path) => format!("切换 {}", path),
            Command::Pwd => "当前目录".to_string(),
            Command::Mkdir(path) => format!("创建目录 {}", path),
            Command::Rmdir(path) => format!("删除目录 {}", path),
            Command::Del(path) => format!("删除 {}", path),
            Command::Ren { from, to } => format!("重命名 {} {}", from, to),
            Command::Size(path) => format!("大小 {}", path),
            Command::Quit => "退出".to_string(),
        }
    }

    /// Parse a command string into a Command
    pub fn parse(input: &str) -> Result<Self, ProtocolError> {
        let input = input.trim();
        let (cmd, args) = match input.find(' ') {
            Some(pos) => (&input[..pos], input[pos + 1..].trim()),
            None => (input, ""),
        };

        // Support both Chinese and English commands
        match cmd {
            // Chinese commands
            "用户" | "USER" => Ok(Command::User(args.to_string())),
            "密码" | "PASS" => Ok(Command::Pass(args.to_string())),
            "列表" | "LIST" | "ls" | "LS" => Ok(Command::List),
            "下载" | "GET" | "RETR" => Ok(Command::Get(args.to_string())),
            "上传" | "PUT" | "STOR" => {
                let parts: Vec<&str> = args.splitn(2, ' ').collect();
                if parts.len() == 2 {
                    let size: u64 = parts[1].parse().unwrap_or(0);
                    Ok(Command::Put(parts[0].to_string(), size))
                } else {
                    Ok(Command::Put(args.to_string(), 0))
                }
            }
            "切换" | "CWD" | "CD" => Ok(Command::Cd(args.to_string())),
            "当前目录" | "PWD" => Ok(Command::Pwd),
            "创建目录" | "MKD" | "MKDIR" => Ok(Command::Mkdir(args.to_string())),
            "删除目录" | "RMD" | "RMDIR" => Ok(Command::Rmdir(args.to_string())),
            "删除" | "DELE" | "DEL" | "RM" => Ok(Command::Del(args.to_string())),
            "重命名" | "REN" | "RENAME" | "MV" => {
                let parts: Vec<&str> = args.splitn(2, ' ').collect();
                if parts.len() != 2 {
                    return Err(ProtocolError::InvalidCommand(
                        "重命名需要两个参数: 重命名 <原文件名> <新文件名>".to_string(),
                    ));
                }
                Ok(Command::Ren {
                    from: parts[0].to_string(),
                    to: parts[1].to_string(),
                })
            }
            "大小" | "SIZE" => Ok(Command::Size(args.to_string())),
            "退出" | "QUIT" | "BYE" | "exit" => Ok(Command::Quit),
            _ => Err(ProtocolError::UnknownCommand(cmd.to_string())),
        }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::User(name) => write!(f, "用户 {}", name),
            Command::Pass(_) => write!(f, "密码 ****"),
            Command::List => write!(f, "列表"),
            Command::Get(path) => write!(f, "下载 {}", path),
            Command::Put(path, size) => write!(f, "上传 {} {}", path, size),
            Command::Cd(path) => write!(f, "切换 {}", path),
            Command::Pwd => write!(f, "当前目录"),
            Command::Mkdir(path) => write!(f, "创建目录 {}", path),
            Command::Rmdir(path) => write!(f, "删除目录 {}", path),
            Command::Del(path) => write!(f, "删除 {}", path),
            Command::Ren { from, to } => write!(f, "重命名 {} {}", from, to),
            Command::Size(path) => write!(f, "大小 {}", path),
            Command::Quit => write!(f, "退出"),
        }
    }
}

/// FTP response status codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseCode {
    /// 准备传输数据
    DataConnectionOpening = 150,
    /// 命令成功
    CommandOk = 200,
    /// 服务就绪
    ServiceReady = 220,
    /// 连接关闭
    ConnectionClosing = 221,
    /// 传输完成
    TransferComplete = 226,
    /// 登录成功
    LoginSuccessful = 230,
    /// 文件操作成功
    FileActionOk = 250,
    /// 路径已创建
    PathCreated = 257,
    /// 需要密码
    PasswordRequired = 331,
    /// 文件状态
    FileStatus = 213,
    /// 语法错误
    SyntaxError = 501,
    /// 命令未实现
    NotImplemented = 502,
    /// 登录失败
    LoginFailed = 530,
    /// 文件未找到
    FileNotFound = 550,
}

/// FTP response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub code: u16,
    pub message: String,
}

impl Response {
    pub fn new(code: ResponseCode, message: impl Into<String>) -> Self {
        Self {
            code: code as u16,
            message: message.into(),
        }
    }

    /// Format response as FTP-style string: "code message"
    pub fn format(&self) -> String {
        format!("{} {}", self.code, self.message)
    }

    /// Parse a response string
    pub fn parse(input: &str) -> Result<Self, ProtocolError> {
        let input = input.trim();
        let space_pos = input
            .find(' ')
            .ok_or_else(|| ProtocolError::InvalidResponse(input.to_string()))?;
        let code: u16 = input[..space_pos]
            .parse()
            .map_err(|_| ProtocolError::InvalidResponse(input.to_string()))?;
        let message = input[space_pos + 1..].to_string();
        Ok(Self { code, message })
    }
}

/// Protocol errors
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("未知命令: {0}")]
    UnknownCommand(String),
    #[error("无效命令: {0}")]
    InvalidCommand(String),
    #[error("无效响应: {0}")]
    InvalidResponse(String),
    #[error("连接已关闭")]
    ConnectionClosed,
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_chinese_commands() {
        assert!(matches!(Command::parse("用户 alice"), Ok(Command::User(name)) if name == "alice"));
        assert!(matches!(Command::parse("密码 secret"), Ok(Command::Pass(p)) if p == "secret"));
        assert!(matches!(Command::parse("列表"), Ok(Command::List)));
        assert!(matches!(Command::parse("下载 file.txt"), Ok(Command::Get(f)) if f == "file.txt"));
        assert!(matches!(Command::parse("上传 file.txt 100"), Ok(Command::Put(f, 100)) if f == "file.txt"));
        assert!(matches!(Command::parse("切换 /home"), Ok(Command::Cd(d)) if d == "/home"));
        assert!(matches!(Command::parse("当前目录"), Ok(Command::Pwd)));
        assert!(matches!(Command::parse("退出"), Ok(Command::Quit)));
    }

    #[test]
    fn test_parse_english_commands() {
        assert!(matches!(Command::parse("USER alice"), Ok(Command::User(name)) if name == "alice"));
        assert!(matches!(Command::parse("PASS secret"), Ok(Command::Pass(p)) if p == "secret"));
        assert!(matches!(Command::parse("LIST"), Ok(Command::List)));
        assert!(matches!(Command::parse("GET file.txt"), Ok(Command::Get(f)) if f == "file.txt"));
        assert!(matches!(Command::parse("PUT file.txt 100"), Ok(Command::Put(f, 100)) if f == "file.txt"));
        assert!(matches!(Command::parse("PWD"), Ok(Command::Pwd)));
        assert!(matches!(Command::parse("QUIT"), Ok(Command::Quit)));
    }

    #[test]
    fn test_parse_unknown_command() {
        assert!(matches!(
            Command::parse("UNKNOWN"),
            Err(ProtocolError::UnknownCommand(_))
        ));
    }

    #[test]
    fn test_response_format() {
        let resp = Response::new(ResponseCode::LoginSuccessful, "登录成功");
        assert_eq!(resp.format(), "230 登录成功");
    }

    #[test]
    fn test_response_parse() {
        let resp = Response::parse("230 登录成功").unwrap();
        assert_eq!(resp.code, 230);
        assert_eq!(resp.message, "登录成功");
    }
}
