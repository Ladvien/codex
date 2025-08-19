use crate::security::{Claims, RbacConfig, Result, SecurityError};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, warn};

/// Role-Based Access Control manager
pub struct RbacManager {
    config: RbacConfig,
    role_permissions: HashMap<String, HashSet<String>>,
    user_roles: HashMap<String, String>,
    admin_users: HashSet<String>,
}

/// Permission check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionCheck {
    pub allowed: bool,
    pub user_role: String,
    pub required_permission: String,
    pub user_permissions: Vec<String>,
    pub reason: String,
}

/// Resource permission requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePermission {
    pub resource: String,
    pub action: String,
    pub required_permissions: Vec<String>,
    pub allow_admin_override: bool,
}

impl RbacManager {
    pub fn new(config: RbacConfig) -> Self {
        let mut manager = Self {
            role_permissions: HashMap::new(),
            user_roles: HashMap::new(),
            admin_users: HashSet::new(),
            config: config.clone(),
        };

        if config.enabled {
            manager.initialize_roles();
        }

        manager
    }

    fn initialize_roles(&mut self) {
        // Load role permissions from config
        for (role, permissions) in &self.config.roles {
            let perm_set: HashSet<String> = permissions.iter().cloned().collect();
            self.role_permissions.insert(role.clone(), perm_set);
        }

        // Load admin users
        for admin_user in &self.config.admin_users {
            self.admin_users.insert(admin_user.clone());
        }

        debug!(
            "Initialized RBAC with {} roles and {} admin users",
            self.role_permissions.len(),
            self.admin_users.len()
        );
    }

    /// Check if user has specific permission
    pub fn check_permission(&self, user_id: &str, permission: &str) -> PermissionCheck {
        if !self.config.enabled {
            return PermissionCheck {
                allowed: true,
                user_role: "none".to_string(),
                required_permission: permission.to_string(),
                user_permissions: vec!["all".to_string()],
                reason: "RBAC disabled".to_string(),
            };
        }

        // Check if user is admin
        if self.admin_users.contains(user_id) {
            return PermissionCheck {
                allowed: true,
                user_role: "admin".to_string(),
                required_permission: permission.to_string(),
                user_permissions: vec!["admin".to_string()],
                reason: "Admin override".to_string(),
            };
        }

        // Get user role
        let user_role = self.get_user_role(user_id);

        // Get user permissions
        let user_permissions = self.get_user_permissions(user_id);

        // Check if user has the required permission
        let allowed = user_permissions.contains(&permission.to_string())
            || user_permissions.contains(&"*".to_string());

        let reason = if allowed {
            "Permission granted".to_string()
        } else {
            format!("Missing required permission: {permission}")
        };

        PermissionCheck {
            allowed,
            user_role,
            required_permission: permission.to_string(),
            user_permissions,
            reason,
        }
    }

    /// Check multiple permissions (user must have ALL of them)
    pub fn check_permissions(&self, user_id: &str, permissions: &[&str]) -> PermissionCheck {
        if permissions.is_empty() {
            return PermissionCheck {
                allowed: true,
                user_role: self.get_user_role(user_id),
                required_permission: "none".to_string(),
                user_permissions: self.get_user_permissions(user_id),
                reason: "No permissions required".to_string(),
            };
        }

        for permission in permissions {
            let check = self.check_permission(user_id, permission);
            if !check.allowed {
                return check;
            }
        }

        PermissionCheck {
            allowed: true,
            user_role: self.get_user_role(user_id),
            required_permission: permissions.join(", "),
            user_permissions: self.get_user_permissions(user_id),
            reason: "All permissions granted".to_string(),
        }
    }

    /// Check if user can access resource with specific action
    pub fn check_resource_access(
        &self,
        user_id: &str,
        resource: &str,
        action: &str,
    ) -> PermissionCheck {
        let permission = format!("{resource}:{action}");
        let check = self.check_permission(user_id, &permission);

        if !check.allowed {
            // Try wildcard permissions
            let resource_wildcard = format!("{resource}:*");
            let wildcard_check = self.check_permission(user_id, &resource_wildcard);

            if wildcard_check.allowed {
                return PermissionCheck {
                    allowed: true,
                    user_role: wildcard_check.user_role,
                    required_permission: permission,
                    user_permissions: wildcard_check.user_permissions,
                    reason: "Wildcard permission granted".to_string(),
                };
            }
        }

        check
    }

    /// Assign role to user
    pub fn assign_role(&mut self, user_id: &str, role: &str) -> Result<()> {
        if !self.config.enabled {
            return Err(SecurityError::AuthorizationFailed {
                message: "RBAC is disabled".to_string(),
            });
        }

        if !self.role_permissions.contains_key(role) {
            return Err(SecurityError::AuthorizationFailed {
                message: format!("Role '{role}' does not exist"),
            });
        }

        self.user_roles
            .insert(user_id.to_string(), role.to_string());
        debug!("Assigned role '{}' to user '{}'", role, user_id);
        Ok(())
    }

    /// Remove role from user
    pub fn remove_user_role(&mut self, user_id: &str) -> Result<()> {
        if !self.config.enabled {
            return Err(SecurityError::AuthorizationFailed {
                message: "RBAC is disabled".to_string(),
            });
        }

        if self.user_roles.remove(user_id).is_some() {
            debug!("Removed role from user '{}'", user_id);
            Ok(())
        } else {
            Err(SecurityError::AuthorizationFailed {
                message: format!("User '{user_id}' has no role assigned"),
            })
        }
    }

    /// Add permission to role
    pub fn add_permission_to_role(&mut self, role: &str, permission: &str) -> Result<()> {
        if !self.config.enabled {
            return Err(SecurityError::AuthorizationFailed {
                message: "RBAC is disabled".to_string(),
            });
        }

        let permissions = self.role_permissions.entry(role.to_string()).or_default();
        permissions.insert(permission.to_string());
        debug!("Added permission '{}' to role '{}'", permission, role);
        Ok(())
    }

    /// Remove permission from role
    pub fn remove_permission_from_role(&mut self, role: &str, permission: &str) -> Result<()> {
        if !self.config.enabled {
            return Err(SecurityError::AuthorizationFailed {
                message: "RBAC is disabled".to_string(),
            });
        }

        if let Some(permissions) = self.role_permissions.get_mut(role) {
            if permissions.remove(permission) {
                debug!("Removed permission '{}' from role '{}'", permission, role);
                Ok(())
            } else {
                Err(SecurityError::AuthorizationFailed {
                    message: format!("Role '{role}' does not have permission '{permission}'"),
                })
            }
        } else {
            Err(SecurityError::AuthorizationFailed {
                message: format!("Role '{role}' does not exist"),
            })
        }
    }

    /// Create a new role
    pub fn create_role(&mut self, role: &str, permissions: Vec<String>) -> Result<()> {
        if !self.config.enabled {
            return Err(SecurityError::AuthorizationFailed {
                message: "RBAC is disabled".to_string(),
            });
        }

        if self.role_permissions.contains_key(role) {
            return Err(SecurityError::AuthorizationFailed {
                message: format!("Role '{role}' already exists"),
            });
        }

        let perm_set: HashSet<String> = permissions.into_iter().collect();
        self.role_permissions.insert(role.to_string(), perm_set);
        debug!("Created new role '{}'", role);
        Ok(())
    }

    /// Delete a role
    pub fn delete_role(&mut self, role: &str) -> Result<()> {
        if !self.config.enabled {
            return Err(SecurityError::AuthorizationFailed {
                message: "RBAC is disabled".to_string(),
            });
        }

        // Don't allow deletion of default roles
        if role == "admin" || role == "user" {
            return Err(SecurityError::AuthorizationFailed {
                message: "Cannot delete built-in roles".to_string(),
            });
        }

        // Remove role from all users
        self.user_roles.retain(|_, user_role| user_role != role);

        // Remove role permissions
        if self.role_permissions.remove(role).is_some() {
            debug!("Deleted role '{}'", role);
            Ok(())
        } else {
            Err(SecurityError::AuthorizationFailed {
                message: format!("Role '{role}' does not exist"),
            })
        }
    }

    /// Get user's role
    pub fn get_user_role(&self, user_id: &str) -> String {
        if self.admin_users.contains(user_id) {
            "admin".to_string()
        } else {
            self.user_roles
                .get(user_id)
                .cloned()
                .unwrap_or_else(|| self.config.default_role.clone())
        }
    }

    /// Get user's permissions
    pub fn get_user_permissions(&self, user_id: &str) -> Vec<String> {
        if !self.config.enabled {
            return vec!["*".to_string()];
        }

        let user_role = self.get_user_role(user_id);

        if let Some(permissions) = self.role_permissions.get(&user_role) {
            permissions.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Get all roles
    pub fn get_roles(&self) -> HashMap<String, Vec<String>> {
        self.role_permissions
            .iter()
            .map(|(role, permissions)| (role.clone(), permissions.iter().cloned().collect()))
            .collect()
    }

    /// Get all users and their roles
    pub fn get_user_roles(&self) -> HashMap<String, String> {
        let mut all_users = self.user_roles.clone();

        // Add admin users
        for admin_user in &self.admin_users {
            all_users.insert(admin_user.clone(), "admin".to_string());
        }

        all_users
    }

    /// Check if user is admin
    pub fn is_admin(&self, user_id: &str) -> bool {
        self.admin_users.contains(user_id)
    }

    /// Add admin user
    pub fn add_admin(&mut self, user_id: &str) -> Result<()> {
        if !self.config.enabled {
            return Err(SecurityError::AuthorizationFailed {
                message: "RBAC is disabled".to_string(),
            });
        }

        self.admin_users.insert(user_id.to_string());
        debug!("Added admin user: {}", user_id);
        Ok(())
    }

    /// Remove admin user
    pub fn remove_admin(&mut self, user_id: &str) -> Result<()> {
        if !self.config.enabled {
            return Err(SecurityError::AuthorizationFailed {
                message: "RBAC is disabled".to_string(),
            });
        }

        if self.admin_users.remove(user_id) {
            debug!("Removed admin user: {}", user_id);
            Ok(())
        } else {
            Err(SecurityError::AuthorizationFailed {
                message: format!("User '{user_id}' is not an admin"),
            })
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// RBAC middleware for Axum - checks if user has required permission
pub fn require_permission(
    permission: &'static str,
) -> impl Fn(
    Request,
    Next,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = std::result::Result<Response, StatusCode>> + Send>,
> + Clone {
    move |request: Request, next: Next| {
        let required_permission = permission;
        Box::pin(async move {
            // Extract user claims from request
            if let Some(claims) = request.extensions().get::<Claims>() {
                // Check if user has required permission
                if claims
                    .permissions
                    .contains(&required_permission.to_string())
                    || claims.permissions.contains(&"*".to_string())
                    || claims.role == "admin"
                {
                    return Ok(next.run(request).await);
                } else {
                    warn!(
                        "Access denied for user '{}': missing permission '{}'",
                        claims.sub, required_permission
                    );
                    return Err(StatusCode::FORBIDDEN);
                }
            }

            // No authentication found
            warn!(
                "Access denied: no authentication found for permission '{}'",
                required_permission
            );
            Err(StatusCode::UNAUTHORIZED)
        })
    }
}

/// RBAC middleware for resource access
pub fn require_resource_access(
    resource: &'static str,
    action: &'static str,
) -> impl Fn(
    Request,
    Next,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = std::result::Result<Response, StatusCode>> + Send>,
> + Clone {
    move |request: Request, next: Next| {
        let required_resource = resource;
        let required_action = action;
        Box::pin(async move {
            if let Some(claims) = request.extensions().get::<Claims>() {
                let permission = format!("{required_resource}:{required_action}");
                let wildcard = format!("{required_resource}:*");

                if claims.permissions.contains(&permission)
                    || claims.permissions.contains(&wildcard)
                    || claims.permissions.contains(&"*".to_string())
                    || claims.role == "admin"
                {
                    return Ok(next.run(request).await);
                } else {
                    warn!(
                        "Access denied for user '{}': cannot {} on {}",
                        claims.sub, required_action, required_resource
                    );
                    return Err(StatusCode::FORBIDDEN);
                }
            }

            warn!(
                "Access denied: no authentication found for {}:{}",
                required_resource, required_action
            );
            Err(StatusCode::UNAUTHORIZED)
        })
    }
}

/// RBAC middleware with State
pub async fn rbac_middleware(
    State(rbac): State<Arc<RbacManager>>,
    request: Request,
    next: Next,
) -> std::result::Result<Response, StatusCode> {
    if !rbac.is_enabled() {
        return Ok(next.run(request).await);
    }

    // For general middleware, we just pass through if authenticated
    // Specific permission checks should be done with require_permission
    if request.extensions().get::<Claims>().is_some() {
        Ok(next.run(request).await)
    } else {
        warn!("Access denied: no authentication found");
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_rbac() -> RbacManager {
        let mut roles = HashMap::new();
        roles.insert("user".to_string(), vec!["memory:read".to_string()]);
        roles.insert(
            "admin".to_string(),
            vec![
                "memory:read".to_string(),
                "memory:write".to_string(),
                "memory:delete".to_string(),
            ],
        );

        let config = RbacConfig {
            enabled: true,
            default_role: "user".to_string(),
            roles,
            admin_users: vec!["admin@example.com".to_string()],
        };

        RbacManager::new(config)
    }

    #[test]
    fn test_rbac_manager_creation() {
        let rbac = create_test_rbac();
        assert!(rbac.is_enabled());
        assert_eq!(rbac.role_permissions.len(), 2);
        assert_eq!(rbac.admin_users.len(), 1);
    }

    #[test]
    fn test_permission_check_admin() {
        let rbac = create_test_rbac();

        let check = rbac.check_permission("admin@example.com", "memory:delete");
        assert!(check.allowed);
        assert_eq!(check.user_role, "admin");
        assert!(check.reason.contains("Admin override"));
    }

    #[test]
    fn test_permission_check_user() {
        let mut rbac = create_test_rbac();
        rbac.assign_role("user@example.com", "user").unwrap();

        // User should have read permission
        let read_check = rbac.check_permission("user@example.com", "memory:read");
        assert!(read_check.allowed);
        assert_eq!(read_check.user_role, "user");

        // User should not have write permission
        let write_check = rbac.check_permission("user@example.com", "memory:write");
        assert!(!write_check.allowed);
        assert!(write_check.reason.contains("Missing required permission"));
    }

    #[test]
    fn test_resource_access_check() {
        let mut rbac = create_test_rbac();
        rbac.assign_role("user@example.com", "user").unwrap();

        let check = rbac.check_resource_access("user@example.com", "memory", "read");
        assert!(check.allowed);

        let check = rbac.check_resource_access("user@example.com", "memory", "write");
        assert!(!check.allowed);
    }

    #[test]
    fn test_multiple_permissions_check() {
        let mut rbac = create_test_rbac();
        rbac.assign_role("admin@example.com", "admin").unwrap();

        // Admin should have all permissions
        let check = rbac.check_permissions("admin@example.com", &["memory:read", "memory:write"]);
        assert!(check.allowed);

        rbac.assign_role("user@example.com", "user").unwrap();

        // User should not have all permissions
        let check = rbac.check_permissions("user@example.com", &["memory:read", "memory:write"]);
        assert!(!check.allowed);
    }

    #[test]
    fn test_role_assignment() {
        let mut rbac = create_test_rbac();

        // Assign role
        let result = rbac.assign_role("test@example.com", "user");
        assert!(result.is_ok());

        // Check role was assigned
        assert_eq!(rbac.get_user_role("test@example.com"), "user");

        // Try to assign non-existent role
        let result = rbac.assign_role("test@example.com", "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_role_creation_and_deletion() {
        let mut rbac = create_test_rbac();

        // Create new role
        let result = rbac.create_role(
            "moderator",
            vec!["memory:read".to_string(), "memory:moderate".to_string()],
        );
        assert!(result.is_ok());

        // Check role exists
        assert!(rbac.role_permissions.contains_key("moderator"));

        // Delete role
        let result = rbac.delete_role("moderator");
        assert!(result.is_ok());

        // Check role is gone
        assert!(!rbac.role_permissions.contains_key("moderator"));

        // Try to delete built-in role
        let result = rbac.delete_role("admin");
        assert!(result.is_err());
    }

    #[test]
    fn test_permission_management() {
        let mut rbac = create_test_rbac();

        // Add permission to role
        let result = rbac.add_permission_to_role("user", "memory:moderate");
        assert!(result.is_ok());

        // Check permission was added
        let permissions = rbac.role_permissions.get("user").unwrap();
        assert!(permissions.contains("memory:moderate"));

        // Remove permission from role
        let result = rbac.remove_permission_from_role("user", "memory:moderate");
        assert!(result.is_ok());

        // Check permission was removed
        let permissions = rbac.role_permissions.get("user").unwrap();
        assert!(!permissions.contains("memory:moderate"));
    }

    #[test]
    fn test_admin_management() {
        let mut rbac = create_test_rbac();

        // Check initial admin
        assert!(rbac.is_admin("admin@example.com"));

        // Add new admin
        let result = rbac.add_admin("newadmin@example.com");
        assert!(result.is_ok());
        assert!(rbac.is_admin("newadmin@example.com"));

        // Remove admin
        let result = rbac.remove_admin("newadmin@example.com");
        assert!(result.is_ok());
        assert!(!rbac.is_admin("newadmin@example.com"));

        // Try to remove non-existent admin
        let result = rbac.remove_admin("notadmin@example.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_default_role() {
        let rbac = create_test_rbac();

        // User with no assigned role should get default role
        let role = rbac.get_user_role("unknown@example.com");
        assert_eq!(role, "user");
    }

    #[test]
    fn test_disabled_rbac() {
        let config = RbacConfig {
            enabled: false,
            default_role: "user".to_string(),
            roles: HashMap::new(),
            admin_users: Vec::new(),
        };

        let rbac = RbacManager::new(config);
        assert!(!rbac.is_enabled());

        // Should allow all permissions when disabled
        let check = rbac.check_permission("anyone", "anything");
        assert!(check.allowed);
        assert!(check.reason.contains("RBAC disabled"));
    }

    #[test]
    fn test_wildcard_permissions() {
        let mut rbac = create_test_rbac();

        // Add wildcard permission to user role
        rbac.add_permission_to_role("user", "memory:*").unwrap();
        rbac.assign_role("user@example.com", "user").unwrap();

        // Should allow any action on memory resource
        let check = rbac.check_resource_access("user@example.com", "memory", "write");
        assert!(check.allowed);
        assert!(check.reason.contains("Wildcard permission"));

        let check = rbac.check_resource_access("user@example.com", "memory", "delete");
        assert!(check.allowed);
    }

    #[test]
    fn test_get_roles_and_users() {
        let mut rbac = create_test_rbac();
        rbac.assign_role("user1@example.com", "user").unwrap();
        rbac.assign_role("user2@example.com", "admin").unwrap();

        let roles = rbac.get_roles();
        assert_eq!(roles.len(), 2);
        assert!(roles.contains_key("user"));
        assert!(roles.contains_key("admin"));

        let user_roles = rbac.get_user_roles();
        assert!(user_roles.len() >= 3); // At least 3 users (2 assigned + 1 admin)
        assert_eq!(
            user_roles.get("admin@example.com"),
            Some(&"admin".to_string())
        );
    }
}
