use quicftp_common::protocol::{Command, Response, ResponseCode};
use quicftp_common::user::verify_password;
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::session::Session;

/// Handle an FTP command and return a response
pub async fn handle_command(command: &Command, session: &mut Session) -> Response {
    match command {
        Command::User(username) => handle_user(username, session).await,
        Command::Pass(password) => handle_pass(password, session).await,
        Command::List => handle_list(session).await,
        Command::Get(path) => handle_get(path, session).await,
        Command::Put(path, size) => handle_put(path, *size, session).await,
        Command::Cd(path) => handle_cd(path, session).await,
        Command::Pwd => handle_pwd(session).await,
        Command::Mkdir(path) => handle_mkdir(path, session).await,
        Command::Rmdir(path) => handle_rmdir(path, session).await,
        Command::Del(path) => handle_del(path, session).await,
        Command::Ren { from, to } => handle_ren(from, to, session).await,
        Command::Size(path) => handle_size(path, session).await,
        Command::Quit => unreachable!("Quit should be handled in session"),
    }
}

async fn handle_user(username: &str, session: &mut Session) -> Response {
    // Check if user exists, then store pending username
    let user_exists = {
        let users = session.state().users.read().await;
        users.find_user(username).is_some()
    };

    if user_exists {
        // Store the pending username for later verification in PASS
        session.set_pending_username(username.to_string());
        Response::new(
            ResponseCode::PasswordRequired,
            format!("请输入密码: {}", username),
        )
    } else {
        Response::new(ResponseCode::LoginFailed, "未知用户")
    }
}

async fn handle_pass(password: &str, session: &mut Session) -> Response {
    // Get the pending username from the USER command
    let pending_user = session.pending_username().map(|s| s.to_string());

    debug!("密码命令收到, 待验证用户: {:?}", pending_user);

    let username = match pending_user {
        Some(u) if !u.is_empty() => u,
        _ => return Response::new(ResponseCode::LoginFailed, "请先发送用户命令"),
    };

    debug!("验证用户密码: {}", username);

    // Find the specific user and verify password
    let matched_user = {
        let users = session.state().users.read().await;
        match users.find_user(&username) {
            Some(user) => {
                debug!("找到用户: {}, 验证密码", user.username);
                let valid = verify_password(password, &user.password_hash).unwrap_or(false);
                debug!("密码验证: {}", valid);
                if valid {
                    Some((user.username.clone(), user.home_dir.clone()))
                } else {
                    None
                }
            }
            None => {
                debug!("用户未找到: {}", username);
                None
            }
        }
    };

    if let Some((username, home_dir)) = matched_user {
        let home = PathBuf::from(&home_dir);
        session.set_user(username.clone(), home.clone());

        // Create home directory if not exists
        let _ = std::fs::create_dir_all(&home);

        return Response::new(
            ResponseCode::LoginSuccessful,
            format!("欢迎, {}", username),
        );
    }

    // Clear pending username on failed login
    session.set_pending_username(String::new());
    Response::new(ResponseCode::LoginFailed, "登录失败")
}

async fn handle_list(session: &mut Session) -> Response {
    if !session.is_authenticated() {
        return Response::new(ResponseCode::LoginFailed, "请先登录");
    }

    let current = match session.full_path(".") {
        Some(p) => p,
        None => return Response::new(ResponseCode::FileNotFound, "无效路径"),
    };

    if !current.exists() {
        return Response::new(ResponseCode::FileNotFound, "目录不存在");
    }

    let mut entries = Vec::new();
    match std::fs::read_dir(&current) {
        Ok(dir) => {
            for entry in dir.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let metadata = entry.metadata();
                let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

                if is_dir {
                    entries.push(format!("📁 <目录> {}", name));
                } else {
                    entries.push(format!("📄 {:>10} 字节  {}", size, name));
                }
            }
        }
        Err(e) => {
            return Response::new(
                ResponseCode::FileNotFound,
                format!("无法读取目录: {}", e),
            );
        }
    }

    if entries.is_empty() {
        Response::new(ResponseCode::CommandOk, "(空目录)")
    } else {
        Response::new(ResponseCode::CommandOk, entries.join("\n"))
    }
}

async fn handle_get(path: &str, session: &mut Session) -> Response {
    if !session.is_authenticated() {
        return Response::new(ResponseCode::LoginFailed, "请先登录");
    }

    let full_path = match session.full_path(path) {
        Some(p) => p,
        None => return Response::new(ResponseCode::FileNotFound, "无效路径"),
    };

    if !full_path.exists() {
        return Response::new(ResponseCode::FileNotFound, "文件不存在");
    }

    if !full_path.is_file() {
        return Response::new(ResponseCode::FileNotFound, "不是文件");
    }

    let size = std::fs::metadata(&full_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Response::new(
        ResponseCode::DataConnectionOpening,
        format!("准备下载 {} ({} 字节)", path, size),
    )
}

async fn handle_put(path: &str, size: u64, session: &mut Session) -> Response {
    if !session.is_authenticated() {
        return Response::new(ResponseCode::LoginFailed, "请先登录");
    }

    let full_path = match session.full_path(path) {
        Some(p) => p,
        None => return Response::new(ResponseCode::FileNotFound, "无效路径"),
    };

    // Check if parent directory exists
    if let Some(parent) = full_path.parent() {
        if !parent.exists() {
            return Response::new(ResponseCode::FileNotFound, "父目录不存在");
        }
    }

    Response::new(
        ResponseCode::DataConnectionOpening,
        format!("准备上传 {} ({} 字节)", path, size),
    )
}

async fn handle_cd(path: &str, session: &mut Session) -> Response {
    if !session.is_authenticated() {
        return Response::new(ResponseCode::LoginFailed, "请先登录");
    }

    let new_dir = if path == "/" {
        PathBuf::from("/")
    } else if path == ".." {
        let current = session.current_dir();
        current
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"))
    } else if path.starts_with('/') {
        PathBuf::from(path)
    } else {
        session.current_dir().join(path)
    };

    // Verify the path exists on disk
    if let Some(full) = session.full_path(path) {
        if full.exists() && full.is_dir() {
            session.set_current_dir(new_dir);
            return Response::new(ResponseCode::CommandOk, "目录已切换");
        }
    }

    Response::new(ResponseCode::FileNotFound, "目录不存在")
}

async fn handle_pwd(session: &mut Session) -> Response {
    if !session.is_authenticated() {
        return Response::new(ResponseCode::LoginFailed, "请先登录");
    }

    Response::new(
        ResponseCode::PathCreated,
        format!("当前目录: {}", session.current_dir().display()),
    )
}

async fn handle_mkdir(path: &str, session: &mut Session) -> Response {
    if !session.is_authenticated() {
        return Response::new(ResponseCode::LoginFailed, "请先登录");
    }

    let full_path = match session.full_path(path) {
        Some(p) => p,
        None => return Response::new(ResponseCode::FileNotFound, "无效路径"),
    };

    match std::fs::create_dir_all(&full_path) {
        Ok(()) => Response::new(
            ResponseCode::PathCreated,
            format!("目录已创建: {}", path),
        ),
        Err(e) => Response::new(
            ResponseCode::FileNotFound,
            format!("无法创建目录: {}", e),
        ),
    }
}

async fn handle_rmdir(path: &str, session: &mut Session) -> Response {
    if !session.is_authenticated() {
        return Response::new(ResponseCode::LoginFailed, "请先登录");
    }

    let full_path = match session.full_path(path) {
        Some(p) => p,
        None => return Response::new(ResponseCode::FileNotFound, "无效路径"),
    };

    if !full_path.exists() {
        return Response::new(ResponseCode::FileNotFound, "目录不存在");
    }

    if !full_path.is_dir() {
        return Response::new(ResponseCode::FileNotFound, "不是目录");
    }

    match std::fs::remove_dir(&full_path) {
        Ok(()) => Response::new(ResponseCode::FileActionOk, "目录已删除"),
        Err(e) => Response::new(
            ResponseCode::FileNotFound,
            format!("无法删除目录: {}", e),
        ),
    }
}

async fn handle_del(path: &str, session: &mut Session) -> Response {
    if !session.is_authenticated() {
        return Response::new(ResponseCode::LoginFailed, "请先登录");
    }

    let full_path = match session.full_path(path) {
        Some(p) => p,
        None => return Response::new(ResponseCode::FileNotFound, "无效路径"),
    };

    if !full_path.exists() {
        return Response::new(ResponseCode::FileNotFound, "文件不存在");
    }

    if !full_path.is_file() {
        return Response::new(ResponseCode::FileNotFound, "不是文件");
    }

    match std::fs::remove_file(&full_path) {
        Ok(()) => Response::new(ResponseCode::FileActionOk, "文件已删除"),
        Err(e) => Response::new(
            ResponseCode::FileNotFound,
            format!("无法删除文件: {}", e),
        ),
    }
}

async fn handle_ren(from: &str, to: &str, session: &mut Session) -> Response {
    if !session.is_authenticated() {
        return Response::new(ResponseCode::LoginFailed, "请先登录");
    }

    let from_path = match session.full_path(from) {
        Some(p) => p,
        None => return Response::new(ResponseCode::FileNotFound, "无效源路径"),
    };

    let to_path = match session.full_path(to) {
        Some(p) => p,
        None => return Response::new(ResponseCode::FileNotFound, "无效目标路径"),
    };

    if !from_path.exists() {
        return Response::new(ResponseCode::FileNotFound, "源文件不存在");
    }

    match std::fs::rename(&from_path, &to_path) {
        Ok(()) => Response::new(ResponseCode::FileActionOk, "文件已重命名"),
        Err(e) => Response::new(
            ResponseCode::FileNotFound,
            format!("无法重命名: {}", e),
        ),
    }
}

async fn handle_size(path: &str, session: &mut Session) -> Response {
    if !session.is_authenticated() {
        return Response::new(ResponseCode::LoginFailed, "请先登录");
    }

    let full_path = match session.full_path(path) {
        Some(p) => p,
        None => return Response::new(ResponseCode::FileNotFound, "无效路径"),
    };

    if !full_path.exists() {
        return Response::new(ResponseCode::FileNotFound, "文件不存在");
    }

    match std::fs::metadata(&full_path) {
        Ok(metadata) => Response::new(
            ResponseCode::FileStatus,
            format!("{} 字节", metadata.len()),
        ),
        Err(e) => Response::new(
            ResponseCode::FileNotFound,
            format!("无法获取大小: {}", e),
        ),
    }
}
