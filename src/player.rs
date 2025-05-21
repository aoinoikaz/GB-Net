use crate::serialize::{Serialize, Deserialize};
use std::time::Instant;
use std::collections::HashMap;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serialize_all]
pub struct Player {
    pub id: u32,
    pub name: String,
    pub position: [f32; 3],
    pub health: u8,
    #[no_serialize]
    pub last_updated: Instant,
    pub inventory: Vec<Item>,
    pub status: PlayerStatus,
    pub attributes: HashMap<String, u16>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serialize_all]
pub struct Item {
    pub item_id: u16,
    pub quantity: u8,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum PlayerStatus {
    Idle,
    Running,
    Attacking { target_id: u32 },
}