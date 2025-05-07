#[derive(serde::Serialize, serde::Deserialize,Debug,Clone)]
pub struct Member {
    pub id: Option<u32>,
    pub user_id: Option<u32>,
    pub skin_id: Option<u32>,
    pub discord: Option<String>,
    pub is_admin: Option<bool>,
}