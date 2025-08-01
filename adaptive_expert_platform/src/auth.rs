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
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,  // Subject (user ID)
    pub exp: usize,   // Expiration time
    pub iat: usize,   // Issued at
    pub roles: Vec<String>, // User roles
}

/// User authentication information
#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub roles: Vec<String>,
    pub active: bool,
}

/// Authentication manager
#[derive(Clone)]
pub struct AuthManager {
    users: Arc<RwLock<HashMap<String, User>>>,
    jwt_secret: String,
    jwt_expiry_hours: usize,
}

impl AuthManager {
    pub fn new(jwt_secret: String) -> Self {
        let users = HashMap::new();

        Self {
            users: Arc::new(RwLock::new(users)),
            jwt_secret,
            jwt_expiry_hours: 24, // 24 hour expiry
        }
    }

    /// Initialize the first admin user during setup
    pub async fn initialize_admin(&self, username: String, password: &str) -> Result<()> {
        let users = self.users.read().await;
        if !users.is_empty() {
            return Err(anyhow!("Admin user already exists. Cannot reinitialize."));
        }
        drop(users);

        let password_hash = Self::hash_password(password)?;
        let mut users = self.users.write().await;
        users.insert(username.clone(), User {
            id: username.clone(),
            username,
            password_hash,
            roles: vec!["admin".to_string(), "user".to_string()],
            active: true,
        });

        Ok(())
    }

    /// Check if admin user exists
    pub async fn has_admin(&self) -> bool {
        let users = self.users.read().await;
        users.values().any(|user| user.roles.contains(&"admin".to_string()))
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
        let users = self.users.read().await;

        let user = users.get(username)
            .ok_or_else(|| anyhow!("User not found"))?;

        if !user.active {
            return Err(anyhow!("User account is disabled"));
        }

        if !Self::verify_password(password, &user.password_hash)? {
            warn!("Failed authentication attempt for user: {}", username);
            return Err(anyhow!("Invalid credentials"));
        }

        info!("Successful authentication for user: {}", username);
        self.generate_token(user).await
    }

    /// Generate JWT token for user
    async fn generate_token(&self, user: &User) -> Result<String> {
        let now = chrono::Utc::now();
        let exp = now + chrono::Duration::hours(self.jwt_expiry_hours as i64);

        let claims = Claims {
            sub: user.id.clone(),
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
            roles: user.roles.clone(),
        };

        let header = Header::new(Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(self.jwt_secret.as_ref());

        encode(&header, &claims, &encoding_key)
            .map_err(|e| anyhow!("Token generation failed: {}", e))
    }

    /// Validate JWT token and extract claims
    pub fn validate_token(&self, token: &str) -> Result<Claims> {
        let decoding_key = DecodingKey::from_secret(self.jwt_secret.as_ref());
        let validation = Validation::new(Algorithm::HS256);

        let token_data = decode::<Claims>(token, &decoding_key, &validation)
            .map_err(|e| anyhow!("Token validation failed: {}", e))?;

        Ok(token_data.claims)
    }

    /// Check if user has required role
    pub fn has_role(&self, claims: &Claims, required_role: &str) -> bool {
        claims.roles.contains(&required_role.to_string())
    }

    /// Add new user (admin only)
    pub async fn add_user(&self, username: String, password: &str, roles: Vec<String>) -> Result<()> {
        let mut users = self.users.write().await;

        if users.contains_key(&username) {
            return Err(anyhow!("User already exists"));
        }

        let password_hash = Self::hash_password(password)?;
        let user = User {
            id: username.clone(),
            username: username.clone(),
            password_hash,
            roles,
            active: true,
        };

        users.insert(username, user);
        Ok(())
    }

    /// Update user password
    pub async fn update_password(&self, username: &str, new_password: &str) -> Result<()> {
        let mut users = self.users.write().await;

        let user = users.get_mut(username)
            .ok_or_else(|| anyhow!("User not found"))?;

        user.password_hash = Self::hash_password(new_password)?;
        Ok(())
    }

    /// Disable user account
    pub async fn disable_user(&self, username: &str) -> Result<()> {
        let mut users = self.users.write().await;

        let user = users.get_mut(username)
            .ok_or_else(|| anyhow!("User not found"))?;

        user.active = false;
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
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip authentication for health endpoint
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    let token = match extract_token(&headers) {
        Ok(token) => token,
        Err(_) => {
            warn!("Unauthorized request to {}", request.uri().path());
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    match auth_manager.validate_token(&token) {
        Ok(claims) => {
            // Add claims to request extensions for downstream use
            let mut request = request;
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

    #[tokio::test]
    async fn test_password_hashing() {
        let password = "test123";
        let hash = AuthManager::hash_password(password).unwrap();
        assert!(AuthManager::verify_password(password, &hash).unwrap());
        assert!(!AuthManager::verify_password("wrong", &hash).unwrap());
    }

    #[tokio::test]
    async fn test_authentication() {
        let auth_manager = AuthManager::new("test_secret".to_string());

        // Test successful authentication
        let token = auth_manager.authenticate("admin", "admin123").await.unwrap();
        let claims = auth_manager.validate_token(&token).unwrap();
        assert_eq!(claims.sub, "admin");
        assert!(claims.roles.contains(&"admin".to_string()));

        // Test failed authentication
        assert!(auth_manager.authenticate("admin", "wrong").await.is_err());
    }
}
