use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub struct TokenizedInput {
    pub input_ids: Vec<i64>,
    pub attention_mask: Vec<i64>,
    pub token_type_ids: Vec<i64>,
}

pub struct WordPieceTokenizer {
    vocab: HashMap<String, i64>,
    unk_id: i64,
    cls_id: i64,
    sep_id: i64,
    pad_id: i64,
}

impl WordPieceTokenizer {
    pub fn new(vocab_path: &Path) -> Result<Self, String> {
        let file = File::open(vocab_path).map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);
        let mut vocab = HashMap::new();

        for line_res in reader.lines() {
            let line = line_res.map_err(|e| e.to_string())?;
            let trimmed = line.trim().to_string();
            if !trimmed.is_empty() {
                let id = vocab.len() as i64;
                vocab.insert(trimmed, id);
            }
        }

        let unk_id = *vocab.get("[UNK]").unwrap_or(&100);
        let cls_id = *vocab.get("[CLS]").unwrap_or(&101);
        let sep_id = *vocab.get("[SEP]").unwrap_or(&102);
        let pad_id = *vocab.get("[PAD]").unwrap_or(&0);

        Ok(Self {
            vocab,
            unk_id,
            cls_id,
            sep_id,
            pad_id,
        })
    }

    pub fn tokenize_to_ids(&self, text: &str, max_len: usize) -> TokenizedInput {
        let words = tokenize_to_words(&text.to_lowercase());
        let mut input_ids = Vec::new();

        input_ids.push(self.cls_id);

        for word in words {
            if word.is_empty() {
                continue;
            }
            if word.len() > 100 {
                input_ids.push(self.unk_id);
                continue;
            }
            let mut word_tokens = Vec::new();
            let mut start = 0;
            let mut is_bad = false;
            let chars: Vec<char> = word.chars().collect();
            let len = chars.len();

            while start < len {
                let mut end = len;
                let mut cur_substr_id = None;
                while start < end {
                    let raw_sub: String = chars[start..end].iter().collect();
                    let substr = if start == 0 {
                        raw_sub
                    } else {
                        format!("##{}", raw_sub)
                    };
                    if let Some(&id) = self.vocab.get(&substr) {
                        cur_substr_id = Some(id);
                        break;
                    }
                    end -= 1;
                }
                if let Some(id) = cur_substr_id {
                    word_tokens.push(id);
                    start = end;
                } else {
                    is_bad = true;
                    break;
                }
            }

            if is_bad {
                input_ids.push(self.unk_id);
            } else {
                input_ids.extend(word_tokens);
            }
        }

        // Truncate if too long (leave space for [SEP])
        let mut final_input_ids = if input_ids.len() > max_len - 1 {
            input_ids[0..max_len - 1].to_vec()
        } else {
            input_ids
        };
        final_input_ids.push(self.sep_id);

        let mut attention_mask = vec![1i64; final_input_ids.len()];

        // Padding to max_len
        while final_input_ids.len() < max_len {
            final_input_ids.push(self.pad_id);
            attention_mask.push(0i64);
        }

        let token_type_ids = vec![0i64; max_len];

        TokenizedInput {
            input_ids: final_input_ids,
            attention_mask,
            token_type_ids,
        }
    }
}

fn tokenize_to_words(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_alphanumeric() {
            let start = i;
            while i < chars.len() && chars[i].is_alphanumeric() {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            result.push(word);
        } else {
            result.push(c.to_string());
            i += 1;
        }
    }
    result
}
