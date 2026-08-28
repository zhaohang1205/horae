use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub category: String, // context | priority | custom
    pub is_system: bool,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub created_at: i64,
}
