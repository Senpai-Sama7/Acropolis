//! Authentication and authorization system with JWT support and secure password handling.

use anyhow::{anyhow, Result};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::{SqlitePool, SqlitePoolOptions}, Row};
use std::{sync::Arc, time::{Duration, Instant}};
use tracing::{error, info, warn};
use dashmap::DashMap;
use secrecy::{Secret, ExposeSecret};
use crate::settings::SecurityConfig;

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,  // Subject (user ID)
    pub exp: usize,   // Expiration time
    pub iat: usize,   // Issued at
    pub roles: Vec<String>, // User roles
}

/// User authentication information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub roles: Vec<String>,
    pub active: bool,
}

/// Authentication manager backed by an asynchronous SQLite database with caching
#[derive(Clone)]
pub struct AuthManager {
    pool: SqlitePool,
    cache: Arc<DashMap<String, User>>,
    jwt_secret: Secret<String>,
    jwt_expiry_hours: usize,
    token_blacklist: Arc<DashMap<String, usize>>,
    login_attempts: Arc<DashMap<String, (u32, Option<Instant>)>>,
    max_login_attempts: u32,
    lockout_duration: Duration,
}

impl AuthManager {
    /// Creates a new AuthManager with an asynchronous SQLite pool.
    pub async fn new(jwt_secret: Secret<String>, db_url: &str, security: &SecurityConfig) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(db_url)
            .await
            .map_err(|e| anyhow!("Failed to connect to auth database at '{}': {}", db_url, e))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS users (\n\
             id TEXT PRIMARY KEY,\n\
             username TEXT NOT NULL UNIQUE,\n\
             password_hash TEXT NOT NULL,\n\
             roles TEXT NOT NULL,\n\
             active INTEGER NOT NULL\n\
            )"
        ).execute(&pool).await?;

        let cache = Arc::new(DashMap::new());

        info!("Authentication database connected at '{}'", db_url);
        Ok(Self {
            pool,
            cache,
            jwt_secret,
            jwt_expiry_hours: security.jwt_expiry_hours,
            token_blacklist: Arc::new(DashMap::new()),
            login_attempts: Arc::new(DashMap::new()),
            max_login_attempts: security.max_login_attempts,
            lockout_duration: Duration::from_secs(security.lockout_duration_minutes * 60),
        })
    }

    /// Initialize the first admin user during setup
    pub async fn initialize_admin(&self, username: String, password: &str) -> Result<()> {
        if self.has_admin().await? {
            return Err(anyhow!("Admin user already exists. Cannot reinitialize."));
        }
        self.add_user(username, password, vec!["admin".to_string(), "user".to_string()]).await
    }

    /// Check if an admin user exists in the database
    pub async fn has_admin(&self) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM users WHERE roles LIKE '%admin%' LIMIT 1")
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    /// Hash a password securely using Argon2
    pub fn hash_password(password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let password_hash = argon2.hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("Password hashing failed: {}", e))?;

        Ok(password_hash.to_string())
    }

    /// Verify a password against its hash
    pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| anyhow!("Invalid password hash: {}", e))?;

        let argon2 = Argon2::default();
        Ok(argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok())
    }

    /// Authenticate user and return JWT token
    pub async fn authenticate(&self, username: &str, password: &str) -> Result<String> {
        let user = self.get_user(username).await?;

        if !user.active {
            return Err(anyhow!("User account is disabled"));
        }

        // Check lockout status
        if let Some((_, Some(until))) = self.login_attempts.get(username).map(|v| *v) {
            if until > Instant::now() {
                warn!("Locked account login attempt: {}", username);
                return Err(anyhow!("Account temporarily locked"));
            }
        }

        if !Self::verify_password(password, &user.password_hash)? {
            warn!("Failed authentication attempt for user: {}", username);
            let mut entry = self.login_attempts.entry(username.to_string()).or_insert((0, None));
            entry.0 += 1;
            if entry.0 >= self.max_login_attempts {
                entry.1 = Some(Instant::now() + self.lockout_duration);
                warn!("User {} locked out due to too many attempts", username);
            }
            return Err(anyhow!("Invalid credentials"));
        }

        // Successful authentication resets attempt counter
        self.login_attempts.remove(username);

        info!("Successful authentication for user: {}", username);
        self.generate_token(&user)
    }

    /// Generate JWT token for user
    fn generate_token(&self, user: &User) -> Result<String> {
        let now = chrono::Utc::now();
        let exp = now + chrono::Duration::hours(self.jwt_expiry_hours as i64);

        let claims = Claims {
            sub: user.id.clone(),
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
            roles: user.roles.clone(),
        };

        let header = Header::new(Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(self.jwt_secret.expose_secret().as_ref());

        encode(&header, &claims, &encoding_key)
            .map_err(|e| anyhow!("Token generation failed: {}", e))
    }

    /// Validate JWT token and extract claims
    pub fn validate_token(&self, token: &str) -> Result<Claims> {
        if self.is_token_revoked(token) {
            return Err(anyhow!("Token has been revoked"));
        }

        let decoding_key = DecodingKey::from_secret(self.jwt_secret.expose_secret().as_ref());
        let validation = Validation::new(Algorithm::HS256);

        let token_data = decode::<Claims>(token, &decoding_key, &validation)
            .map_err(|e| anyhow!("Token validation failed: {}", e))?;

        Ok(token_data.claims)
    }

    /// Revoke a token by adding it to the blacklist until its expiry
    pub fn revoke_token(&self, token: &str) -> Result<()> {
        let decoding_key = DecodingKey::from_secret(self.jwt_secret.expose_secret().as_ref());
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false;
        let data = decode::<Claims>(token, &decoding_key, &validation)
            .map_err(|e| anyhow!("Invalid token: {}", e))?;
        self.token_blacklist.insert(token.to_string(), data.claims.exp);
        Ok(())
    }

    fn is_token_revoked(&self, token: &str) -> bool {
        let now = chrono::Utc::now().timestamp() as usize;
        if let Some(exp) = self.token_blacklist.get(token).map(|v| *v) {
            if exp > now {
                return true;
            }
            self.token_blacklist.remove(token);
        }
        false
    }

    /// Check if user has required role
    pub fn has_role(&self, claims: &Claims, required_role: &str) -> bool {
        claims.roles.contains(&required_role.to_string())
    }

    /// Add new user (admin only)
    pub async fn add_user(&self, username: String, password: &str, roles: Vec<String>) -> Result<()> {
        let password_hash = Self::hash_password(password)?;
        let roles_json = serde_json::to_string(&roles)?;
        sqlx::query("INSERT INTO users (id, username, password_hash, roles, active) VALUES (?, ?, ?, ?, 1)")
            .bind(&username)
            .bind(&username)
            .bind(&password_hash)
            .bind(&roles_json)
            .execute(&self.pool)
            .await?;
        let user = User { id: username.clone(), username: username.clone(), password_hash, roles, active: true };
        self.cache.insert(username, user);
        Ok(())
    }

    /// Update user password
    pub async fn update_password(&self, username: &str, new_password: &str) -> Result<()> {
        let mut user = self.get_user(username).await?;
        user.password_hash = Self::hash_password(new_password)?;
        self.update_user(&user).await
    }

    /// Disable user account
    pub async fn disable_user(&self, username: &str) -> Result<()> {
        let mut user = self.get_user(username).await?;
        user.active = false;
        self.update_user(&user).await
    }

    async fn get_user(&self, username: &str) -> Result<User> {
        if let Some(user) = self.cache.get(username).map(|u| u.clone()) {
            return Ok(user);
        }
        let row = sqlx::query("SELECT id, username, password_hash, roles, active FROM users WHERE username = ?")
            .bind(username)
            .fetch_one(&self.pool)
            .await
            .map_err(|_| anyhow!("User not found"))?;
        let roles: Vec<String> = serde_json::from_str(row.get::<String, _>("roles").as_str())?;
        let active: bool = row.get::<i64, _>("active") != 0;
        let user = User {
            id: row.get("id"),
            username: row.get("username"),
            password_hash: row.get("password_hash"),
            roles,
            active,
        };
        self.cache.insert(user.username.clone(), user.clone());
        Ok(user)
    }

    async fn update_user(&self, user: &User) -> Result<()> {
        let roles_json = serde_json::to_string(&user.roles)?;
        sqlx::query("UPDATE users SET password_hash = ?, roles = ?, active = ? WHERE username = ?")
            .bind(&user.password_hash)
            .bind(&roles_json)
            .bind(if user.active { 1 } else { 0 })
            .bind(&user.username)
            .execute(&self.pool)
            .await?;
        self.cache.insert(user.username.clone(), user.clone());
        Ok(())
    }
}

/// Extract JWT token from Authorization header
pub fn extract_token(headers: &HeaderMap) -> Result<String> {
    let auth_header = headers.get("Authorization")
        .ok_or_else(|| anyhow!("Missing Authorization header"))?
        .to_str()
        .map_err(|_| anyhow!("Invalid Authorization header"))?;

    if !auth_header.starts_with("Bearer ") {
        return Err(anyhow!("Invalid Authorization format"));
    }

    Ok(auth_header[7..].to_string())
}

/// Authentication middleware
pub async fn auth_middleware(
    State(auth_manager): State<Arc<AuthManager>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip authentication for health endpoint
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    let token = match extract_token(request.headers()) {
        Ok(token) => token,
        Err(_) => {
            warn!("Unauthorized request to {}", request.uri().path());
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    match auth_manager.validate_token(&token) {
        Ok(claims) => {
            // Add claims to request extensions for downstream use
            request.extensions_mut().insert(claims);
            Ok(next.run(request).await)
        }
        Err(e) => {
            error!("Token validation failed: {}", e);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Role-based authorization middleware
pub fn require_role(required_role: &'static str) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, StatusCode>> + Send>> + Clone {
    move |request: Request, next: Next| {
        Box::pin(async move {
            let claims = request.extensions().get::<Claims>()
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

            if !claims.roles.contains(&required_role.to_string()) {
                warn!("Insufficient permissions for user {} to access {}", claims.sub, request.uri().path());
                return Err(StatusCode::FORBIDDEN);
            }

            Ok(next.run(request).await)
        })
    }
}

/// Login request structure
#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response structure
#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_in: usize,
    pub user_id: String,
    pub roles: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_auth_manager() -> AuthManager {
        AuthManager::new(Secret::new("test_secret".to_string()), "sqlite::memory:", &SecurityConfig::default()).await.unwrap()
    }

    #[test]
    fn test_password_hashing() {
        let password = "test_password_123!";
        let hash = AuthManager::hash_password(password).unwrap();
        assert!(AuthManager::verify_password(password, &hash).unwrap());
        assert!(!AuthManager::verify_password("wrong_password", &hash).unwrap());
    }

    #[tokio::test]
    async fn test_user_management() {
        let auth_manager = create_test_auth_manager().await;
        let username = "test_user".to_string();
        let password = "test_password_123!";
        let roles = vec!["user".to_string()];

        auth_manager.add_user(username.clone(), password, roles.clone()).await.unwrap();

        let token = auth_manager.authenticate(&username, password).await.unwrap();
        let claims = auth_manager.validate_token(&token).unwrap();
        assert_eq!(claims.sub, username);
        assert_eq!(claims.roles, roles);

        auth_manager.disable_user(&username).await.unwrap();
        let err = auth_manager.authenticate(&username, password).await.unwrap_err();
        assert_eq!(err.to_string(), "User account is disabled");
    }

    #[tokio::test]
    async fn test_admin_initialization() {
        let auth_manager = create_test_auth_manager().await;
        assert!(!auth_manager.has_admin().await.unwrap());

        auth_manager.initialize_admin("admin".to_string(), "admin_password").await.unwrap();
        assert!(auth_manager.has_admin().await.unwrap());

        assert!(auth_manager.initialize_admin("admin2".to_string(), "admin_password2").await.is_err());
    }
}
