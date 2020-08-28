#![allow(dead_code)]

use std::collections::HashMap;
use unic_ucd_category::GeneralCategory;

pub fn tokens_iter(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c| !is_token_charcter(c)).filter(|t| !t.is_empty())
}

pub fn is_token_charcter(c: char) -> bool {
    let category = GeneralCategory::of(c);
    category.is_number() || category.is_letter() || category == GeneralCategory::PrivateUse
}

const FTS_FREQUENCY_POSTFIX: &[u8] = b"_frequency";
const FTS_TOKENS_POSTFIX: &[u8] = b"_tokens";
const FTS_DOCLEN_POSTIFX: &[u8] = b"_doclen";

pub struct FTSTree {
    frequency: sled::Tree,
    tokens: sled::Tree,
    doclen: sled::Tree,
}

pub trait FTSExt {
    fn open_fts<V: AsRef<[u8]>>(&self, name: V) -> sled::Result<FTSTree>;
}

impl FTSExt for sled::Db {
    fn open_fts<V: AsRef<[u8]>>(&self, name: V) -> sled::Result<FTSTree> {
        let name_ref = name.as_ref();

        let mut frequency_name = name_ref.to_vec();
        frequency_name.extend_from_slice(FTS_FREQUENCY_POSTFIX);
        let frequency = self.open_tree(frequency_name)?;

        let mut tokens_name = name_ref.to_vec();
        tokens_name.extend_from_slice(FTS_TOKENS_POSTFIX);
        let tokens = self.open_tree(tokens_name)?;

        let mut doclen_name = name_ref.to_vec();
        doclen_name.extend_from_slice(FTS_DOCLEN_POSTIFX);
        let doclen = self.open_tree(doclen_name)?;

        Ok(FTSTree {
            frequency,
            tokens,
            doclen,
        })
    }
}

impl FTSTree {
    pub fn insert<K: AsRef<[u8]>>(&self, key: K, value: &str) -> sled::Result<()> {
        assert_eq!(key.as_ref().len(), 0);
        use sled::Transactional;
        use std::convert::TryFrom;
        let mut token_counts: HashMap<&str, u32> = HashMap::new();
        let mut total_count = 0u32;
        for token in tokens_iter(value) {
            *token_counts.entry(token).or_insert(0) += 1;
            total_count += 1;
        }
        token_counts.insert("", 1);
        (&self.frequency, &self.tokens, &self.doclen)
            .transaction(move |(frequency, tokens, doclen)| {
                // TODO: Enable updates
                if let Some(_old_total_count) =
                    doclen.insert(key.as_ref(), total_count.to_le_bytes().as_ref())?
                {
                    return Err(sled::Error::Unsupported(
                        "Updates to FTSTree are not allowed".to_owned(),
                    )
                    .into());
                }
                let old_total_dl = doclen
                    .get(&[])?
                    .map(|dl| u32::from_le_bytes(TryFrom::try_from(dl.as_ref()).unwrap()))
                    .unwrap_or(0);
                doclen.insert(&[], (old_total_dl + total_count).to_le_bytes().as_ref())?;
                for (token, count) in token_counts.iter() {
                    let (id, old_count) = if let Some(old) = tokens.get(token)? {
                        let old_count = u32::from_le_bytes(TryFrom::try_from(&old[0..4]).unwrap());
                        let id = u64::from_le_bytes(TryFrom::try_from(&old[4..12]).unwrap());
                        (id, old_count)
                    } else {
                        let id = tokens.generate_id()?;
                        (id, 0)
                    };
                    let mut frequency_key = id.to_le_bytes().as_ref().to_vec();
                    frequency_key.extend_from_slice(key.as_ref());
                    if let Some(_) =
                        frequency.insert(frequency_key, count.to_le_bytes().as_ref())?
                    {
                        unreachable!();
                    }
                    let mut new = (old_count + count).to_le_bytes().as_ref().to_vec();
                    new.extend_from_slice(id.to_le_bytes().as_ref());
                    tokens.insert(*token, new)?;
                }
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError<()>| match e {
                sled::transaction::TransactionError::Storage(s) => s,
                _ => unreachable!(),
            })
    }

    pub fn remove<K: AsRef<[u8]>>(&self, key: K, value: &str) -> sled::Result<()> {
        assert_eq!(key.as_ref().len(), 0);
        use sled::Transactional;
        use std::convert::TryFrom;
        let mut token_counts: HashMap<&str, u32> = HashMap::new();
        let mut total_count = 0u32;
        for token in tokens_iter(value) {
            *token_counts.entry(token).or_insert(0) += 1;
            total_count += 1;
        }
        token_counts.insert("", 1);
        (&self.frequency, &self.tokens, &self.doclen)
            .transaction(move |(frequency, tokens, doclen)| {
                let old_total_count = doclen
                    .remove(key.as_ref())?
                    .ok_or(sled::transaction::ConflictableTransactionError::Abort(()))?;
                if old_total_count.as_ref() != total_count.to_le_bytes().as_ref() {
                    return Err(sled::Error::Unsupported(
                        "value does not match inserted document".to_owned(),
                    )
                    .into());
                }
                let old_total_dl = doclen
                    .get(&[])?
                    .map(|dl| u32::from_le_bytes(TryFrom::try_from(dl.as_ref()).unwrap()))
                    .unwrap_or(0);
                doclen.insert(&[], (old_total_dl - total_count).to_le_bytes().as_ref())?;
                for (token, count) in token_counts.iter() {
                    let (id, old_count) = if let Some(old) = tokens.get(token)? {
                        let old_count = u32::from_le_bytes(TryFrom::try_from(&old[0..4]).unwrap());
                        let id = u64::from_le_bytes(TryFrom::try_from(&old[4..12]).unwrap());
                        (id, old_count)
                    } else {
                        return Err(sled::Error::Unsupported(
                            "value does not match inserted document".to_owned(),
                        )
                        .into());
                    };
                    let mut frequency_key = id.to_le_bytes().as_ref().to_vec();
                    frequency_key.extend_from_slice(key.as_ref());
                    let old_frequency =
                        frequency
                            .remove(frequency_key)?
                            .ok_or(sled::Error::Unsupported(
                                "value does not match inserted document".to_owned(),
                            ))?;
                    if old_frequency.as_ref() != count.to_le_bytes().as_ref() {
                        return Err(sled::Error::Unsupported(
                            "value does not match inserted document".to_owned(),
                        )
                        .into());
                    }
                    let mut new = (old_count - count).to_le_bytes().as_ref().to_vec();
                    new.extend_from_slice(id.to_le_bytes().as_ref());
                    tokens.insert(*token, new)?;
                }
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError<()>| match e {
                sled::transaction::TransactionError::Storage(s) => s,
                _ => unreachable!(),
            })
    }

    pub fn query(&self, value: &str) -> sled::Result<HashMap<sled::IVec, f32>> {
        use std::convert::TryFrom;
        let mut token_counts: HashMap<&str, u32> = HashMap::new();
        for token in tokens_iter(value) {
            *token_counts.entry(token).or_insert(0) += 1;
        }

        let mut ret = HashMap::new();

        let num_documents = self
            .tokens
            .get("")?
            .map(|data| u32::from_le_bytes(TryFrom::try_from(&data[0..4]).unwrap()))
            .unwrap_or(0);

        let total_dl = self
            .doclen
            .get(&[])?
            .map(|dl| u32::from_le_bytes(TryFrom::try_from(dl.as_ref()).unwrap()))
            .unwrap_or(0);
        let avgdl = total_dl as f32 / num_documents as f32;
        for (token, count) in token_counts {
            if let Some(token_data) = self.tokens.get(token)? {
                let total_count = u32::from_le_bytes(TryFrom::try_from(&token_data[0..4]).unwrap());
                let id = &token_data[4..12];
                for frequency_data_result in self.frequency.scan_prefix(id) {
                    let (id_and_key, frequency_data) = frequency_data_result?;
                    let frequency =
                        u32::from_le_bytes(TryFrom::try_from(frequency_data.as_ref()).unwrap());
                    let key = sled::IVec::from(&id_and_key[8..]);

                    let k1 = 1.2;
                    let b = 0.75;
                    let idf = ((num_documents as f32 - total_count as f32 + 0.5)
                        / (total_count as f32 + 0.5)
                        + 1.0)
                        .ln();
                    // TODO: Handle missing dl properly
                    let dl = self
                        .doclen
                        .get(&key)?
                        .map(|dl| u32::from_le_bytes(TryFrom::try_from(dl.as_ref()).unwrap()))
                        .unwrap_or(0);
                    let bm25 = idf * frequency as f32 * (k1 + 1.0)
                        / (frequency as f32 + k1 * (1.0 - b + b * dl as f32 / avgdl as f32));
                    *ret.entry(key).or_insert(0.0) += bm25 * count as f32;
                }
            }
        }

        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let fts_tree = db.open_fts("test").unwrap();
        fts_tree.insert(b"k1", "foo bar").unwrap();
        fts_tree.insert(b"k2", "foo").unwrap();
        fts_tree.insert(b"k3", "bar").unwrap();
        let res = fts_tree.query("foo").unwrap();
        assert_eq!(res.get(&sled::IVec::from(b"k1")), Some(&0.3901917));
        assert_eq!(res.get(&sled::IVec::from(b"k2")), Some(&0.52354836));
        assert_eq!(res.get(&sled::IVec::from(b"k3")), None);
        let res = fts_tree.query("foo bar").unwrap();
        assert_eq!(res.get(&sled::IVec::from(b"k1")), Some(&0.7803834));
        assert_eq!(res.get(&sled::IVec::from(b"k2")), Some(&0.52354836));
        assert_eq!(res.get(&sled::IVec::from(b"k3")), Some(&0.52354836));
    }

    #[test]
    fn delete() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let fts_tree = db.open_fts("test").unwrap();
        fts_tree.insert(b"k1", "foo bar").unwrap();
        fts_tree.insert(b"k2", "foo").unwrap();
        let cs = db.checksum();
        fts_tree.insert(b"k3", "bar").unwrap();
        fts_tree.remove(b"k3", "bar").unwrap();
        assert_eq!(cs, db.checksum());
    }
}
