//! Per-game seat-to-identity mapping table.

use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SeatRow {
    pub game_id: Uuid,
    pub seat_index: i32,
    pub player_id: Uuid,
    pub user_id: Option<Uuid>,
    pub anon_user_id: Option<Uuid>,
    pub is_bot: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SeatOwner {
    pub user_id: Option<Uuid>,
    pub anon_user_id: Option<Uuid>,
    pub is_bot: bool,
}
