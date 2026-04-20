use crate::models::*;
use crate::parser;
use std::sync::OnceLock;

/// Embedded compressed data (gated by feature flag).
#[cfg(feature = "embedded-data")]
mod embedded {
    pub static DATA_ADJ: &[u8] = include_bytes!("../data/data.adj.zst");
    pub static DATA_NOUN: &[u8] = include_bytes!("../data/data.noun.zst");
    pub static DATA_VERB: &[u8] = include_bytes!("../data/data.verb.zst");
    pub static DATA_ADV: &[u8] = include_bytes!("../data/data.adv.zst");
    pub static INDEX_SENSE: &[u8] = include_bytes!("../data/index.sense.zst");
    pub static CNTLIST: &[u8] = include_bytes!("../data/cntlist.zst");
}

/// Decompress zstd bytes to string.
#[cfg(feature = "embedded-data")]
fn decompress(data: &[u8]) -> Result<String, String> {
    use std::io::Read;
    let mut decoder =
        ruzstd::StreamingDecoder::new(data).map_err(|e| format!("zstd init error: {e}"))?;
    let mut bytes = Vec::new();
    decoder
        .read_to_end(&mut bytes)
        .map_err(|e| format!("zstd decompress error: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("UTF-8 error: {e}"))
}

pub struct WordNet {
    index: OnceLock<Vec<(String, Vec<(POS, SynsetId)>)>>,
    freq_map: OnceLock<std::collections::HashMap<String, u32>>,
    adj: OnceLock<PosData>,
    noun: OnceLock<PosData>,
    verb: OnceLock<PosData>,
    adv: OnceLock<PosData>,
}

impl WordNet {
    #[cfg(feature = "embedded-data")]
    pub fn embedded() -> &'static Self {
        static INSTANCE: OnceLock<WordNet> = OnceLock::new();
        INSTANCE.get_or_init(|| WordNet {
            index: OnceLock::new(),
            freq_map: OnceLock::new(),
            adj: OnceLock::new(),
            noun: OnceLock::new(),
            verb: OnceLock::new(),
            adv: OnceLock::new(),
        })
    }

    fn freq_map(&self) -> &std::collections::HashMap<String, u32> {
        self.freq_map.get_or_init(|| {
            #[cfg(feature = "embedded-data")]
            {
                let cntlist_text = decompress(embedded::CNTLIST).unwrap_or_default();
                let index_text = decompress(embedded::INDEX_SENSE).unwrap_or_default();
                parser::build_freq_map(&cntlist_text, &index_text)
            }
            #[cfg(not(feature = "embedded-data"))]
            {
                std::collections::HashMap::new()
            }
        })
    }

    pub fn index(&self) -> &Vec<(String, Vec<(POS, SynsetId)>)> {
        self.index.get_or_init(|| {
            #[cfg(feature = "embedded-data")]
            {
                let text = decompress(embedded::INDEX_SENSE).unwrap_or_default();
                parser::parse_index_sense(&text)
            }
            #[cfg(not(feature = "embedded-data"))]
            {
                Vec::new()
            }
        })
    }

    fn load_pos_data(&self, pos: POS) -> &PosData {
        let lock = match pos {
            POS::Adj => &self.adj,
            POS::Noun => &self.noun,
            POS::Verb => &self.verb,
            POS::Adv => &self.adv,
        };
        lock.get_or_init(|| {
            #[cfg(feature = "embedded-data")]
            {
                let data = match pos {
                    POS::Adj => embedded::DATA_ADJ,
                    POS::Noun => embedded::DATA_NOUN,
                    POS::Verb => embedded::DATA_VERB,
                    POS::Adv => embedded::DATA_ADV,
                };
                let text = decompress(data).unwrap_or_default();
                let synsets = parser::parse_data_file(&text, self.freq_map());
                PosData::new(synsets)
            }
            #[cfg(not(feature = "embedded-data"))]
            {
                PosData::new(Vec::new())
            }
        })
    }

    /// Lookup a word in the index.
    pub fn lookup_word(&self, word: &str) -> Option<&Vec<(POS, SynsetId)>> {
        parser::index_lookup(self.index(), word)
    }

    /// Get a synset by POS and ID.
    pub fn get_synset(&self, pos: POS, id: SynsetId) -> Option<&Synset> {
        self.load_pos_data(pos).get(id)
    }

    /// Iterate all synsets for a given POS.
    pub fn all_synsets(&self, pos: POS) -> &[Synset] {
        &self.load_pos_data(pos).synsets
    }

    /// Iterate all synsets across all POS types.
    pub fn all_pos_synsets(&self) -> impl Iterator<Item = &Synset> {
        [POS::Adj, POS::Noun, POS::Verb, POS::Adv]
            .iter()
            .flat_map(|&pos| self.all_synsets(pos).iter())
    }
}

#[cfg(all(test, feature = "embedded-data"))]
mod tests {
    use super::*;

    #[test]
    fn test_wordnet_loads() {
        let wn = WordNet::embedded();
        let index = wn.index();
        assert!(!index.is_empty(), "word index should not be empty");
    }

    #[test]
    fn test_lookup_dog() {
        let wn = WordNet::embedded();
        let senses = wn.lookup_word("dog");
        assert!(senses.is_some(), "dog should be in the index");
        let senses = senses.unwrap();
        assert!(!senses.is_empty(), "dog should have at least one sense");
        // Verify we can load the synset
        let (pos, id) = senses[0];
        let synset = wn.get_synset(pos, id);
        assert!(synset.is_some(), "synset for dog should exist");
        let synset = synset.unwrap();
        assert!(!synset.definition.is_empty(), "definition should not be empty");
    }

    #[test]
    fn test_synset_counts() {
        let wn = WordNet::embedded();
        let adj_count = wn.all_synsets(POS::Adj).len();
        let noun_count = wn.all_synsets(POS::Noun).len();
        let verb_count = wn.all_synsets(POS::Verb).len();
        let adv_count = wn.all_synsets(POS::Adv).len();
        assert_eq!(adj_count, 18185, "adj synset count");
        assert_eq!(noun_count, 82192, "noun synset count");
        assert_eq!(verb_count, 13789, "verb synset count");
        assert_eq!(adv_count, 3625, "adv synset count");
        assert_eq!(adj_count + noun_count + verb_count + adv_count, 117791);
    }
}
