use crate::serialize::{Serialize, Deserialize, BitSerialize, BitDeserialize};
use crate::bit_io::{BitWriter, BitReader};
use std::time::Instant;
use std::collections::HashMap;
use std::io::Cursor;

#[derive(Debug, PartialEq, Serialize, Deserialize, BitSerialize, BitDeserialize)]
#[serialize_all]
pub struct Player {
    pub id: u32,
    pub name: String,
    #[bits(7)] // 0-100 range, fits in 7 bits
    pub health: u8,
    #[no_serialize]
    pub last_updated: Instant,
    pub inventory: Vec<Item>,
    #[bits(2)] // 3 variants, needs 2 bits
    pub status: PlayerStatus,
    pub attributes: HashMap<String, u16>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, BitSerialize, BitDeserialize)]
#[serialize_all]
pub struct Item {
    pub item_id: u16,
    pub quantity: u8,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, BitSerialize, BitDeserialize)]
pub enum PlayerStatus {
    Idle,
    Running,
    Attacking { target_id: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_serialization() {
        let player = Player {
            id: 42,
            name: "Hero".to_string(),
            health: 100,
            last_updated: Instant::now(),
            inventory: vec![Item { item_id: 1, quantity: 5 }],
            status: PlayerStatus::Running,
            attributes: [("strength".to_string(), 10)].into_iter().collect(),
        };

        // Test regular serialization
        let mut buffer = Vec::new();
        player.serialize(&mut buffer).unwrap();

        let mut cursor = Cursor::new(buffer);
        let deserialized = Player::deserialize(&mut cursor).unwrap();

        assert_eq!(player.id, deserialized.id);
        assert_eq!(player.name, deserialized.name);
        assert_eq!(player.health, deserialized.health);
        assert_eq!(player.inventory, deserialized.inventory);
        assert_eq!(player.status, deserialized.status);
        assert_eq!(player.attributes, deserialized.attributes);

        // Test bit-packed serialization
        let mut bit_writer = BitWriter::new();
        player.bit_serialize(&mut bit_writer).unwrap();
        let bit_buffer = bit_writer.into_bytes();

        let mut bit_reader = BitReader::new(bit_buffer);
        let bit_deserialized = Player::bit_deserialize(&mut bit_reader).unwrap();

        assert_eq!(player.id, bit_deserialized.id);
        assert_eq!(player.name, bit_deserialized.name);
        assert_eq!(player.health, bit_deserialized.health);
        assert_eq!(player.inventory, bit_deserialized.inventory);
        assert_eq!(player.status, bit_deserialized.status);
        assert_eq!(player.attributes, bit_deserialized.attributes);
    }
}