use crate::model::*;
use sled::transaction::{TransactionError, Transactional};

fn serialize_id(id: u64) -> [u8; 8] {
    id.to_le_bytes()
}

fn deserialize_id<V: AsRef<[u8]>>(id: V) -> u64 {
    use std::convert::TryInto;
    u64::from_le_bytes(id.as_ref().try_into().unwrap())
}

pub trait UserDb {
    type Error;
    fn add_user(&self, user: User) -> Result<Option<u64>, Self::Error>;
    fn get_user(&self, id: u64) -> Result<Option<User>, Self::Error>;
    fn get_user_by_username(&self, username: &str) -> Result<Option<(u64, User)>, Self::Error>;
}

const USERS: &'static [u8] = b"users";
const USERS_USERNAME: &'static [u8] = b"USERS_USERNAME";

impl UserDb for sled::Db {
    type Error = sled::Error;

    fn add_user(&self, user: User) -> sled::Result<Option<u64>> {
        let users = self.open_tree(USERS)?;
        let users_username = self.open_tree(USERS_USERNAME)?;
        let id = self.generate_id()?;
        if let Err(err) = (&users, &users_username).transaction(|(users, users_username)| {
            users.insert(&serialize_id(id), bincode::serialize(&user).unwrap())?;
            if let Some(_) = users_username.insert(user.username.as_bytes(), &serialize_id(id))? {
                sled::transaction::abort(())?;
            }
            Ok(())
        }) {
            match err {
                TransactionError::Storage(e) => return Err(e),
                TransactionError::Abort(_) => return Ok(None),
            };
        }
        Ok(Some(id))
    }

    fn get_user(&self, id: u64) -> sled::Result<Option<User>> {
        let users = self.open_tree(USERS)?;
        Ok(users
            .get(serialize_id(id))?
            .map(|d| bincode::deserialize(&d).unwrap()))
    }

    fn get_user_by_username(&self, username: &str) -> sled::Result<Option<(u64, User)>> {
        let users_username = self.open_tree(USERS_USERNAME)?;
        let users = self.open_tree(USERS)?;
        if let Some(id) = users_username.get(&username)? {
            let user =
                bincode::deserialize(&users.get(&id)?.expect("Bad index users_username")).unwrap();
            Ok(Some((deserialize_id(id), user)))
        } else {
            Ok(None)
        }
    }
}
