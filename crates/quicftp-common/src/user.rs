use argon2::{
    password_hash::{
        rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    },
    Argon2,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// User permissions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Permission {
    #[serde(rename = "read")]
    Read,
    #[serde(rename = "write")]
    Write,
}

/// User data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub password_hash: String,
    pub home_dir: String,
    pub permissions: Vec<Permission>,
}

/// Users file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsersFile {
    pub users: Vec<User>,
}

impl UsersFile {
    /// Load users from a TOML file
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(UsersFile { users: Vec::new() });
        }
        let content = std::fs::read_to_string(path)?;
        let users: UsersFile = toml::from_str(&content)?;
        Ok(users)
    }

    /// Save users to a TOML file
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Find a user by username
    pub fn find_user(&self, username: &str) -> Option<&User> {
        self.users.iter().find(|u| u.username == username)
    }

    /// Add a new user
    pub fn add_user(
        &mut self,
        username: String,
        password: &str,
        home_dir: String,
        permissions: Vec<Permission>,
    ) -> anyhow::Result<()> {
        if self.users.iter().any(|u| u.username == username) {
            anyhow::bail!("User '{}' already exists", username);
        }

        let password_hash = hash_password(password)?;
        self.users.push(User {
            username,
            password_hash,
            home_dir,
            permissions,
        });
        Ok(())
    }

    /// Remove a user
    pub fn remove_user(&mut self, username: &str) -> bool {
        let len = self.users.len();
        self.users.retain(|u| u.username != username);
        self.users.len() < len
    }
}

/// Hash a password using Argon2id
pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Password hashing failed: {}", e))?
        .to_string();
    Ok(password_hash)
}

/// Verify a password against a hash
pub fn verify_password(password: &str, hash: &str) -> anyhow::Result<bool> {
    let parsed_hash =
        PasswordHash::new(hash).map_err(|e| anyhow::anyhow!("Invalid password hash: {}", e))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hash_and_verify() {
        let password = "test_password_123";
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_add_and_find_user() {
        let mut users = UsersFile { users: Vec::new() };
        users
            .add_user(
                "alice".to_string(),
                "password123",
                "/home/alice".to_string(),
                vec![Permission::Read, Permission::Write],
            )
            .unwrap();

        assert!(users.find_user("alice").is_some());
        assert!(users.find_user("bob").is_none());
    }

    #[test]
    fn test_duplicate_user() {
        let mut users = UsersFile { users: Vec::new() };
        users
            .add_user(
                "alice".to_string(),
                "pass1",
                "/home/alice".to_string(),
                vec![Permission::Read],
            )
            .unwrap();

        let result = users.add_user(
            "alice".to_string(),
            "pass2",
            "/home/alice2".to_string(),
            vec![Permission::Read],
        );
        assert!(result.is_err());
    }
}
