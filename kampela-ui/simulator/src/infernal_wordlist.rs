use mnemonic_external::{
    error::ErrorWordList, regular::InternalWordList, AsWordList, Bits11, WordListElement, wordlist::WORDLIST_ENGLISH
};

use std::path::PathBuf;
use std::fs::File;
use crc32fast::Hasher as CRC32;


/**
 * This is a wasteful implementation of a wordlist,
 * it takes mnemonics from the internal wordlist
 * in case if no file has been provided,
 * or reads it from the file.
 */
#[derive(Debug)]
struct InfernalWordList {
    words: Option<[String; 2048]>,
}

impl InfernalWordList {
    pub fn new() -> Self {
        Self { words: None }
    }

    pub fn load_words(&mut self, buf: Vec<u8>) {
        let binding = String::from_utf8(buf)
            .expect("file is not utf8");
        let words = binding.split_whitespace();
        self.words = Some([const {String::new()}; 2048]);
        let Some(words_ref) = self.words.as_mut() else { panic!("Impossible, but okay!") };
        for (i, word) in words.enumerate() {
            words_ref[i] = word.to_string();
        }
    }
}

// impl AsWordList for InfernalWordList {
//     type Word = String;

//     fn get_word(&self, bits: Bits11) -> Result<Self::Word, ErrorWordList> {
//         // if self.words.is_none() {
//         let result = InternalWordList.get_word(bits);
//         if let Ok(word) = result {
//             return Ok(word);
//         } else {
//             return result;
//         }
        // }
        // let word_order = bits.bits() as usize;
        // return Ok(self.words[word_order]);
    // }

    // fn get_words_by_prefix(
    //     &self,
    //     prefix: &str,
    // ) -> Result<Vec<WordListElement<Self>>, ErrorWordList> {
    //     if self.words.is_none() {
    //         return InternalWordList.get_words_by_prefix(prefix);
    //     }
    //     // TODO: more efficient search of lower bound
    //     let mut result = Vec::new();
    //     for (i, word) in self.words.iter().enumerate() {
    //         if word.starts_with(prefix) {
    //             result.push(WordListElement {
    //                 word: word,
    //                 index: i,
    //             });
    //         }
    //     }
    //     return Ok(result);
    // }

    // fn bits11_for_word(&self, word: &str) -> Result<Bits11, ErrorWordList> {
    //     if self.words.is_none() {
    //         return InternalWordList.bits11_for_word(word);
    //     }
    //     let index = self.words.iter().position(|&x| x == word);
    //     match index {
    //         Some(i) => {
    //             return Ok(Bits11::new(i as u16));
    //         },
    //         None => {
    //             return Err(ErrorWordList::WordNotFound);
    //         },
    //     }
    // }
// }

#[cfg(test)] // Compile this module only during testing
mod tests {
    use super::*; // Import functions from the parent module

    #[test]
    fn test_read_mnemonic() {
        let mut wordlist = InfernalWordList::new();
        let mut buf = Vec::new();
        // Fill the buffer with words from static array in mnemonic_external, separate with spaces
        for word in WORDLIST_ENGLISH.iter() {
            buf.extend(word.as_bytes());
            buf.push(b' ');
        }
        // Check buffer length
        assert_eq!(buf.len(), 13116);
        // Calculate checksum of the buffer
        let mut hasher = CRC32::new();
        hasher.update(&buf);
        let checksum = hasher.finalize();
        // Check that the checksum is correct
        assert_eq!(checksum, 0x1543df86);
        // Load words from the buffer
        wordlist.load_words(buf);
        // Check that now words in the wordlist are not None
        assert!(wordlist.words.is_some());
        // Compare stored words with the original ones
        let words = wordlist.words.unwrap();
        for (i, word) in words.iter().enumerate() {
            assert_eq!(word, InternalWordList.get_word(Bits11::from(i as u16).expect("??? o.O")).expect("??? o.O"));
        }
    }
}
