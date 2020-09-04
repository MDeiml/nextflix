use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    pub username: String,
    pub password_hash: String,
    pub friends: HashMap<u64, FriendData>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FriendData {
    pub movies: Vec<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Movie {
    pub name: String,
}
